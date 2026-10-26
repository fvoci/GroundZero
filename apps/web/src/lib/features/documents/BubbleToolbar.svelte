<script lang="ts">
  // 글자를 고르면 뜨는 작은 서식 도구 막대(BubbleMenu 요소 안에 그린다). 블록 유형 바꾸기, 굵게·기울임·취소선·
  // 코드, 링크 넣기·고치기·해제·열기. 막대 안 입력란·선택 상자로 초점이 와도 막대는 그대로 남는다.
  import type { Editor } from '@tiptap/core';
  import { onMount } from 'svelte';

  import { createFormDraft } from '$lib/features/common/form-draft.svelte';
  import FormDraftOffer from '$lib/features/common/FormDraftOffer.svelte';
  import DiscardChangesConfirm from '$lib/features/common/DiscardChangesConfirm.svelte';
  import { m } from '$lib/paraglide/messages.js';
  import { Input } from '$lib/ui/input';

  import type { BlockKind } from './block-types';
  import { activeBlockKind, BLOCK_KINDS, blockKindLabel, setBlockKind } from './block-types';
  import { normalizeHref } from './link-href';

  interface Props {
    editor: Editor | undefined;
    version: number;
    draftName?: string | undefined;
  }

  const { editor, version, draftName }: Props = $props();

  type Mark = 'bold' | 'italic' | 'strike' | 'code';

  const info = $derived.by(() => {
    if (editor === undefined || version < 0) return undefined;
    return {
      kind: activeBlockKind(editor),
      marks: {
        bold: editor.isActive('bold'),
        italic: editor.isActive('italic'),
        strike: editor.isActive('strike'),
        code: editor.isActive('code'),
      } satisfies Record<Mark, boolean>,
      href: linkHref(editor),
    };
  });

  function linkHref(e: Editor): string {
    if (!e.isActive('link')) return '';
    const href: unknown = e.getAttributes('link').href;
    return typeof href === 'string' ? href : '';
  }

  let linkEditing = $state(false);
  let linkValue = $state('');
  let linkError = $state(false);
  let linkInput = $state<HTMLInputElement | null>(null);
  let linkBaseline = $state('');
  const linkDirty = $derived(linkValue !== linkBaseline);
  function resetLink(): void {
    linkEditing = false;
    linkValue = '';
    linkBaseline = '';
    linkFor = undefined;
    linkError = false;
  }
  const linkDraft = createFormDraft<string>({
    name: () => (draftName === undefined ? undefined : `editor-link:${draftName}`),
    snapshot: () => linkValue,
    parse: (value) => (typeof value === 'string' ? value : undefined),
    restore: (value) => {
      startLink();
      linkValue = value;
      linkBaseline = '';
    },
    reset: resetLink,
    dirty: () => linkDirty,
  });
  onMount(linkDraft.attach);
  // 링크 입력을 열면 주소 칸으로 초점을 옮긴다(조건부로 그려지는 칸에는 autofocus가 듣지 않는다).
  $effect(() => {
    if (linkEditing && linkInput !== null) {
      linkInput.focus();
      linkInput.select();
    }
  });

  // 링크 입력은 연 때의 선택에 붙는다. 선택이 바뀌면(다른 곳을 고르면) 닫는다.
  let linkFor: { from: number; to: number } | undefined;
  $effect(() => {
    if (version < 0 || editor === undefined || linkFor === undefined) return;
    const { from, to } = editor.state.selection;
    if (from !== linkFor.from || to !== linkFor.to) {
      linkEditing = false;
      linkFor = undefined;
    }
  });

  function toggle(mark: Mark): void {
    const chain = editor?.chain().focus();
    if (chain === undefined) return;
    switch (mark) {
      case 'bold':
        chain.toggleBold().run();
        break;
      case 'italic':
        chain.toggleItalic().run();
        break;
      case 'strike':
        chain.toggleStrike().run();
        break;
      case 'code':
        chain.toggleCode().run();
        break;
    }
  }

  function startLink(): void {
    if (editor !== undefined) {
      const { from, to } = editor.state.selection;
      linkFor = { from, to };
    }
    if (!linkDirty) {
      linkValue = info?.href ?? '';
      linkBaseline = linkValue;
    }
    linkError = false;
    linkEditing = true;
  }

  function applyLink(): void {
    if (editor?.isEditable !== true) return;
    const href = normalizeHref(linkValue);
    if (href === undefined) {
      linkError = true;
      return;
    }
    if (editor.chain().focus().extendMarkRange('link').setLink({ href }).run()) resetLink();
  }

  function removeLink(): void {
    if (editor?.isEditable !== true) return;
    linkDraft.leave.request(() => {
      editor.chain().focus().extendMarkRange('link').unsetLink().run();
      resetLink();
    });
  }

  const MARKS = $derived<{ mark: Mark; label: string; short: string; cls: string }[]>([
    { mark: 'bold', label: m.documents_tool_bold(), short: 'B', cls: 'font-bold' },
    { mark: 'italic', label: m.documents_tool_italic(), short: 'I', cls: 'italic' },
    { mark: 'strike', label: m.documents_tool_strike(), short: 'S', cls: 'line-through' },
    { mark: 'code', label: m.documents_tool_inline_code(), short: '</>', cls: 'font-mono text-xs' },
  ]);
</script>

{#if info !== undefined}
  {@const current = info}
  {#each linkDraft.offers as offer (offer.key)}
    <FormDraftOffer
      onrestore={() => {
        linkDraft.restore(offer);
      }}
      ondiscard={() => {
        linkDraft.discard(offer);
      }}
    />
  {/each}
  {#if linkEditing}
    <form
      class="flex items-center gap-1"
      onsubmit={(e: SubmitEvent) => {
        e.preventDefault();
        applyLink();
      }}
    >
      <!-- 공통 Input의 테두리·초점·잘못된 값 표시를 쓰고, 막대 안에 맞게 높이·폭·모서리·여백만 줄인다. -->
      <Input
        class="h-7 w-56 max-w-[60vw] rounded bg-background px-2"
        aria-label={m.documents_link_url()}
        aria-invalid={linkError}
        placeholder="https://"
        bind:value={linkValue}
        bind:ref={linkInput}
        onkeydown={(e: KeyboardEvent) => {
          if (e.key === 'Escape') {
            e.preventDefault();
            linkDraft.leave.request(() => {
              resetLink();
              editor?.commands.focus();
            });
          }
        }}
      />
      <button type="submit" class="h-7 rounded px-2 text-sm hover:bg-accent"
        >{m.documents_link_apply()}</button
      >
      {#if current.href !== ''}
        <button type="button" class="h-7 rounded px-2 text-sm hover:bg-accent" onclick={removeLink}
          >{m.documents_link_remove()}</button
        >
      {/if}
      {#if linkError}<span class="sr-only" role="alert">{m.documents_link_invalid()}</span>{/if}
    </form>
    {#if linkError}<p class="px-1 pt-1 text-xs text-destructive">
        {m.documents_link_invalid()}
      </p>{/if}
  {:else}
    <div class="flex items-center gap-0.5">
      <select
        class="h-7 rounded border-0 bg-transparent px-1 text-sm hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
        aria-label={m.documents_block_turn_into()}
        value={current.kind}
        onchange={(e: Event) => {
          if (editor === undefined || !(e.currentTarget instanceof HTMLSelectElement)) return;
          const value = e.currentTarget.value;
          const next = BLOCK_KINDS.find((k): k is BlockKind => k === value);
          if (next !== undefined) setBlockKind(editor, next);
        }}
      >
        {#each BLOCK_KINDS as kind (kind)}
          <option value={kind}>{blockKindLabel(kind)}</option>
        {/each}
      </select>
      <span class="mx-0.5 h-5 w-px bg-border" aria-hidden="true"></span>
      {#each MARKS as item (item.mark)}
        <button
          type="button"
          class={`flex h-7 min-w-7 items-center justify-center rounded px-1.5 text-sm hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none ${current.marks[item.mark] ? 'bg-accent text-accent-foreground' : ''}`}
          aria-label={item.label}
          aria-pressed={current.marks[item.mark]}
          title={item.label}
          onmousedown={(e: MouseEvent) => {
            e.preventDefault();
          }}
          onclick={() => {
            toggle(item.mark);
          }}><span class={item.cls} aria-hidden="true">{item.short}</span></button
        >
      {/each}
      <span class="mx-0.5 h-5 w-px bg-border" aria-hidden="true"></span>
      <button
        type="button"
        class={`h-7 rounded px-2 text-sm hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none ${current.href !== '' ? 'bg-accent text-accent-foreground underline' : ''}`}
        aria-pressed={current.href !== ''}
        onmousedown={(e: MouseEvent) => {
          e.preventDefault();
        }}
        onclick={startLink}>{m.documents_link()}</button
      >
      {#if current.href !== ''}
        <a
          class="h-7 rounded px-2 text-sm leading-7 hover:bg-accent"
          href={current.href}
          target="_blank"
          rel="noopener noreferrer">{m.documents_link_open()}</a
        >
      {/if}
    </div>
  {/if}
{/if}

<DiscardChangesConfirm
  open={linkDraft.leave.open}
  onkeep={linkDraft.leave.keep}
  ondiscard={linkDraft.discardAndLeave}
/>
