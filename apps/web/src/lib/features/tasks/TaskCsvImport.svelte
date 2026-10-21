<script lang="ts">
  // 업무 CSV 일괄 등록(D-39). 한 줄씩 기존 업무 만들기·고치기 API로 넣는다(같은 검증과 권한). 줄마다 진행 상태를
  // 기억해 두어(csv-import.ts) 중간에 실패해도 다시 누르면 다 넣은 줄은 건너뛰고, 제목만 만들어진 줄은 그 업무에 채운다.
  import type { AssigneeView } from '$lib/api/client';
  import { api } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';
  import { Label } from '$lib/ui/label';

  import { parseTasksCsv } from './csv';
  import { matchRows, type RowProgress } from './csv-import';

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
  // 줄별 진행 상태. 모두 넣을 때까지 지킨다(글자에서 지운 줄의 것도: 다시 넣으면 같은 업무).
  let progress: RowProgress[] = [];

  $effect(() => {
    const id = itemId;
    api
      .assignable(id)
      .then((a) => (assignable = a))
      .catch(() => (assignable = []));
  });

  const parsed = $derived(text.trim() === '' ? undefined : parseTasksCsv(text, assignable));

  async function importRows(): Promise<void> {
    const result = parsed;
    if (result === undefined || result.errors.length > 0 || busy) return;
    const source = text;
    const steps = matchRows(result.rows, progress);
    const used = new Set(steps.map((s) => s.progress));
    progress = [...used, ...progress.filter((p) => !used.has(p))];
    busy = true;
    error = undefined;
    let done = 0;
    // 다른 변경과 겹쳐 멈춘 줄의 제목.
    let conflicted: string | undefined;
    try {
      for (const { row, key, progress: step } of steps) {
        // 다 넣은 줄은 다시 보내지 않는다. 만들기는 같은 요청 id에 지금 판을 돌려주므로, 다시 고치기를 보내면 그사이
        // 다른 사람이 바꾼 상태·담당자를 CSV 값으로 조용히 되돌린다.
        if (step.applied !== key) {
          const task = step.task ?? (await api.createTask(itemId, step.requestId, row.title));
          step.task = { id: task.id, version: task.version };
          // 기준 판은 이 등록이 받은 판이다(만들기 또는 지난 고치기). 그사이 다른 곳에서 바뀌었으면 409.
          const updated = await api.updateTask(task.id, step.task.version, {
            title: row.title,
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
          if (updated.kind !== 'updated') {
            // 알린 뒤 다시 누르면 지금 판 위에 이 줄을 넣는다(메시지가 덮어쓴다고 알린다).
            step.task = { id: task.id, version: updated.current.version };
            conflicted = row.title;
            throw new Error('conflict');
          }
          step.task = { id: task.id, version: updated.task.version };
          step.applied = key;
        }
        done += 1;
        status = m.tasks_csv_progress({ done, total: result.rows.length });
      }
      status = m.tasks_csv_imported({ count: done });
      // 등록하는 동안 글자를 고쳤으면 그 글자와 진행 상태를 남긴다(다시 누르면 고친 줄만 넣는다).
      if (text === source) {
        text = '';
        progress = [];
      }
    } catch (e: unknown) {
      error = m.tasks_csv_import_error({
        reason:
          conflicted === undefined
            ? errorText(e, m.tasks_csv_import_failed())
            : m.tasks_csv_conflict({ title: conflicted }),
        count: done,
      });
    } finally {
      busy = false;
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
      bind:value={text}></textarea>
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
