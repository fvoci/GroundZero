<script lang="ts">
  // 업무 CSV 일괄 등록(D-39). 한 줄씩 기존 업무 만들기 API로 넣는다(같은 검증과 권한). 한 줄은 모든 칸을 실은 요청
  // 하나라(D-105) 서버가 한 트랜잭션에서 확인·저장하고, 거절된 줄은 아무것도 남기지 않는다. 줄마다 진행 상태를 기억해
  // 두어(csv-import.ts) 중간에 실패해도 다시 누르면 다 넣은 줄은 건너뛰고 나머지를 같은 요청 id로 보낸다(응답을 잃은 줄이
  // 둘이 되지 않는다). 이어서 넣는 동안은 그렇다고 보여 준다. 등록하지 않는 동안 글자를 모두 지우거나 지난 줄과 겹치지
  // 않는 글자로 누르면 새 묶음이다.
  import type { AssigneeView } from '$lib/api/client';
  import type { TaskStatusDefinition } from '@fvoci/api-client';
  import { untrack } from 'svelte';
  import { me } from '$lib/me.svelte';
  import { org } from '$lib/org.svelte';
  import DiscardChangesConfirm from '$lib/features/common/DiscardChangesConfirm.svelte';
  import { createLeaveGuard, leavingWithDrafts } from '$lib/features/common/leave-guard.svelte';
  import { notifyDone } from '$lib/features/feedback/feedback.svelte';
  import { api, ApiError } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';
  import { Label } from '$lib/ui/label';
  import { Textarea } from '$lib/ui/textarea';
  import * as Collapsible from '$lib/ui/collapsible';

  import { parseTasksCsv } from './csv';
  import {
    matchRows,
    resetRow,
    resumeCount,
    rowApplied,
    listImportDrafts,
    holdImportDraft,
    importTabKey,
    removeImportDraft,
    type ImportDraft,
    saveImport,
    type RowProgress,
    type RowStep,
  } from './csv-import';

  interface Props {
    itemId: string;
    statuses: TaskStatusDefinition[];
    onimported: () => Promise<void>;
  }

  const { itemId, statuses, onimported }: Props = $props();
  const editingUser = untrack(() => me.userId);

  let text = $state('');
  let assignable = $state<AssigneeView[]>([]);
  let status = $state('');
  let error = $state<string | undefined>(undefined);
  let busy = $state(false);
  // 줄별 진행 상태. 이 묶음을 모두 넣을 때까지 지킨다(글자에서 지운 줄의 것도: 다시 넣으면 같은 업무). 글자를 모두
  // 지우면 버린다(새 묶음). 안의 값은 등록하면서 고치므로 등록이 끝날 때 배열을 바꿔 안내를 다시 계산한다.
  let progress = $state.raw<RowProgress[]>([]);
  let offers = $state<ImportDraft[]>([]);
  let restoredSource: string | undefined;
  const storageKey = $derived.by(() => {
    // 같은 조직의 다른 계정으로 목록을 다시 받아도 사용자 키를 다시 계산한다.
    if (org.organizations.length === 0) return '';
    return org.key(`csv-import:${itemId}`);
  });
  let loadedKey = $state('');
  const reserved = $derived(offers.some((entry) => entry.key === importTabKey(loadedKey)));
  let controller: AbortController | undefined;
  let generation = 0;
  // 마지막 저장이 이 탭에 남았는지. 계정 전환으로 다시 읽을 때 남았으면 막지 않는다(D-96).
  let kept = $state(true);
  let expanded = $state(true);

  $effect(() => {
    const key = storageKey;
    const id = itemId;
    const selected = org.currentId;
    const mine = ++generation;
    untrack(() => {
      offers = listImportDrafts(key).map((entry) => holdImportDraft(key, entry));
      restoredSource = undefined;
      text = '';
      progress = [];
      loadedKey = key;
      busy = false;
      status = '';
      error = undefined;
      assignable = [];
    });
    if (selected !== undefined) {
      void api
        .assignable(id)
        .then((a) => {
          if (mine === generation) assignable = a;
        })
        .catch(() => undefined);
    }
    return () => {
      generation += 1;
      controller?.abort();
    };
  });

  $effect(() => {
    if (
      !reserved &&
      me.userId === editingUser &&
      loadedKey === storageKey &&
      org.currentId !== undefined
    )
      persistImport(loadedKey, text, progress);
  });

  function persistImport(key: string, source: string, rows: RowProgress[]): void {
    kept = saveImport(key, source, rows);
    if (!kept) return;
    if (restoredSource !== undefined && restoredSource !== importTabKey(key))
      removeImportDraft(restoredSource);
    restoredSource = undefined;
  }

  function restoreOffer(entry: ImportDraft): void {
    if (busy || text !== '') return;
    text = entry.value.text;
    progress = entry.value.rows;
    restoredSource = entry.key;
    offers = offers.filter((candidate) => candidate.key !== entry.key);
    persistImport(loadedKey, text, progress);
  }

  // 공통 이탈 가드(D-125). 등록하는 중도 묻는다(`busy`로 넘기면 확인 없이 막기만 해 떠날 수 없다). 떠나기로 해도 입력과
  // 줄별 진행 상태는 이 탭에 남고(요청 id 포함), 돌아오면 되살려 같은 요청으로 이어 간다. 지우는 것은 제안의 버리기뿐이다.
  const unsaved = (): boolean => me.userId === editingUser && (busy || text.trim() !== '');
  const leave = createLeaveGuard({ dirty: unsaved, identity: () => loadedKey });
  $effect(() => leave.attach({ beforeUnload: () => unsaved() && (!leavingWithDrafts() || !kept) }));

  const parsed = $derived(
    text.trim() === '' ? undefined : parseTasksCsv(text, assignable, statuses),
  );
  // 지난 등록에 이어서 넣는다는 안내. 건너뛸 줄이 없으면 비운다.
  const resume = $derived.by(() => {
    if (parsed === undefined) return '';
    const skip = resumeCount(parsed.rows, progress);
    if (skip === 0) return '';
    return `${m.tasks_csv_resume_skip({ count: skip })} ${m.tasks_csv_resume_restart()}`;
  });

  // 글자를 모두 지우면 지난 묶음은 끝이다. 그 진행 상태를 남기면 다음 묶음의 같은 줄을 건너뛰거나 같은 제목 줄이 지난
  // 요청 id를 쓴다. 지난 묶음의 안내도 함께 지운다. 등록하는 동안 지운 것은 묶음을 끝내지 않는다: 그 등록은 끝까지
  // 가므로, 버리면 실패 뒤 같은 글자로 다시 누를 때(안내는 이어서 넣는다고 한다) 넣은 줄을 새 업무로 또 만든다.
  function edited(value: string): void {
    if (value.trim() !== '' || busy) return;
    progress = [];
    error = undefined;
    status = '';
  }

  async function importRows(): Promise<void> {
    const result = parsed;
    if (result === undefined || result.errors.length > 0 || busy) return;
    if (org.currentId === undefined || me.userId !== editingUser || reserved) return;
    const mine = generation;
    const keyForImport = loadedKey;
    const abort = new AbortController();
    controller = abort;
    const source = text;
    const steps = matchRows(result.rows, progress);
    const used = new Set(steps.map((s) => s.progress));
    // 지난 상태를 이어받는 줄이 하나도 없으면 다른 묶음을 붙여 넣은 것이다: 지난 상태를 버린다.
    const resumed = progress.some((p) => used.has(p));
    progress = resumed ? [...used, ...progress.filter((p) => !used.has(p))] : [...used];
    const batch = progress;
    persistImport(keyForImport, source, batch);
    busy = true;
    error = undefined;
    let done = 0;
    // 앞서 보낸 같은 요청 id로 이미 만들어져 이 줄 내용과 다른 업무의 제목.
    let differs: string | undefined;
    // 요청을 보내는 줄의 상태와 제목. 만들기가 404이면 그 줄을 새 요청 id로 다시 만든다.
    let current: RowStep | undefined;
    try {
      for (const step of steps) {
        if (me.userId !== editingUser) return;
        abort.signal.throwIfAborted();
        const { row, key, progress: line } = step;
        // 다 넣은 줄은 다시 보내지 않는다. 여기 오는 줄은 새 줄이거나 아직 다 넣지 못한 줄이다(matchRows는 다 넣은
        // 상태를 내용이 같은 줄에만 준다).
        if (line.applied !== key) {
          current = step;
          const task = await api.createTask(
            editingUser,
            itemId,
            line.requestId,
            row.title,
            null,
            {
              description: row.description,
              ...(row.status === undefined ? {} : { status: row.status }),
              priority: row.priority,
              start_date: row.start_date,
              due_date: row.due_date,
              estimate_minutes: row.estimate_minutes,
              tags: row.tags,
              assignee_ids: row.assignee_ids,
              milestone: false,
            },
            abort.signal,
          );
          abort.signal.throwIfAborted();
          // 같은 요청 id에 업무가 이미 있었다(응답을 잃은 앞선 요청). 다시 쓰지 않으므로 다르면 알린다. 다시 누르면
          // 다음 줄부터 넣는다(같은 업무를 또 만들지 않는다).
          line.applied = key;
          persistImport(keyForImport, source, batch);
          // 빈 상태 칸은 서버가 기본 상태를 고르므로 응답의 상태를 그대로 허용한다.
          if (!rowApplied({ ...row, status: row.status ?? task.status }, task)) {
            differs = row.title;
            throw new Error('differs');
          }
        }
        done += 1;
        status = m.tasks_csv_progress({ done, total: result.rows.length });
      }
      // 일반 완료는 토스트다(D-82). 진행 글("n/n")은 비운다.
      status = '';
      notifyDone(m.tasks_csv_imported({ count: done }));
      // 등록하는 동안 글자를 고쳤으면 그 글자와 진행 상태를 남긴다(다시 누르면 고친 줄만 넣는다). 등록하는 동안 글자를
      // 모두 지웠으면 새 묶음을 바란 것이므로 끝난 묶음의 진행 상태를 버린다(남기면 같은 줄을 다시 넣어도 건너뛴다).
      if (text === source || text.trim() === '') {
        text = '';
        progress = [];
      }
    } catch (e: unknown) {
      if (mine !== generation) return;
      if (abort.signal.aborted) {
        status = m.tasks_csv_cancelled();
        return;
      }
      let reason = errorText(e, m.tasks_csv_import_failed());
      if (differs !== undefined) {
        reason = m.tasks_csv_already_created({ title: differs });
      } else if (current !== undefined && e instanceof ApiError && e.code === 'task_deleted') {
        // 이 줄의 요청 id로 만든 업무를 그사이 지웠다(409 `task_deleted`). 같은 요청 id로는 다시 만들 수 없으므로 그
        // 줄은 다음에 새 요청 id로 만든다.
        resetRow(current.progress);
        reason = m.tasks_csv_task_gone({ title: current.row.title });
      } else if (current !== undefined) {
        // 거절된 줄은 아무것도 남기지 않았다(D-105). 어느 줄인지 알린다. 요청 id는 지킨다: 404(항목을 잠시 볼 수 없음)나
        // 연결 오류 뒤에도 응답을 잃은 만들기가 반영됐을 수 있어, 새 id로 보내면 다시 볼 수 있게 된 뒤 같은 줄이 둘이 된다.
        reason = m.tasks_csv_row_failed({ title: current.row.title, reason });
      }
      error = m.tasks_csv_import_error({ reason, count: done });
    } finally {
      if (mine === generation) {
        busy = false;
        progress = [...progress];
        persistImport(keyForImport, text, progress);
        await onimported();
      }
    }
  }
</script>

<DiscardChangesConfirm
  open={leave.open}
  description={m.tasks_csv_leave_description()}
  confirmLabel={m.tasks_csv_leave()}
  onkeep={leave.keep}
  ondiscard={() => {
    controller?.abort();
    leave.discard();
  }}
/>

{#if me.userId === editingUser}
  <Collapsible.Root bind:open={expanded} class="text-sm">
    <Collapsible.Trigger>{m.tasks_csv_summary()}</Collapsible.Trigger>
    <Collapsible.Content class="flex flex-col gap-2 pt-2">
      {#each offers as entry (entry.key)}
        <div role="region" aria-label={m.tasks_draft_region()} data-testid="task-csv-draft-offer">
          <p>{m.tasks_draft_region()}</p>
          <p class="max-w-full truncate">{entry.value.text.split('\n')[0]}</p>
          <Button
            variant="outline"
            disabled={text !== '' || busy}
            onclick={() => {
              restoreOffer(entry);
            }}>{m.tasks_draft_restore()}</Button
          >
          <Button
            variant="ghost"
            onclick={() => {
              removeImportDraft(entry.key);
              offers = offers.filter((candidate) => candidate.key !== entry.key);
            }}>{m.tasks_draft_discard()}</Button
          >
        </div>
      {/each}
      <Label for={`tasks-csv-${itemId}`}
        >{m.tasks_csv_label({
          statuses: statuses.map((definition) => definition.name).join(' · '),
        })}</Label
      >
      <Textarea
        id={`tasks-csv-${itemId}`}
        class="max-h-96 min-h-32 overflow-y-auto font-code"
        bind:value={text}
        disabled={busy || reserved || me.userId !== editingUser}
        oninput={(e: Event) => {
          if (e.currentTarget instanceof HTMLTextAreaElement) edited(e.currentTarget.value);
        }}
      />
      <p role="status" hidden={status === ''}>{status}</p>
      <p aria-live="polite">
        {#if error !== undefined}
          <span class="text-destructive">{error}</span>
        {:else if parsed !== undefined}
          {m.tasks_csv_read({ count: parsed.rows.length })}
          {#if parsed.errors.length > 0}
            <span class="text-destructive">
              {m.tasks_csv_fix_lines({
                lines: parsed.errors
                  .map((er) => m.tasks_csv_line_error({ line: er.line, reason: er.reason }))
                  .join(', '),
              })}
            </span>
          {/if}
        {/if}
      </p>
      <p class="text-muted-foreground" aria-live="polite">{resume}</p>
      <div>
        <Button
          variant="outline"
          disabled={busy ||
            reserved ||
            me.userId !== editingUser ||
            org.currentId === undefined ||
            parsed === undefined ||
            parsed.rows.length === 0 ||
            parsed.errors.length > 0}
          onclick={() => void importRows()}>{m.tasks_csv_import()}</Button
        >
        {#if busy}<Button variant="outline" onclick={() => controller?.abort()}
            >{m.common_cancel()}</Button
          >{/if}
        {#if org.currentId === undefined}<p>{m.app_organization_required()}</p>{/if}
      </div>
    </Collapsible.Content>
  </Collapsible.Root>
{/if}
