<script lang="ts">
  // 편집기가 없는 화면의 체크리스트 (D-30, O-04). 항목의 원본은 서버의 행이며, 이 구성요소는 표시와 입력만 한다.
  // Markdown 붙여넣기: `- [x]`·`- [ ]` 줄을 항목으로 펼쳐 보여 주고, 사용자가 확인·수정한 뒤 한 번에 덧붙인다.
  import { Button } from '$lib/ui/button';
  import { Checkbox } from '$lib/ui/checkbox';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';

  import { newRequestId } from '$lib/api/client';

  import type { ChecklistItem, NewChecklistEntry } from './checklist-item';
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
  let preview = $state<NewChecklistEntry[]>([]);
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
      error = e instanceof Error ? e.message : '처리하지 못했습니다.';
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
    preview = parseMarkdownChecklist(pasted).map((p) => ({ ...p, requestId: newRequestId() }));
  }

  function confirmPaste(): void {
    const confirmed = preview.filter((p) => p.text.trim() !== '');
    if (confirmed.length === 0) return;
    void guard(async () => {
      await onadd(confirmed.map((p) => ({ ...p, text: p.text.trim() })));
      pasted = '';
      preview = [];
      pasteOpen = false;
    });
  }

  const done = $derived(items.filter((i) => i.done).length);
</script>

<section aria-labelledby={`${idPrefix}-heading`} class="flex flex-col gap-3">
  <h3 id={`${idPrefix}-heading`} class="text-sm font-semibold">
    체크리스트 <span class="font-normal text-muted-foreground">{done}/{items.length}</span>
  </h3>

  <ul class="flex flex-col gap-2" data-testid="checklist-items">
    {#each items as item (item.id)}
      <li class="flex items-center gap-2">
        <Checkbox
          id={`${idPrefix}-check-${item.id}`}
          checked={item.done}
          disabled={!canEdit || busy}
          onCheckedChange={(value) => void guard(() => ontoggle(item.id, value === true))}
          aria-label={`${item.done ? '완료' : '미완료'} 항목: ${item.text}`}
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
            aria-label={`${item.text} 삭제`}
            disabled={busy}
            onclick={() => void guard(() => onremove(item.id))}>삭제</Button
          >
        {/if}
      </li>
    {:else}
      <li class="text-sm text-muted-foreground">항목이 없습니다.</li>
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
        <Label for={`${idPrefix}-new`}>새 항목</Label>
        <Input
          id={`${idPrefix}-new`}
          bind:value={draft}
          placeholder="할 일을 입력"
          autocomplete="off"
        />
      </div>
      <Button type="submit" disabled={busy || draft.trim() === ''}>추가</Button>
      <Button type="button" variant="outline" onclick={() => (pasteOpen = !pasteOpen)}
        >Markdown 붙여넣기</Button
      >
    </form>
    {#if pasteOpen}
      <div class="flex flex-col gap-2 rounded-md border p-3">
        <Label for={`${idPrefix}-paste`}>붙여넣을 체크리스트 (`- [x] 완료`, `- [ ] 미완료`)</Label>
        <textarea
          id={`${idPrefix}-paste`}
          bind:value={pasted}
          rows={4}
          class="rounded-md border border-input bg-transparent px-3 py-2 font-mono text-sm focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
        ></textarea>
        <div>
          <Button size="sm" variant="outline" onclick={expand} disabled={pasted.trim() === ''}
            >항목으로 펼치기</Button
          >
        </div>
        {#if preview.length > 0}
          <p class="text-sm text-muted-foreground">
            확인하고 고친 뒤 덧붙이세요. 기존 항목은 그대로 둡니다.
          </p>
          <ul class="flex flex-col gap-2" aria-label="덧붙일 항목">
            {#each preview as entry, index (index)}
              <li class="flex items-center gap-2">
                <Checkbox
                  checked={entry.done}
                  onCheckedChange={(value) => (preview[index] = { ...entry, done: value === true })}
                  aria-label={`${entry.text} 완료 여부`}
                />
                <Input
                  value={entry.text}
                  aria-label={`항목 ${String(index + 1)} 내용`}
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
              >{preview.length}개 덧붙이기</Button
            >
          </div>
        {:else if pasted.trim() !== ''}
          <p class="text-sm text-muted-foreground">펼치기를 누르면 항목을 미리 봅니다.</p>
        {/if}
      </div>
    {/if}
  {/if}
  {#if error !== undefined}
    <p role="alert" class="text-sm text-destructive">{error}</p>
  {/if}
</section>
