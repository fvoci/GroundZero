<script lang="ts">
  // 편집기가 없는 화면의 체크리스트 (D-30, O-04). 항목의 원본은 서버의 행이며, 이 구성요소는 표시와 입력만 한다.
  // Markdown 붙여넣기: `- [x]`·`- [ ]` 줄을 항목으로 펼쳐 보여 주고, 사용자가 확인·수정한 뒤 한 번에 덧붙인다.
  import { Button } from '$lib/ui/button';
  import { Checkbox } from '$lib/ui/checkbox';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';

  import { newRequestId } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import { RetryKey } from '$lib/api/retry-key';
  import { m } from '$lib/paraglide/messages.js';

  import type { ChecklistItem, NewChecklistEntry, ParsedChecklistItem } from './checklist-item';
  import { parseMarkdownChecklist } from './checklist-item';

  interface Props {
    /** 화면 안에서 id가 겹치지 않게 붙이는 접두어. */
    idPrefix: string;
    items: ChecklistItem[];
    canEdit: boolean;
    onadd: (items: NewChecklistEntry[]) => Promise<void>;
    ontoggle: (id: string, done: boolean) => Promise<void>;
    onremove: (id: string) => Promise<void>;
  }

  const { idPrefix, items, canEdit, onadd, ontoggle, onremove }: Props = $props();

  let draft = $state('');
  let pasteOpen = $state(false);
  let pasted = $state('');
  let preview = $state<ParsedChecklistItem[]>([]);
  // 미리보기 줄마다의 재시도 키. 고치지 않고 다시 보내면 같은 id, 고친 줄은 새 id(같은 id면 서버가 처음 것을 남기고
  // 고친 내용을 버린다).
  let previewKeys: RetryKey[] = [];
  // 한 줄 추가의 재시도용 요청 id. 입력이 바뀌면 새로 만든다.
  let draftRequest: { text: string; requestId: string } | undefined;
  let busy = $state(false);
  let error = $state<string | undefined>(undefined);

  async function guard(action: () => Promise<void>): Promise<void> {
    busy = true;
    error = undefined;
    try {
      await action();
    } catch (e: unknown) {
      error = errorText(e, m.checklists_action_failed());
    } finally {
      busy = false;
    }
  }

  function addOne(): void {
    const text = draft.trim();
    if (text === '') return;
    void guard(async () => {
      if (draftRequest?.text !== text) draftRequest = { text, requestId: newRequestId() };
      await onadd([{ text, done: false, requestId: draftRequest.requestId }]);
      draftRequest = undefined;
      draft = '';
    });
  }

  function expand(): void {
    preview = parseMarkdownChecklist(pasted);
    // 실패한 뒤 다시 펼쳐도 줄마다의 키를 지킨다(같은 내용이면 같은 id라 응답을 잃은 덧붙이기가 중복되지 않는다).
    // 덧붙이기가 성공했을 때만 키를 비운다.
    previewKeys = preview.map((_, index) => previewKeys[index] ?? new RetryKey());
  }

  function confirmPaste(): void {
    const confirmed: NewChecklistEntry[] = [];
    preview.forEach((p, index) => {
      const entry = { text: p.text.trim(), done: p.done };
      if (entry.text === '') return;
      const key = previewKeys[index] ?? new RetryKey();
      previewKeys[index] = key;
      confirmed.push({ ...entry, requestId: key.for(entry) });
    });
    if (confirmed.length === 0) return;
    void guard(async () => {
      await onadd(confirmed);
      pasted = '';
      preview = [];
      previewKeys = [];
      pasteOpen = false;
    });
  }

  const done = $derived(items.filter((i) => i.done).length);
</script>

<section aria-labelledby={`${idPrefix}-heading`} class="flex flex-col gap-3">
  <h3 id={`${idPrefix}-heading`} class="text-sm font-semibold">
    {m.checklists_heading()}
    <span class="font-normal text-muted-foreground">{done}/{items.length}</span>
  </h3>

  <ul class="flex flex-col gap-2" data-testid="checklist-items">
    {#each items as item (item.id)}
      <li class="flex items-center gap-2">
        <Checkbox
          id={`${idPrefix}-check-${item.id}`}
          checked={item.done}
          disabled={!canEdit || busy}
          onCheckedChange={(value) => void guard(() => ontoggle(item.id, value === true))}
          aria-label={item.done
            ? m.checklists_item_done_aria({ text: item.text })
            : m.checklists_item_open_aria({ text: item.text })}
        />
        <Label
          for={`${idPrefix}-check-${item.id}`}
          class={item.done ? 'text-muted-foreground line-through' : ''}>{item.text}</Label
        >
        {#if canEdit}
          <Button
            variant="ghost"
            size="sm"
            class="ml-auto"
            aria-label={m.checklists_delete_aria({ text: item.text })}
            disabled={busy}
            onclick={() => void guard(() => onremove(item.id))}>{m.checklists_delete()}</Button
          >
        {/if}
      </li>
    {:else}
      <li class="text-sm text-muted-foreground">{m.checklists_empty()}</li>
    {/each}
  </ul>

  {#if canEdit}
    <form
      class="flex items-end gap-2"
      onsubmit={(event) => {
        event.preventDefault();
        addOne();
      }}
    >
      <div class="flex flex-1 flex-col gap-1">
        <Label for={`${idPrefix}-new`}>{m.checklists_new_item()}</Label>
        <Input
          id={`${idPrefix}-new`}
          bind:value={draft}
          placeholder={m.checklists_new_item_placeholder()}
          autocomplete="off"
        />
      </div>
      <Button type="submit" disabled={busy || draft.trim() === ''}>{m.checklists_add()}</Button>
      <Button type="button" variant="outline" onclick={() => (pasteOpen = !pasteOpen)}
        >{m.checklists_paste_markdown()}</Button
      >
    </form>
    {#if pasteOpen}
      <div class="flex flex-col gap-2 rounded-md border p-3">
        <Label for={`${idPrefix}-paste`}>{m.checklists_paste_label()}</Label>
        <textarea
          id={`${idPrefix}-paste`}
          bind:value={pasted}
          rows={4}
          class="rounded-md border border-input bg-transparent px-3 py-2 font-code text-sm focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
        ></textarea>
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
