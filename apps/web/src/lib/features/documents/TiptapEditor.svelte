<script lang="ts">
  // Tiptap 편집기 래퍼. 인스턴스는 마운트 때 한 번 만들고 해제 때 destroy한다.
  // 내용을 통째로 바꿀 때는 부모가 {#key}로 이 컴포넌트를 다시 만든다.
  // 모드 (D-27): 서식 편집(기본), Markdown 원문 편집(추가 입력 방식), 읽기 미리보기.
  // Markdown으로 바꿨다가 원문을 고치지 않고 돌아오면 원래 내용을 그대로 둔다. Markdown으로 다 표현하지 못하는
  // 내용이면 들어갈 때 알려 주고, 원문을 고친 경우에만 원문에서 다시 만든 내용으로 바꾼다(조용한 손실 방지).
  import type { Extensions } from '@tiptap/core';
  import { Editor } from '@tiptap/core';
  import BubbleMenu from '@tiptap/extension-bubble-menu';
  import Collaboration from '@tiptap/extension-collaboration';
  import CollaborationCaret from '@tiptap/extension-collaboration-caret';
  import { Placeholder } from '@tiptap/extensions';
  import { TextSelection } from '@tiptap/pm/state';
  import { onMount, untrack } from 'svelte';

  import type { DocNode } from '$lib/api/client';
  import { parseDocNode } from '$lib/api/client';
  import type { ReferenceContext } from '$lib/features/references/context';
  import { ReferenceSuggestion } from '$lib/features/references/reference-extension';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';

  import BlockControls from './BlockControls.svelte';
  import { BlockKeys, insertDivider } from './block-ops';
  import {
    BLOCK_KINDS,
    blockKindHint,
    blockKindKeywords,
    blockKindLabel,
    setBlockKind,
  } from './block-types';
  import BubbleToolbar from './BubbleToolbar.svelte';
  import { canonicalJson } from './drafts';
  import { schemaExtensions } from './editor-schema';
  import type { LiveSession } from './live';
  import type { SlashItem } from './slash-command';
  import { SlashCommand } from './slash-command';
  import { describeAdjustments, describeMarkdownIssues } from './content-notices';
  import { contentIssues, PasteGuard, SafeImageInput } from './paste-guard';

  interface Props {
    content: DocNode;
    editable: boolean;
    label: string;
    onchange?: (body: DocNode) => void;
    /** 참조 노드 표시와 @ 입력. 없으면 참조를 읽기만 하고 이동·입력은 하지 않는다. */
    references?: ReferenceContext | undefined;
    /** 파일·이미지 넣기: 문서 첨부로 올리고 넣을 주소를 돌려준다. 없으면 단추를 보이지 않는다. */
    insertFile?:
      ((file: File) => Promise<{ href: string; name: string; image: boolean }>) | undefined;
    /** 실시간 공동 편집(D-70). 있으면 본문은 이 Yjs 문서가 맡고 `content`는 쓰지 않는다. */
    collab?: LiveSession | undefined;
  }

  const { content, editable, label, onchange, references, insertFile, collab }: Props = $props();

  type Command = 'bold' | 'italic' | 'heading' | 'bulletList' | 'taskList' | 'codeBlock';
  type Mode = 'rich' | 'markdown' | 'preview';

  let element: HTMLDivElement | undefined = $state();
  let editor: Editor | undefined;

  // 블록 손잡이·선택 도구 막대가 쓰는 편집기 참조와 상태 판(트랜잭션마다 늘어난다).
  let editorRef = $state.raw<Editor | undefined>(undefined);
  let version = $state(0);
  let container = $state<HTMLDivElement | undefined>();
  let bubbleEl = $state<HTMLDivElement | undefined>();
  let menuRequest = $state<number | undefined>(undefined);
  // 붙여넣기·입력에서 문서 규칙에 맞춰 바꾼 것의 안내, Markdown 원문에서 저장할 수 없는 것의 안내.
  let contentNotice = $state('');
  let markdownProblem = $state<string | undefined>(undefined);
  // 블록 단축키 안내(화면 낭독기가 편집기 설명으로 읽는다).
  const keysHintId = `editor-keys-${crypto.randomUUID()}`;

  /** / 메뉴 항목. 이 편집기에서 지금 할 수 있는 것만(첨부·참조는 기능이 있을 때). */
  function slashItems(): SlashItem[] {
    const blocks = BLOCK_KINDS.map((kind): SlashItem => ({
      id: kind,
      label: blockKindLabel(kind),
      hint: blockKindHint(kind),
      keywords: blockKindKeywords(kind),
      run: (e, range) => {
        e.chain().focus().deleteRange(range).run();
        setBlockKind(e, kind);
      },
    }));
    const extra: SlashItem[] = [
      {
        id: 'divider',
        label: m.documents_kind_divider(),
        hint: m.documents_hint_divider(),
        keywords: ['divider', 'hr', 'rule', 'line', '구분선', '---'],
        run: (e, range) => {
          insertDivider(e, range);
        },
      },
      {
        id: 'table',
        label: m.documents_kind_table(),
        hint: m.documents_hint_table(),
        keywords: ['table', 'grid', '표', '테이블'],
        run: (e, range) => {
          e.chain()
            .focus()
            .deleteRange(range)
            .insertTable({ rows: 3, cols: 3, withHeaderRow: true })
            .run();
        },
      },
      {
        id: 'math',
        label: m.documents_kind_math(),
        hint: m.documents_hint_math(),
        keywords: ['math', 'latex', 'equation', 'formula', '수식', '수학'],
        run: (e, range) => {
          e.chain().focus().deleteRange(range).run();
          openMath('block', '', undefined);
        },
      },
    ];
    if (insertFile !== undefined)
      extra.push({
        id: 'file',
        label: m.documents_kind_file(),
        hint: m.documents_hint_file(),
        keywords: ['file', 'image', 'upload', 'attach', '파일', '이미지', '첨부', '사진'],
        run: (e, range) => {
          e.chain().focus().deleteRange(range).run();
          fileInput?.click();
        },
      });
    if (references !== undefined)
      extra.push({
        id: 'reference',
        label: m.documents_kind_reference(),
        hint: m.documents_hint_reference(),
        keywords: ['mention', 'reference', 'link', 'page', '참조', '멘션', '언급', '@'],
        run: (e, range) => {
          // @ 입력으로 넘겨 기존 참조 찾기(ReferenceSuggestion)를 연다.
          e.chain().focus().deleteRange(range).insertContent('@').run();
        },
      });
    return [...blocks, ...extra];
  }
  let marks = $state<Record<Command, boolean>>({
    bold: false,
    italic: false,
    heading: false,
    bulletList: false,
    taskList: false,
    codeBlock: false,
  });
  let mode = $state<Mode>('rich');
  let markdownText = $state('');
  let markdownAtEntry = '';
  let lossy = $state(false);

  // 표·수식·파일(D-27).
  let inTable = $state(false);
  let math = $state<
    { kind: 'inline' | 'block'; latex: string; pos: number | undefined } | undefined
  >(undefined);
  let fileInput = $state<HTMLInputElement | undefined>();
  let fileStatus = $state('');

  function openMath(kind: 'inline' | 'block', latex: string, pos: number | undefined): void {
    if (editor?.isEditable !== true) return;
    math = { kind, latex, pos };
  }

  function applyMath(): void {
    const e = editor;
    const m = math;
    if (e === undefined || m === undefined) return;
    const latex = m.latex.trim();
    const chain = e.chain().focus();
    if (latex === '') {
      if (m.pos !== undefined)
        (m.kind === 'inline'
          ? chain.deleteInlineMath({ pos: m.pos })
          : chain.deleteBlockMath({ pos: m.pos })
        ).run();
    } else if (m.pos === undefined) {
      (m.kind === 'inline'
        ? chain.insertInlineMath({ latex })
        : chain.insertBlockMath({ latex })
      ).run();
    } else {
      (m.kind === 'inline'
        ? chain.updateInlineMath({ latex, pos: m.pos })
        : chain.updateBlockMath({ latex, pos: m.pos })
      ).run();
    }
    math = undefined;
  }

  async function onFile(event: Event): Promise<void> {
    const input = event.currentTarget;
    if (!(input instanceof HTMLInputElement) || insertFile === undefined) return;
    const file = input.files?.[0];
    input.value = '';
    if (file === undefined) return;
    fileStatus = m.documents_file_uploading({ name: file.name });
    try {
      const added = await insertFile(file);
      const chain = editor?.chain().focus();
      if (added.image) chain?.setImage({ src: added.href, alt: added.name }).run();
      else
        chain
          ?.insertContent({
            type: 'text',
            text: added.name,
            marks: [{ type: 'link', attrs: { href: added.href } }],
          })
          .run();
      fileStatus = m.documents_file_inserted({ name: file.name });
    } catch (e: unknown) {
      fileStatus =
        e instanceof Error
          ? m.documents_file_insert_failed_reason({ reason: e.message })
          : m.documents_file_insert_failed();
    }
  }

  function table(
    action: 'insert' | 'row' | 'column' | 'deleteRow' | 'deleteColumn' | 'delete',
  ): void {
    const chain = editor?.chain().focus();
    if (chain === undefined) return;
    switch (action) {
      case 'insert':
        chain.insertTable({ rows: 3, cols: 3, withHeaderRow: true }).run();
        break;
      case 'row':
        chain.addRowAfter().run();
        break;
      case 'column':
        chain.addColumnAfter().run();
        break;
      case 'deleteRow':
        chain.deleteRow().run();
        break;
      case 'deleteColumn':
        chain.deleteColumn().run();
        break;
      case 'delete':
        chain.deleteTable().run();
        break;
    }
  }

  function readMarks(e: Editor): Record<Command, boolean> {
    return {
      bold: e.isActive('bold'),
      italic: e.isActive('italic'),
      heading: e.isActive('heading', { level: 2 }),
      bulletList: e.isActive('bulletList'),
      taskList: e.isActive('taskList'),
      codeBlock: e.isActive('codeBlock'),
    };
  }

  // 참조 노드는 항상 등록한다(모르는 노드는 Tiptap이 버리므로 읽기 화면에서도 필요하다).
  // @ 입력은 고칠 수 있고 참조 문맥이 있을 때만 켠다.
  function extensionsFor(
    ctx: ReferenceContext | undefined,
    canEdit: boolean,
    live: LiveSession | undefined,
  ): Extensions {
    const schema: Extensions = schemaExtensions({
      reference: {
        hrefFor: ctx?.hrefFor ?? (() => null),
        navigate: ctx?.navigate ?? (() => undefined),
      },
      onMathClick: (kind, node, pos) => {
        openMath(kind, String(node.attrs.latex ?? ''), pos);
      },
      collaborative: live !== undefined,
    });
    // 실시간이면 본문은 Yjs 문서(조각 `default`, 서버와 같은 이름)가 맡고 참여자 커서를 보인다.
    const base: Extensions =
      live === undefined
        ? schema
        : [
            ...schema,
            Collaboration.configure({ document: live.document, field: 'default' }),
            CollaborationCaret.configure({ provider: live.provider, user: live.user }),
          ];
    if (!canEdit) return base;
    // 고칠 때만: / 메뉴, 블록 단축키, 빈 줄 안내, 선택 도구 막대. 모두 장식·명령이고 문서 형식(스키마)은 그대로다.
    const editing: Extensions = [
      ...base,
      SlashCommand.configure({ items: () => slashItems() }),
      PasteGuard.configure({
        onAdjusted: (stats) => {
          queueMicrotask(() => {
            contentNotice = describeAdjustments(stats);
          });
        },
      }),
      SafeImageInput.configure({
        onLinked: () => {
          queueMicrotask(() => {
            contentNotice = m.documents_image_input_linked();
          });
        },
      }),
      BlockKeys.configure({
        onMenu: (pos: number) => {
          menuRequest = pos;
        },
      }),
      Placeholder.configure({
        placeholder: ({ editor: e, node }) =>
          node.type.name === 'heading'
            ? m.documents_placeholder_heading()
            : e.isEmpty
              ? m.documents_placeholder_empty()
              : m.documents_placeholder_block(),
      }),
    ];
    const bubble = bubbleEl;
    if (bubble !== undefined)
      editing.push(
        BubbleMenu.configure({
          element: bubble,
          options: {
            placement: 'top',
            strategy: 'fixed',
            offset: 8,
            flip: true,
            shift: { padding: 8 },
          },
          // 글자를 골랐을 때, 또는 링크 안에 커서가 있을 때. 코드 블록 안은 서식이 없으므로 띄우지 않는다.
          // 편집기(또는 막대 안 입력란)에 초점이 있을 때만. 다른 사람의 편집으로 문서가 바뀌어도 초점 없는 화면에 뜨지 않게.
          shouldShow: ({ editor: e, state }) =>
            e.isEditable &&
            (e.view.hasFocus() || bubble.contains(document.activeElement)) &&
            !e.isActive('codeBlock') &&
            ((!state.selection.empty && state.selection instanceof TextSelection) ||
              e.isActive('link')),
        }),
      );
    if (ctx === undefined) return editing;
    return [
      ...editing,
      ReferenceSuggestion.configure({
        workspaceId: ctx.workspaceId,
        describeWorkspace: ctx.describeWorkspace,
      }),
    ];
  }

  onMount(() => {
    if (element === undefined) return;
    const instance = new Editor({
      element,
      extensions: untrack(() => extensionsFor(references, editable, collab)),
      // 실시간이면 처음 내용은 서버의 Yjs 문서에서 온다.
      content: untrack(() => (collab === undefined ? { ...content } : null)),
      editable: untrack(() => editable),
      editorProps: {
        attributes: {
          class: 'fvoci-prose',
          role: 'textbox',
          'aria-multiline': 'true',
          'aria-label': untrack(() => label),
          ...(untrack(() => editable) ? { 'aria-describedby': keysHintId } : {}),
        },
      },
      onUpdate: ({ editor: e }) => {
        onchange?.(parseDocNode(e.getJSON()));
      },
      onTransaction: ({ editor: e }) => {
        // 초점이 옮겨 갈 때(메뉴 열기 등) 화면을 그리는 도중에도 트랜잭션이 올 수 있으므로 상태는 다음 틈에 고친다.
        queueMicrotask(() => {
          version += 1;
          marks = readMarks(e);
          inTable = e.isActive('table');
        });
      },
    });
    editor = instance;
    editorRef = instance;
    return () => {
      editorRef = undefined;
      instance.destroy();
      editor = undefined;
    };
  });

  // 편집 가능 여부가 뒤에 바뀌면 따른다(실시간 연결이 권한·내용 문제로 멈추면 더 고칠 수 없게 한다).
  // 나머지 설정(확장·도움말)은 처음 값으로 둔다.
  $effect(() => {
    const instance = editorRef;
    if (instance !== undefined && instance.isEditable !== editable)
      instance.setEditable(editable, false);
  });

  function run(command: Command): void {
    const chain = editor?.chain().focus();
    if (chain === undefined) return;
    switch (command) {
      case 'bold':
        chain.toggleBold().run();
        break;
      case 'italic':
        chain.toggleItalic().run();
        break;
      case 'heading':
        chain.toggleHeading({ level: 2 }).run();
        break;
      case 'bulletList':
        chain.toggleBulletList().run();
        break;
      case 'taskList':
        chain.toggleTaskList().run();
        break;
      case 'codeBlock':
        chain.toggleCodeBlock().run();
        break;
    }
  }

  const tools = $derived<{ command: Command; label: string }[]>([
    { command: 'bold', label: m.documents_tool_bold() },
    { command: 'italic', label: m.documents_tool_italic() },
    { command: 'heading', label: m.documents_tool_heading() },
    { command: 'bulletList', label: m.documents_tool_bullet_list() },
    { command: 'taskList', label: m.documents_tool_task_list() },
    { command: 'codeBlock', label: m.documents_tool_code() },
  ]);

  /** 원문을 고쳤을 때만 원문에서 다시 만든 내용을 쓴다. */
  function parsedMarkdown(e: Editor): DocNode | undefined {
    if (markdownText === markdownAtEntry || e.markdown === undefined) return undefined;
    return parseDocNode(e.markdown.parse(markdownText));
  }

  function switchMode(next: Mode): void {
    const e = editor;
    if (e === undefined || next === mode) return;
    if (mode === 'markdown') {
      const parsed = parsedMarkdown(e);
      if (parsed !== undefined) e.commands.setContent(parsed, { emitUpdate: true });
    }
    if (next === 'markdown') {
      markdownText = e.getMarkdown();
      markdownAtEntry = markdownText;
      const roundTrip = e.markdown?.parse(markdownText);
      lossy = roundTrip === undefined || canonicalJson(roundTrip) !== canonicalJson(e.getJSON());
    }
    // 원문 아래 안내는 원문을 보는 동안 지금 내용 기준으로만 보인다(나갔다 오면 다시 계산).
    markdownProblem =
      next === 'markdown' ? describeMarkdownIssues(contentIssues(e.getJSON())) : undefined;
    e.setEditable(next === 'rich');
    mode = next;
  }

  function onMarkdownInput(): void {
    const e = editor;
    if (e === undefined) return;
    // 자동 저장은 원문에서 만든 내용으로 한다. 편집기는 모드를 바꿀 때 맞춘다. 원문을 들어올 때 그대로 되돌리면
    // 편집기 내용이 곧 원문이므로 그것으로 알리고 검사한다(중간에 고친 내용이 남지 않게).
    const current = parsedMarkdown(e) ?? parseDocNode(e.getJSON());
    markdownProblem = describeMarkdownIssues(contentIssues(current));
    onchange?.(current);
  }

  const MODES = $derived<{ mode: Mode; label: string }[]>([
    { mode: 'rich', label: m.documents_mode_rich() },
    { mode: 'markdown', label: m.documents_mode_markdown() },
    { mode: 'preview', label: m.documents_mode_preview() },
  ]);
</script>

{#if editable}
  <div class="flex flex-wrap items-center gap-1 border-b pb-2">
    <div class="flex gap-1" role="group" aria-label={m.documents_mode_label()}>
      {#each collab === undefined ? MODES : [] as option (option.mode)}
        <Button
          size="sm"
          variant={mode === option.mode ? 'secondary' : 'ghost'}
          aria-pressed={mode === option.mode}
          onclick={() => {
            switchMode(option.mode);
          }}>{option.label}</Button
        >
      {/each}
    </div>
    {#if mode === 'rich'}
      <div
        class="ml-2 flex flex-wrap gap-1 border-l pl-2"
        role="toolbar"
        aria-label={m.documents_format_toolbar()}
      >
        {#each tools as tool (tool.command)}
          <Button
            variant={marks[tool.command] ? 'secondary' : 'ghost'}
            size="sm"
            aria-pressed={marks[tool.command]}
            onclick={() => {
              run(tool.command);
            }}>{tool.label}</Button
          >
        {/each}
        <Button
          variant="ghost"
          size="sm"
          onclick={() => {
            table('insert');
          }}>{m.documents_table()}</Button
        >
        <Button
          variant="ghost"
          size="sm"
          onclick={() => {
            openMath('inline', '', undefined);
          }}>{m.documents_math()}</Button
        >
        {#if insertFile !== undefined}
          <Button
            variant="ghost"
            size="sm"
            onclick={() => {
              fileInput?.click();
            }}>{m.documents_insert_file()}</Button
          >
          <input
            bind:this={fileInput}
            type="file"
            class="hidden"
            aria-label={m.documents_file_input()}
            onchange={(e: Event) => void onFile(e)}
          />
        {/if}
      </div>
      {#if inTable}
        <div
          class="flex flex-wrap gap-1 border-l pl-2"
          role="toolbar"
          aria-label={m.documents_table()}
        >
          <Button
            variant="ghost"
            size="sm"
            onclick={() => {
              table('row');
            }}>{m.documents_table_add_row()}</Button
          >
          <Button
            variant="ghost"
            size="sm"
            onclick={() => {
              table('column');
            }}>{m.documents_table_add_column()}</Button
          >
          <Button
            variant="ghost"
            size="sm"
            onclick={() => {
              table('deleteRow');
            }}>{m.documents_table_delete_row()}</Button
          >
          <Button
            variant="ghost"
            size="sm"
            onclick={() => {
              table('deleteColumn');
            }}>{m.documents_table_delete_column()}</Button
          >
          <Button
            variant="ghost"
            size="sm"
            onclick={() => {
              table('delete');
            }}>{m.documents_table_delete()}</Button
          >
        </div>
      {/if}
    {/if}
  </div>
  {#if math !== undefined}
    {@const current = math}
    <form
      class="flex flex-wrap items-end gap-2 border-b py-2 text-sm"
      aria-label={m.documents_math()}
      onsubmit={(e: SubmitEvent) => {
        e.preventDefault();
        applyMath();
      }}
    >
      <label class="flex flex-1 flex-col gap-1">
        <span class="font-medium">{m.documents_math_latex()}</span>
        <input
          class="h-8 rounded-md border border-input bg-transparent px-2 font-code"
          bind:value={current.latex}
          placeholder={m.documents_math_placeholder()}
        />
      </label>
      {#if current.pos === undefined}
        <label class="flex items-center gap-1 pb-1.5">
          <input
            type="checkbox"
            checked={current.kind === 'block'}
            onchange={(e: Event) => {
              if (e.currentTarget instanceof HTMLInputElement)
                current.kind = e.currentTarget.checked ? 'block' : 'inline';
            }}
          />
          {m.documents_math_block()}
        </label>
      {/if}
      <Button type="submit" size="sm"
        >{current.pos === undefined
          ? m.documents_math_insert()
          : m.documents_math_replace()}</Button
      >
      <Button type="button" size="sm" variant="ghost" onclick={() => (math = undefined)}
        >{m.common_cancel()}</Button
      >
      {#if current.pos !== undefined}
        <span class="w-full text-xs text-muted-foreground">{m.documents_math_clear_hint()}</span>
      {/if}
    </form>
  {/if}
  {#if fileStatus !== ''}<p class="py-1 text-sm" aria-live="polite">{fileStatus}</p>{/if}
  <!-- 붙여넣기·입력을 문서 규칙에 맞춰 바꿨을 때의 안내. 사용자가 닫을 때까지 남긴다. -->
  <div class="text-sm" aria-live="polite">
    {#if contentNotice !== ''}
      <p
        class="mt-1 flex items-start gap-2 rounded-md border border-amber-300 bg-amber-50 px-3 py-2 text-amber-900 dark:border-amber-700 dark:bg-amber-950 dark:text-amber-200"
        data-testid="content-notice"
      >
        <span class="flex-1">{contentNotice}</span>
        <button
          type="button"
          class="shrink-0 rounded px-1 underline-offset-2 hover:underline focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
          onclick={() => {
            contentNotice = '';
          }}>{m.documents_notice_dismiss()}</button
        >
      </p>
    {/if}
  </div>
{/if}
{#if mode === 'markdown'}
  {#if lossy}
    <p class="mt-2 text-sm text-amber-700 dark:text-amber-400" role="note">
      {m.documents_markdown_lossy()}
    </p>
  {/if}
  <label class="mt-2 block text-sm font-medium" for="markdown-source"
    >{m.documents_markdown_source()}</label
  >
  <textarea
    id="markdown-source"
    bind:value={markdownText}
    oninput={onMarkdownInput}
    spellcheck="false"
    class="mt-1 min-h-[16rem] w-full rounded-md border border-input bg-transparent px-3 py-2 font-code text-sm focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
  ></textarea>
  {#if markdownProblem !== undefined}
    <p class="mt-1 text-sm text-destructive" role="alert" data-testid="markdown-problem">
      {markdownProblem}
    </p>
  {/if}
{/if}
<!-- 고칠 때는 왼쪽 여백에 블록 손잡이(+·메뉴)가 붙는다. 넓은 화면에서는 그 여백을 본문 바깥으로 뺀다. -->
<div
  bind:this={container}
  class={`relative ${editable && mode === 'rich' ? 'pl-12 lg:-ml-12' : ''} ${mode === 'markdown' ? 'hidden' : ''}`}
>
  {#if editable && mode === 'rich' && editorRef !== undefined && container !== undefined}
    <BlockControls
      editor={editorRef}
      {container}
      {version}
      {menuRequest}
      onmenuhandled={() => {
        menuRequest = undefined;
      }}
    />
  {/if}
  <div bind:this={element} class="min-h-[12rem] py-3" data-testid="editor"></div>
  {#if editable}<p id={keysHintId} class="sr-only">{m.documents_block_keys()}</p>{/if}
</div>
{#if editable}
  <!-- 선택 도구 막대: BubbleMenu가 보일 때 편집기 옆에 붙이고 숨길 때 떼어 낸다. -->
  <div
    bind:this={bubbleEl}
    class="z-30 rounded-md border bg-popover p-1 text-popover-foreground shadow-md"
    style="visibility: hidden; position: fixed; top: 0; left: 0"
    role="toolbar"
    aria-label={m.documents_bubble_label()}
    data-testid="bubble-toolbar"
  >
    <BubbleToolbar editor={editorRef} {version} />
  </div>
{/if}
