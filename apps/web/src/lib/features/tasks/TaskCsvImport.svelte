<script lang="ts">
  // 업무 CSV 일괄 등록(D-39). 한 줄씩 기존 업무 만들기·고치기 API로 넣는다(같은 검증과 권한). 줄마다 요청 id를
  // 기억해 두어 중간에 실패해도 다시 누르면 이미 만든 업무를 또 만들지 않는다.
  import type { AssigneeView } from '$lib/api/client';
  import { api, newRequestId } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';
  import { Label } from '$lib/ui/label';

  import { parseTasksCsv } from './csv';

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
  // 같은 글자로 다시 가져오면 같은 요청 id를 쓴다(줄 순서 기준).
  let requestIds: string[] = [];
  let forText = '';

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
    if (forText !== text) {
      requestIds = result.rows.map(() => newRequestId());
      forText = text;
    }
    busy = true;
    error = undefined;
    let done = 0;
    // 다른 변경과 겹쳐 멈춘 줄의 제목.
    let conflicted: string | undefined;
    try {
      for (const [index, row] of result.rows.entries()) {
        const created = await api.createTask(
          itemId,
          requestIds[index] ?? newRequestId(),
          row.title,
        );
        const updated = await api.updateTask(created.id, created.version, {
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
          conflicted = row.title;
          throw new Error('conflict');
        }
        done += 1;
        status = m.tasks_csv_progress({ done, total: result.rows.length });
      }
      status = m.tasks_csv_imported({ count: done });
      text = '';
      forText = '';
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
      class="min-h-32 rounded-md border bg-background p-2 font-mono text-sm"
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
