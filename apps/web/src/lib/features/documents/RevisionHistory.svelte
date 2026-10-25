<script lang="ts">
  // 리비전 기록·비교·복원 (D-27). 복원은 옛 내용으로 새 리비전을 만들며 기록을 지우지 않는다.
  // 편집 중 저장하지 않은 내용은 편집기의 초안 보존(D-53)이 지킨다.
  import { formatInstant } from '$lib/display';
  import type { DocumentView, RevisionSummaryView, RevisionView } from '$lib/api/client';
  import { api, newRequestId } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import { m } from '$lib/paraglide/messages.js';
  import { notifyDone } from '$lib/features/feedback/feedback.svelte';
  import { Button } from '$lib/ui/button';
  import * as Dialog from '$lib/ui/dialog';

  import { standInLabel } from './revision-author';
  import type { DiffLine } from './revision-diff';
  import { diffRevisions } from './revision-diff';

  interface Props {
    doc: DocumentView;
    onrestored: (doc: DocumentView) => void;
  }

  const { doc, onrestored }: Props = $props();

  let open = $state(false);
  let list = $state<RevisionSummaryView[]>([]);
  let more = $state(false);
  let selected = $state<RevisionView | undefined>(undefined);
  let previous = $state<RevisionView | undefined>(undefined);
  let compareWith = $state<'current' | 'previous'>('current');
  let error = $state<string | undefined>(undefined);
  let busy = $state(false);
  // 복원이 충돌하면 서버가 준 지금 문서(그사이 남은 리비전 포함). 비교와 다시 복원의 기준을 이것으로 바꾼다(저절로 다시
  // 보내지 않고, 사람이 그 내용을 보고 다시 누를 때만 이 리비전을 기준으로 보낸다).
  let latest = $state<DocumentView | undefined>(undefined);
  // 화면의 문서가 그 뒤 더 새 리비전이 되면(저장 등) 그것을 쓴다.
  const current = $derived(latest !== undefined && latest.revision > doc.revision ? latest : doc);
  // 같은 대상의 재시도는 같은 request_id(중복 리비전 방지).
  let pending: { revision: number; base: number; requestId: string } | undefined;

  const PAGE = 100;

  async function load(before: number | null): Promise<void> {
    try {
      const page = await api.revisions(doc.id, before);
      list = before === null ? page : [...list, ...page];
      more = page.length === PAGE;
      error = undefined;
    } catch (e: unknown) {
      error = errorText(e, m.documents_history_load_failed());
    }
  }

  // 마지막으로 누른 리비전의 응답만 쓴다(빨리 두 개를 누르면 앞의 응답이 늦게 와 다른 리비전을 고른 것처럼 되지 않게).
  let chosen = 0;
  // 고른 리비전의 직전 리비전을 불러오지 못했다. 그때는 "불러오는 중…"을 거두고 아래의 오류만 보인다(다시 누르면 다시 묻는다).
  let previousFailed = $state(false);

  async function choose(revision: number): Promise<void> {
    const mine = ++chosen;
    previousFailed = false;
    try {
      const next = await api.revision(doc.id, revision);
      if (mine !== chosen) return;
      selected = next;
      const before = revision > 1 ? await api.revision(doc.id, revision - 1) : undefined;
      if (mine !== chosen) return;
      previous = before;
      error = undefined;
    } catch (e: unknown) {
      if (mine !== chosen) return;
      if (selected?.revision === revision) previousFailed = true;
      error = errorText(e, m.documents_revision_load_failed());
    }
  }

  // 리비전 목록은 목록 상자다: Tab으로는 한 칸(고른 리비전, 없으면 처음)에만 들어가고, 위·아래·Home·End로 옮기며
  // 고른다(옮긴 리비전을 바로 불러 비교한다).
  const tabRevision = $derived(
    list.some((r) => r.revision === selected?.revision) ? selected?.revision : list.at(0)?.revision,
  );

  function moveInList(e: KeyboardEvent & { currentTarget: HTMLElement }): void {
    const options = [...e.currentTarget.querySelectorAll<HTMLElement>('[role="option"]')];
    const at = options.findIndex((o) => o === document.activeElement);
    const last = options.length - 1;
    const next =
      e.key === 'ArrowDown'
        ? Math.min(at + 1, last)
        : e.key === 'ArrowUp'
          ? Math.max(at - 1, 0)
          : e.key === 'Home'
            ? 0
            : e.key === 'End'
              ? last
              : undefined;
    const target = next === undefined ? undefined : options.at(next);
    if (next === undefined || target === undefined) return;
    e.preventDefault();
    target.focus();
    if (next !== at) target.click();
  }

  function openHistory(): void {
    open = true;
    // 지난번에 열었을 때 고른 리비전의 늦은 응답은 버린다.
    chosen += 1;
    previousFailed = false;
    selected = undefined;
    previous = undefined;
    latest = undefined;
    compareWith = 'current';
    void load(null);
  }

  // 직전과 비교할 리비전을 아직 받지 못했다(다른 리비전을 고른 뒤 그 직전 리비전을 불러오는 중이거나 실패). 그동안 앞서
  // 고른 리비전의 직전 리비전과 비교해 "r9 → r10"이라고 보이지 않는다.
  const previousMissing = $derived(
    selected !== undefined && selected.revision > 1 && previous?.revision !== selected.revision - 1,
  );

  const lines = $derived.by((): DiffLine[] => {
    if (selected === undefined) return [];
    if (compareWith === 'previous') {
      if (previousMissing) return [];
      return previous === undefined
        ? diffRevisions({ title: '', body: { type: 'doc' } }, selected)
        : diffRevisions(previous, selected);
    }
    // 선택한 리비전 → 지금 문서: 복원하면 무엇이 바뀌는지는 반대 방향이므로 지금 → 선택으로 보여 준다.
    return diffRevisions(current, selected);
  });

  const changed = $derived(lines.some((l) => l.kind !== 'same'));

  async function restore(): Promise<void> {
    if (selected === undefined) return;
    const target = selected.revision;
    if (pending?.revision !== target || pending.base !== current.revision) {
      pending = { revision: target, base: current.revision, requestId: newRequestId() };
    }
    busy = true;
    error = undefined;
    try {
      const result = await api.restoreRevision(doc.id, target, {
        requestId: pending.requestId,
        baseRevision: pending.base,
      });
      if (result.kind === 'conflict') {
        latest = result.current;
        error = m.documents_restore_conflict({ revision: result.current.revision });
        return;
      }
      pending = undefined;
      latest = undefined;
      open = false;
      onrestored(result.document);
      notifyDone(m.common_restored());
    } catch (e: unknown) {
      error = errorText(e, m.documents_restore_failed());
    } finally {
      busy = false;
    }
  }

  const when = (iso: string): string => formatInstant(iso);
</script>

<Button size="sm" variant="outline" onclick={openHistory}>{m.documents_history_button()}</Button>

<Dialog.Root bind:open>
  <Dialog.Content
    class="max-h-[calc(100dvh-2rem)] grid-cols-[minmax(0,1fr)] overflow-y-auto sm:max-w-4xl"
  >
    <Dialog.Header>
      <Dialog.Title>{m.documents_history_title()}</Dialog.Title>
      <Dialog.Description>
        {m.documents_history_description({ revision: doc.revision })}
      </Dialog.Description>
    </Dialog.Header>
    <div class="grid max-h-[65vh] gap-4 overflow-hidden md:grid-cols-[14rem_1fr]">
      <div class="flex min-h-0 flex-col gap-1 overflow-y-auto">
        <div
          class="flex flex-col gap-1"
          role="listbox"
          tabindex="-1"
          aria-label={m.documents_revisions_label()}
          onkeydown={moveInList}
        >
          {#each list as rev (rev.revision)}
            <button
              type="button"
              role="option"
              tabindex={rev.revision === tabRevision ? 0 : -1}
              aria-selected={selected?.revision === rev.revision}
              class={`rounded px-2 py-1.5 text-left text-sm hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none ${selected?.revision === rev.revision ? 'bg-accent' : ''}`}
              onclick={() => void choose(rev.revision)}
            >
              <span class="font-mono">r{rev.revision}</span>
              <span class="text-muted-foreground"
                >{standInLabel(rev) ?? rev.author_name ?? m.documents_unknown_author()}</span
              >
              <span class="block text-xs text-muted-foreground">
                {when(rev.created_at)}{rev.restored_from === null
                  ? ''
                  : ` · ${m.documents_restored_from({ revision: rev.restored_from })}`}
              </span>
            </button>
          {/each}
        </div>
        {#if more}
          <Button size="sm" variant="ghost" onclick={() => void load(list.at(-1)?.revision ?? null)}
            >{m.documents_load_more()}</Button
          >
        {/if}
      </div>
      <div class="flex min-h-0 flex-col gap-2 overflow-hidden">
        {#if selected === undefined}
          <p class="text-sm text-muted-foreground">{m.documents_history_pick()}</p>
        {:else}
          <div class="flex flex-wrap items-center gap-2 text-sm">
            <span class="font-semibold">r{selected.revision}</span>
            <span class="text-muted-foreground">{selected.title}</span>
            <div class="ml-auto flex gap-1" role="group" aria-label={m.documents_compare_label()}>
              <Button
                size="sm"
                variant={compareWith === 'current' ? 'secondary' : 'ghost'}
                aria-pressed={compareWith === 'current'}
                onclick={() => (compareWith = 'current')}>{m.documents_compare_current()}</Button
              >
              <Button
                size="sm"
                variant={compareWith === 'previous' ? 'secondary' : 'ghost'}
                aria-pressed={compareWith === 'previous'}
                onclick={() => (compareWith = 'previous')}>{m.documents_compare_previous()}</Button
              >
            </div>
          </div>
          <p class="text-xs text-muted-foreground">
            {compareWith === 'current'
              ? m.documents_compare_current_hint({
                  current: current.revision,
                  selected: selected.revision,
                })
              : m.documents_compare_previous_hint({
                  previous: selected.revision - 1,
                  selected: selected.revision,
                })}
          </p>
          <div
            class="min-h-0 flex-1 overflow-y-auto rounded border p-2 font-code text-sm"
            data-testid="revision-diff"
          >
            {#if compareWith === 'previous' && previousMissing}
              {#if !previousFailed}
                <p class="text-muted-foreground" aria-live="polite">{m.documents_loading()}</p>
              {/if}
            {:else if !changed}
              <p class="text-muted-foreground">{m.documents_diff_none()}</p>
            {/if}
            {#each lines as line, index (index)}
              <div
                class={`whitespace-pre-wrap ${line.kind === 'added' ? 'bg-emerald-500/15' : line.kind === 'removed' ? 'bg-rose-500/15 line-through decoration-rose-500/60' : ''}`}
              >
                <span class="sr-only"
                  >{line.kind === 'added'
                    ? `${m.documents_diff_added()} `
                    : line.kind === 'removed'
                      ? `${m.documents_diff_removed()} `
                      : ''}</span
                ><span aria-hidden="true" class="mr-2 text-muted-foreground"
                  >{line.kind === 'added' ? '+' : line.kind === 'removed' ? '−' : ' '}</span
                >{line.text}
              </div>
            {/each}
          </div>
        {/if}
      </div>
    </div>
    {#if error !== undefined}
      <p role="alert" class="text-sm text-destructive">{error}</p>
    {/if}
    <Dialog.Footer>
      <Button variant="ghost" onclick={() => (open = false)}>{m.documents_close()}</Button>
      {#if doc.can_edit && selected !== undefined && selected.revision !== current.revision}
        <Button
          disabled={busy || (compareWith === 'current' && !changed)}
          onclick={() => void restore()}
          >{m.documents_restore_to({ revision: selected.revision })}</Button
        >
      {/if}
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
