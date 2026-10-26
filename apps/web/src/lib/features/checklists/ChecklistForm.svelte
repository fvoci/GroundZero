<script lang="ts">
  // 편집기가 없는 화면의 체크리스트 (D-30, O-04). 항목의 원본은 서버의 행이며, 이 구성요소는 표시와 입력만 한다.
  // Markdown 붙여넣기: `- [x]`·`- [ ]` 줄을 항목으로 펼쳐 보여 주고, 사용자가 확인·수정한 뒤 한 번에 덧붙인다.
  import { Button } from '$lib/ui/button';
  import { Checkbox } from '$lib/ui/checkbox';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';
  import { Textarea } from '$lib/ui/textarea';
  import { onDestroy } from 'svelte';

  import { newRequestId } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import FormDraftOffer from '$lib/features/common/FormDraftOffer.svelte';
  import { createFormDraft } from '$lib/features/common/form-draft.svelte';
  import { accountSwitching } from '$lib/features/feedback/feedback.svelte';
  import { canonicalJson } from '$lib/features/documents/drafts';
  import { m } from '$lib/paraglide/messages.js';

  import type { ChecklistItem, NewChecklistEntry, ParsedChecklistItem } from './checklist-item';
  import { parseMarkdownChecklist } from './checklist-item';
  import { parseChecklistDraft, type ChecklistDraft } from './checklist-draft';

  interface Props {
    /** 화면 안에서 id가 겹치지 않게 붙이는 접두어. */
    idPrefix: string;
    items: ChecklistItem[];
    canEdit: boolean;
    onadd: (items: NewChecklistEntry[], editingUser: string) => Promise<void>;
    ontoggle: (id: string, done: boolean, editingUser: string) => Promise<void>;
    /** 항목 문구 바꾸기(완료 여부는 그대로). */
    onrename: (id: string, text: string, editingUser: string) => Promise<void>;
    onremove: (id: string, editingUser: string) => Promise<void>;
    onunsaved?: (value: boolean) => void;
  }

  const { idPrefix, items, canEdit, onadd, ontoggle, onrename, onremove, onunsaved }: Props =
    $props();

  /** 문구를 고치는 항목과 입력. */
  let renaming = $state<{ id: string; text: string; original: string } | undefined>(undefined);

  function saveRename(): void {
    const current = renaming;
    if (current === undefined) return;
    const text = current.text.trim();
    const before = items.find((i) => i.id === current.id)?.text;
    if (text === '' || text === before) {
      renaming = undefined;
      return;
    }
    void guard(async (user, isCurrent) => {
      await onrename(current.id, text, user);
      if (isCurrent()) renaming = undefined;
    });
  }

  let draft = $state('');
  let pasteOpen = $state(false);
  let pasted = $state('');
  let preview = $state<ParsedChecklistItem[]>([]);
  // 미리보기 줄의 재시도 id. 줄 자리가 아니라 내용(문구·완료)과 같은 내용 가운데 몇 번째인지에 붙인다: 고치지 않고
  // 다시 보내면 다시 펼치거나 줄 순서를 바꿔도 같은 id, 고친 줄은 새 id(같은 id면 서버가 처음 것을 남기고 고친 내용을
  // 버린다). 덧붙이기가 성공했을 때만 비운다.
  let previewIds = $state<Record<string, string[]>>({});
  // 한 줄 추가의 재시도용 요청 id. 입력이 바뀌면 새로 만든다.
  let draftRequest = $state<{ text: string; requestId: string } | undefined>();
  let busy = $state(false);
  let error = $state<string | undefined>(undefined);
  const dirty = $derived(
    draft !== '' ||
      pasted !== '' ||
      preview.length > 0 ||
      (renaming !== undefined && renaming.text !== renaming.original),
  );
  const formDraft = createFormDraft<ChecklistDraft>({
    name: () => `checklist:${idPrefix}`,
    snapshot: () => ({
      draft,
      draftRequest: draftRequest ?? null,
      renaming: renaming ?? null,
      pasteOpen,
      pasted,
      preview,
      previewIds,
    }),
    parse: parseChecklistDraft,
    restore: (value) => {
      draft = value.draft;
      draftRequest = value.draftRequest ?? undefined;
      renaming = value.renaming ?? undefined;
      pasteOpen = value.pasteOpen;
      pasted = value.pasted;
      preview = value.preview;
      previewIds = value.previewIds;
    },
    reset: () => {
      draft = '';
      draftRequest = undefined;
      renaming = undefined;
      pasteOpen = false;
      pasted = '';
      preview = [];
      previewIds = {};
      error = undefined;
      busy = false;
    },
    dirty: () => dirty,
    busy: () => busy,
  });
  $effect(() => {
    onunsaved?.(dirty && !accountSwitching());
  });
  onDestroy(() => onunsaved?.(false));

  async function guard(
    action: (user: string, isCurrent: () => boolean) => Promise<void>,
  ): Promise<void> {
    if (busy || !canEdit || accountSwitching()) return;
    const attempt = formDraft.capture();
    if (attempt === undefined) return;
    busy = true;
    error = undefined;
    try {
      await action(attempt.userId, () => formDraft.isCurrent(attempt));
    } catch (e: unknown) {
      if (formDraft.isOwner(attempt)) error = errorText(e, m.checklists_action_failed());
    } finally {
      if (formDraft.isOwner(attempt)) {
        busy = false;
        formDraft.persist();
      }
    }
  }

  function addOne(): void {
    const text = draft.trim();
    if (text === '') return;
    if (draftRequest?.text !== text) draftRequest = { text, requestId: newRequestId() };
    const requestId = draftRequest.requestId;
    void guard(async (user, isCurrent) => {
      await onadd([{ text, done: false, requestId }], user);
      if (isCurrent()) {
        draftRequest = undefined;
        draft = '';
      }
    });
  }

  function expand(): void {
    preview = parseMarkdownChecklist(pasted);
  }

  function confirmPaste(): void {
    const confirmed: NewChecklistEntry[] = [];
    const seen: Record<string, number> = {};
    for (const p of preview) {
      const entry = { text: p.text.trim(), done: p.done };
      if (entry.text === '') continue;
      const same = canonicalJson(entry);
      const nth = seen[same] ?? 0;
      seen[same] = nth + 1;
      const ids = previewIds[same] ?? [];
      const requestId = ids[nth] ?? newRequestId();
      ids[nth] = requestId;
      previewIds[same] = ids;
      confirmed.push({ ...entry, requestId });
    }
    if (confirmed.length === 0) return;
    void guard(async (user, isCurrent) => {
      await onadd(confirmed, user);
      if (isCurrent()) {
        pasted = '';
        preview = [];
        previewIds = {};
        pasteOpen = false;
      }
    });
  }

  const done = $derived(items.filter((i) => i.done).length);
</script>

<section aria-labelledby={`${idPrefix}-heading`} class="flex flex-col gap-3">
  <h3 id={`${idPrefix}-heading`} class="text-sm font-semibold">
    {m.checklists_heading()}
    <span class="font-normal text-muted-foreground">{done}/{items.length}</span>
  </h3>

  {#each formDraft.offers as offer (offer.key)}
    <div data-testid="checklist-new-item-draft-offer">
      <p class="text-sm whitespace-pre-wrap">
        {[
          offer.value.input.draft,
          offer.value.input.renaming?.text,
          offer.value.input.pasted,
          ...offer.value.input.preview.map((entry) => entry.text),
        ]
          .filter(Boolean)
          .join('\n')}
      </p>
      <FormDraftOffer
        onrestore={() => formDraft.restore(offer)}
        ondiscard={() => formDraft.discard(offer)}
      />
    </div>
  {/each}

  <ul class="flex flex-col gap-2" data-testid="checklist-items">
    {#each items as item (item.id)}
      <li class="flex items-center gap-2">
        <Checkbox
          id={`${idPrefix}-check-${item.id}`}
          checked={item.done}
          disabled={!canEdit || busy}
          onCheckedChange={(value) => void guard((user) => ontoggle(item.id, value === true, user))}
          aria-label={item.done
            ? m.checklists_item_done_aria({ text: item.text })
            : m.checklists_item_open_aria({ text: item.text })}
        />
        {#if canEdit && renaming?.id === item.id && !accountSwitching()}
          {@const editing = renaming}
          <form
            class="flex flex-1 items-center gap-2"
            onsubmit={(event) => {
              event.preventDefault();
              saveRename();
            }}
          >
            <Input
              bind:value={editing.text}
              aria-label={m.checklists_edit_label({ text: item.text })}
              autocomplete="off"
              autofocus
              onkeydown={(e: KeyboardEvent) => {
                if (e.key === 'Escape') {
                  e.preventDefault();
                  renaming = undefined;
                }
              }}
            />
            <Button type="submit" size="sm" disabled={busy}>{m.checklists_save()}</Button>
            <Button
              size="sm"
              variant="ghost"
              onclick={() => {
                renaming = undefined;
              }}>{m.checklists_cancel()}</Button
            >
          </form>
        {:else}
          <Label
            for={`${idPrefix}-check-${item.id}`}
            class={item.done ? 'text-muted-foreground line-through' : ''}>{item.text}</Label
          >
        {/if}
        {#if canEdit && renaming?.id !== item.id}
          <Button
            variant="ghost"
            size="sm"
            class="ml-auto"
            aria-label={m.checklists_edit_aria({ text: item.text })}
            disabled={busy || (renaming !== undefined && renaming.text !== renaming.original)}
            onclick={() => {
              renaming = { id: item.id, text: item.text, original: item.text };
            }}>{m.checklists_edit()}</Button
          >
          <Button
            variant="ghost"
            size="sm"
            aria-label={m.checklists_delete_aria({ text: item.text })}
            disabled={busy}
            onclick={() => void guard((user) => onremove(item.id, user))}
            >{m.checklists_delete()}</Button
          >
        {/if}
      </li>
    {:else}
      <li class="text-sm text-muted-foreground">{m.checklists_empty()}</li>
    {/each}
  </ul>

  {#if canEdit && !accountSwitching()}
    <!-- 375px·200%에서는 입력 칸과 두 단추가 한 줄에 들지 않아 줄바꿈한다(가로 넘침 금지, D-110). -->
    <form
      class="flex flex-wrap items-end gap-2"
      onsubmit={(event) => {
        event.preventDefault();
        addOne();
      }}
    >
      <div class="flex min-w-0 flex-1 basis-48 flex-col gap-1">
        <Label for={`${idPrefix}-new`}>{m.checklists_new_item()}</Label>
        <Input
          id={`${idPrefix}-new`}
          bind:value={draft}
          placeholder={m.checklists_new_item_placeholder()}
          autocomplete="off"
        />
      </div>
      <Button type="submit" disabled={busy || draft.trim() === ''}>{m.checklists_add()}</Button>
      <Button
        type="button"
        variant="outline"
        class="h-auto max-w-full whitespace-normal"
        onclick={() => (pasteOpen = !pasteOpen)}>{m.checklists_paste_markdown()}</Button
      >
    </form>
    {#if pasteOpen}
      <div class="flex flex-col gap-2 rounded-md border p-3">
        <Label for={`${idPrefix}-paste`}>{m.checklists_paste_label()}</Label>
        <Textarea
          id={`${idPrefix}-paste`}
          bind:value={pasted}
          rows={4}
          class="max-h-96 min-h-24 overflow-y-auto font-code"
        />
        <div>
          <Button size="sm" variant="outline" onclick={expand} disabled={pasted.trim() === ''}
            >{m.checklists_expand()}</Button
          >
        </div>
        {#if preview.length > 0}
          <p class="text-sm text-muted-foreground">{m.checklists_preview_hint()}</p>
          <ul class="flex flex-col gap-2" aria-label={m.checklists_preview_list()}>
            {#each preview as entry, index (index)}
              <li class="flex items-center gap-2">
                <Checkbox
                  checked={entry.done}
                  onCheckedChange={(value) => (preview[index] = { ...entry, done: value === true })}
                  aria-label={m.checklists_preview_done_aria({ text: entry.text })}
                />
                <Input
                  value={entry.text}
                  aria-label={m.checklists_preview_text_aria({ index: String(index + 1) })}
                  oninput={(e: Event) => {
                    const target = e.currentTarget;
                    if (target instanceof HTMLInputElement)
                      preview[index] = { ...entry, text: target.value };
                  }}
                />
              </li>
            {/each}
          </ul>
          <div>
            <Button size="sm" disabled={busy} onclick={confirmPaste}
              >{m.checklists_append_count({ count: preview.length })}</Button
            >
          </div>
        {:else if pasted.trim() !== ''}
          <p class="text-sm text-muted-foreground">{m.checklists_expand_hint()}</p>
        {/if}
      </div>
    {/if}
  {/if}
  {#if error !== undefined}
    <p role="alert" class="text-sm text-destructive">{error}</p>
  {/if}
</section>
