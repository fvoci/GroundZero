<script lang="ts">
  // 항목 업무의 달력 보기(D-31). 업무는 마감일(없으면 시작일) 한 칸에 놓는다. 날짜가 없는 업무는 아래에 따로.
  import type { TaskView } from '$lib/api/client';
  import { monthRange, shiftMonth } from '$lib/features/events/calendar';
  import { displaySettings, formatMonth, weekdays } from '$lib/display';
  import { dayIn } from '$lib/features/events/time';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';

  import { tasksByDay } from './filters';
  import { STATUS_LABEL } from './labels';

  interface Props {
    tasks: TaskView[];
    onopen: (id: string) => void;
    /** 날짜 칸에서 그 날짜를 마감일로 하는 업무 만들기(쓸 수 있을 때만). */
    oncreate?: ((day: string) => void) | undefined;
  }

  const { tasks, onopen, oncreate }: Props = $props();

  const settings = displaySettings();
  const today = dayIn(new Date().toISOString(), settings.timeZone);
  let month = $state(today.slice(0, 7));
  const range = $derived(monthRange(month, settings.weekStart));
  const byDay = $derived(tasksByDay(tasks));
  const undated = $derived(tasks.filter((t) => t.due_date === null && t.start_date === null));
  const WEEKDAYS = weekdays(settings).map((w) => w.short);
</script>

<div class="flex flex-col gap-2" data-testid="task-calendar">
  <div class="flex items-center gap-2">
    <Button
      size="sm"
      variant="outline"
      aria-label={m.tasks_prev_month()}
      onclick={() => (month = shiftMonth(month, -1))}>‹</Button
    >
    <span class="font-medium" aria-live="polite">{formatMonth(month, settings)}</span>
    <Button
      size="sm"
      variant="outline"
      aria-label={m.tasks_next_month()}
      onclick={() => (month = shiftMonth(month, 1))}>›</Button
    >
    <Button size="sm" variant="ghost" onclick={() => (month = today.slice(0, 7))}
      >{m.tasks_today()}</Button
    >
  </div>
  <div
    class="grid grid-cols-7 gap-px rounded-md border bg-border text-xs"
    role="grid"
    aria-label={m.tasks_calendar_aria({ month })}
  >
    {#each WEEKDAYS as w (w)}
      <div class="bg-background p-1 text-center font-medium" role="columnheader">{w}</div>
    {/each}
    {#each range.days as day (day)}
      {@const list = byDay.get(day) ?? []}
      <div
        class={[
          'flex min-h-20 flex-col gap-0.5 bg-background p-1',
          day.slice(0, 7) !== month ? 'text-muted-foreground' : '',
          day === today ? 'ring-1 ring-primary ring-inset' : '',
        ]}
        role="gridcell"
        aria-label={m.tasks_calendar_day_aria({ day, count: list.length })}
      >
        <span class="flex items-center justify-between">
          <span>{Number(day.slice(8))}</span>
          {#if oncreate !== undefined}
            {@const create = oncreate}
            <button
              type="button"
              class="rounded px-1 text-muted-foreground hover:bg-accent hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
              aria-label={m.events_task_create_on({ day })}
              title={m.events_task_create_on({ day })}
              data-create-day={day}
              onclick={() => {
                create(day);
              }}>+</button
            >
          {/if}
        </span>
        {#each list as t (t.id)}
          <button
            type="button"
            class={[
              'truncate rounded px-1 text-left hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none',
              t.status === 'done' ? 'line-through' : '',
            ]}
            title={`${t.reference ?? ''} ${t.title} · ${STATUS_LABEL[t.status]}`}
            onclick={() => {
              onopen(t.id);
            }}>{t.title}</button
          >
        {/each}
      </div>
    {/each}
  </div>
  {#if undated.length > 0}
    <p class="text-sm text-muted-foreground">
      {m.tasks_calendar_undated({ count: undated.length })}
    </p>
  {/if}
</div>
