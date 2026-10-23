<script lang="ts">
  // 업무 CSV 일괄 등록(D-39). 한 줄씩 기존 업무 만들기 API로 넣는다(같은 검증과 권한). 한 줄은 모든 칸을 실은 요청
  // 하나라(D-105) 서버가 한 트랜잭션에서 확인·저장하고, 거절된 줄은 아무것도 남기지 않는다. 줄마다 진행 상태를 기억해
  // 두어(csv-import.ts) 중간에 실패해도 다시 누르면 다 넣은 줄은 건너뛰고 나머지를 같은 요청 id로 보낸다(응답을 잃은 줄이
  // 둘이 되지 않는다). 이어서 넣는 동안은 그렇다고 보여 준다. 등록하지 않는 동안 글자를 모두 지우거나 지난 줄과 겹치지
  // 않는 글자로 누르면 새 묶음이다.
  import type { AssigneeView } from '$lib/api/client';
  import { api, ApiError } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';
  import { Label } from '$lib/ui/label';

  import { parseTasksCsv } from './csv';
  import {
    matchRows,
    resetRow,
    resumeCount,
    rowApplied,
    type RowProgress,
    type RowStep,
  } from './csv-import';

  interface Props {
    itemId: string;
    onimported: () => Promise<void>;
  }

  const { itemId, onimported }: Props = $props();

  let text = $state('');
  let assignable = $state<AssigneeView[]>([]);
  let status = $state('');
  let error = $state<string | undefined>(undefined);
  let busy = $state(false);
  // 줄별 진행 상태. 이 묶음을 모두 넣을 때까지 지킨다(글자에서 지운 줄의 것도: 다시 넣으면 같은 업무). 글자를 모두
  // 지우면 버린다(새 묶음). 안의 값은 등록하면서 고치므로 등록이 끝날 때 배열을 바꿔 안내를 다시 계산한다.
  let progress = $state.raw<RowProgress[]>([]);

  $effect(() => {
    const id = itemId;
    api
      .assignable(id)
      .then((a) => (assignable = a))
      .catch(() => (assignable = []));
  });

  const parsed = $derived(text.trim() === '' ? undefined : parseTasksCsv(text, assignable));
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
    const source = text;
    const steps = matchRows(result.rows, progress);
    const used = new Set(steps.map((s) => s.progress));
    // 지난 상태를 이어받는 줄이 하나도 없으면 다른 묶음을 붙여 넣은 것이다: 지난 상태를 버린다.
    const resumed = progress.some((p) => used.has(p));
    progress = resumed ? [...used, ...progress.filter((p) => !used.has(p))] : [...used];
    busy = true;
    error = undefined;
    let done = 0;
    // 앞서 보낸 같은 요청 id로 이미 만들어져 이 줄 내용과 다른 업무의 제목.
    let differs: string | undefined;
    // 요청을 보내는 줄의 상태와 제목. 만들기가 404이면 그 줄을 새 요청 id로 다시 만든다.
    let current: RowStep | undefined;
    try {
      for (const step of steps) {
        const { row, key, progress: line } = step;
        // 다 넣은 줄은 다시 보내지 않는다. 여기 오는 줄은 새 줄이거나 아직 다 넣지 못한 줄이다(matchRows는 다 넣은
        // 상태를 내용이 같은 줄에만 준다).
        if (line.applied !== key) {
          current = step;
          const task = await api.createTask(itemId, line.requestId, row.title, null, {
            description: row.description,
            status: row.status,
            priority: row.priority,
            start_date: row.start_date,
            due_date: row.due_date,
            estimate_minutes: row.estimate_minutes,
            tags: row.tags,
            assignee_ids: row.assignee_ids,
            milestone: false,
          });
          // 같은 요청 id에 업무가 이미 있었다(응답을 잃은 앞선 요청). 다시 쓰지 않으므로 다르면 알린다. 다시 누르면
          // 다음 줄부터 넣는다(같은 업무를 또 만들지 않는다).
          line.applied = key;
          if (!rowApplied(row, task)) {
            differs = row.title;
            throw new Error('differs');
          }
        }
        done += 1;
        status = m.tasks_csv_progress({ done, total: result.rows.length });
      }
      status = m.tasks_csv_imported({ count: done });
      // 등록하는 동안 글자를 고쳤으면 그 글자와 진행 상태를 남긴다(다시 누르면 고친 줄만 넣는다). 등록하는 동안 글자를
      // 모두 지웠으면 새 묶음을 바란 것이므로 끝난 묶음의 진행 상태를 버린다(남기면 같은 줄을 다시 넣어도 건너뛴다).
      if (text === source || text.trim() === '') {
        text = '';
        progress = [];
      }
    } catch (e: unknown) {
      let reason = errorText(e, m.tasks_csv_import_failed());
      if (differs !== undefined) {
        reason = m.tasks_csv_already_created({ title: differs });
      } else if (current !== undefined && e instanceof ApiError && e.status === 404) {
        // 이 줄의 요청 id로 만든 업무를 그사이 지웠다(또는 볼 수 없다). 같은 요청 id의 만들기는 404이므로 그 줄은
        // 다음에 새 요청 id로 만든다.
        resetRow(current.progress);
        reason = m.tasks_csv_task_gone({ title: current.row.title });
      } else if (current !== undefined) {
        // 거절된 줄은 아무것도 남기지 않았다(D-105). 어느 줄인지 알린다.
        reason = m.tasks_csv_row_failed({ title: current.row.title, reason });
      }
      error = m.tasks_csv_import_error({ reason, count: done });
    } finally {
      busy = false;
      progress = [...progress];
      await onimported();
    }
  }
</script>

<details class="text-sm">
  <summary class="cursor-pointer">{m.tasks_csv_summary()}</summary>
  <div class="flex flex-col gap-2 pt-2">
    <Label for={`tasks-csv-${itemId}`}>{m.tasks_csv_label()}</Label>
    <textarea
      id={`tasks-csv-${itemId}`}
      class="min-h-32 rounded-md border bg-background p-2 font-code text-sm"
      bind:value={text}
      oninput={(e) => {
        edited(e.currentTarget.value);
      }}></textarea>
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
      {:else}
        {status}
      {/if}
    </p>
    <p class="text-muted-foreground" aria-live="polite">{resume}</p>
    <div>
      <Button
        variant="outline"
        disabled={busy ||
          parsed === undefined ||
          parsed.rows.length === 0 ||
          parsed.errors.length > 0}
        onclick={() => void importRows()}>{m.tasks_csv_import()}</Button
      >
    </div>
  </div>
</details>
