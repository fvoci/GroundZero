<script lang="ts">
  // 업무 CSV 일괄 등록(D-39). 한 줄씩 기존 업무 만들기·고치기 API로 넣는다(같은 검증과 권한). 줄마다 진행 상태를
  // 기억해 두어(csv-import.ts) 중간에 실패해도 다시 누르면 다 넣은 줄은 건너뛰고, 제목만 만들어진 줄은 그 업무에 채운다.
  // 이어서 넣는 동안은 그렇다고 보여 준다. 글자를 모두 지우거나 지난 줄과 겹치지 않는 글자로 누르면 새 묶음이다.
  import type { AssigneeView } from '$lib/api/client';
  import { api } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';
  import { Label } from '$lib/ui/label';

  import { parseTasksCsv } from './csv';
  import { matchRows, resumeCounts, type RowProgress } from './csv-import';

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
  // 지난 등록에 이어서 넣는다는 안내. 이어받는 줄이 없으면 비운다(누르면 모두 새 업무).
  const resume = $derived.by(() => {
    if (parsed === undefined) return '';
    const { skip, fill } = resumeCounts(parsed.rows, progress);
    if (skip === 0 && fill === 0) return '';
    return [
      skip > 0 ? m.tasks_csv_resume_skip({ count: skip }) : '',
      fill > 0 ? m.tasks_csv_resume_fill({ count: fill }) : '',
      m.tasks_csv_resume_restart(),
    ]
      .filter((s) => s !== '')
      .join(' ');
  });

  // 글자를 모두 지우면 지난 묶음은 끝이다. 그 진행 상태를 남기면 다음 묶음의 같은 줄을 건너뛰거나 같은 제목 줄을 지난
  // 업무에 채운다. 지난 묶음의 안내도 함께 지운다.
  function edited(value: string): void {
    if (value.trim() !== '') return;
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
    // 다른 변경과 겹쳐 멈춘 줄의 제목.
    let conflicted: string | undefined;
    // 만들었지만 나머지 칸을 아직 넣지 못한(제목만 있는) 업무의 제목. 고치기가 실패하면 어느 업무인지 알린다.
    let titleOnly: string | undefined;
    try {
      for (const { row, key, progress: step } of steps) {
        // 다 넣은 줄은 다시 보내지 않는다. 만들기는 같은 요청 id에 지금 판을 돌려주므로, 다시 고치기를 보내면 그사이
        // 다른 사람이 바꾼 상태·담당자를 CSV 값으로 조용히 되돌린다. 여기 오는 줄은 새 줄이거나 아직 다 넣지 못한 줄이다
        // (matchRows는 다 넣은 상태를 내용이 같은 줄에만 준다).
        if (step.applied !== key) {
          const task = step.task ?? (await api.createTask(itemId, step.requestId, row.title));
          step.task = { id: task.id, version: task.version };
          titleOnly = row.title;
          // 기준 판은 이 등록이 받은 판이다(만들기 또는 겹침을 알릴 때 받은 판). 그사이 다른 곳에서 바뀌었으면 409.
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
          titleOnly = undefined;
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
      const failed = m.tasks_csv_import_error({
        reason:
          conflicted === undefined
            ? errorText(e, m.tasks_csv_import_failed())
            : m.tasks_csv_conflict({ title: conflicted }),
        count: done,
      });
      error =
        titleOnly === undefined
          ? failed
          : `${failed} ${m.tasks_csv_title_only({ title: titleOnly })}`;
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
