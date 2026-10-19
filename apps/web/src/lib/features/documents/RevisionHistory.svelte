<script lang="ts">
  // 리비전 기록·비교·복원 (D-27). 복원은 옛 내용으로 새 리비전을 만들며 기록을 지우지 않는다.
  // 편집 중 저장하지 않은 내용은 편집기의 초안 보존(D-53)이 지킨다.
  import { formatInstant } from '$lib/display';
  import type { DocumentView, RevisionSummaryView, RevisionView } from '$lib/api/client';
  import { api, newRequestId } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';
  import * as Dialog from '$lib/ui/dialog';

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

  async function choose(revision: number): Promise<void> {
    try {
      selected = await api.revision(doc.id, revision);
      previous = revision > 1 ? await api.revision(doc.id, revision - 1) : undefined;
      error = undefined;
    } catch (e: unknown) {
      error = errorText(e, m.documents_revision_load_failed());
    }
  }

  function openHistory(): void {
    open = true;
    selected = undefined;
    previous = undefined;
    latest = undefined;
    compareWith = 'current';
    void load(null);
  }

  const lines = $derived.by((): DiffLine[] => {
    if (selected === undefined) return [];
    if (compareWith === 'previous') {
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
  <Dialog.Content class="sm:max-w-4xl">
    <Dialog.Header>
      <Dialog.Title>{m.documents_history_title()}</Dialog.Title>
      <Dialog.Description>
        {m.documents_history_description({ revision: doc.revision })}
      </Dialog.Description>
    </Dialog.Header>
    <div class="grid max-h-[65vh] gap-4 overflow-hidden md:grid-cols-[14rem_1fr]">
      <div
        class="flex min-h-0 flex-col gap-1 overflow-y-auto"
        role="listbox"
        aria-label={m.documents_revisions_label()}
      >
        {#each list as rev (rev.revision)}
          <button
            type="button"
            role="option"
            aria-selected={selected?.revision === rev.revision}
            class={`rounded px-2 py-1.5 text-left text-sm hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none ${selected?.revision === rev.revision ? 'bg-accent' : ''}`}
            onclick={() => void choose(rev.revision)}
          >
            <span class="font-mono">r{rev.revision}</span>
            <span class="text-muted-foreground"
              >{rev.author_name ?? m.documents_unknown_author()}</span
            >
            <span class="block text-xs text-muted-foreground">
              {when(rev.created_at)}{rev.restored_from === null
                ? ''
                : ` · ${m.documents_restored_from({ revision: rev.restored_from })}`}
            </span>
          </button>
        {/each}
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
            {#if !changed}
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
