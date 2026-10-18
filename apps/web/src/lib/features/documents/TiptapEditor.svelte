<script lang="ts">
  // Tiptap 편집기 래퍼. 인스턴스는 마운트 때 한 번 만들고 해제 때 destroy한다.
  // 내용을 통째로 바꿀 때는 부모가 {#key}로 이 컴포넌트를 다시 만든다.
  // 모드 (D-27): 서식 편집(기본), Markdown 원문 편집(추가 입력 방식), 읽기 미리보기.
  // Markdown으로 바꿨다가 원문을 고치지 않고 돌아오면 원래 내용을 그대로 둔다. Markdown으로 다 표현하지 못하는
  // 내용이면 들어갈 때 알려 주고, 원문을 고친 경우에만 원문에서 다시 만든 내용으로 바꾼다(조용한 손실 방지).
  import type { Extensions } from '@tiptap/core';
  import { Editor } from '@tiptap/core';
  import Collaboration from '@tiptap/extension-collaboration';
  import CollaborationCaret from '@tiptap/extension-collaboration-caret';
  import { onMount, untrack } from 'svelte';

  import type { DocNode } from '$lib/api/client';
  import { parseDocNode } from '$lib/api/client';
  import type { ReferenceContext } from '$lib/features/references/context';
  import { ReferenceSuggestion } from '$lib/features/references/reference-extension';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';

  import { canonicalJson } from './drafts';
  import { schemaExtensions } from './editor-schema';
  import type { LiveSession } from './live';

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
    if (ctx === undefined || !canEdit) return base;
    return [
      ...base,
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
        },
      },
      onUpdate: ({ editor: e }) => {
        onchange?.(parseDocNode(e.getJSON()));
      },
      onTransaction: ({ editor: e }) => {
        marks = readMarks(e);
        inTable = e.isActive('table');
      },
    });
    editor = instance;
    return () => {
      instance.destroy();
      editor = undefined;
    };
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
    e.setEditable(next === 'rich');
    mode = next;
  }

  function onMarkdownInput(): void {
    const e = editor;
    if (e === undefined) return;
    const parsed = parsedMarkdown(e);
    // 자동 저장은 원문에서 만든 내용으로 한다. 편집기는 모드를 바꿀 때 맞춘다.
    if (parsed !== undefined) onchange?.(parsed);
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
          class="h-8 rounded-md border border-input bg-transparent px-2 font-mono"
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
    class="mt-1 min-h-[16rem] w-full rounded-md border border-input bg-transparent px-3 py-2 font-mono text-sm focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
  ></textarea>
{/if}
<div
  bind:this={element}
  class={`min-h-[12rem] py-3 ${mode === 'markdown' ? 'hidden' : ''}`}
  data-testid="editor"
></div>
