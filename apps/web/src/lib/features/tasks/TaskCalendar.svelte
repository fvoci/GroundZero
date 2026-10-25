<script lang="ts">
  // 항목 업무의 달력 보기(D-31). 시작·마감이 있으면 기간 막대, 한 날짜만 있으면 점으로 놓는다. 날짜가 없는 업무는 아래에 따로.
  import type { TaskView } from '$lib/api/client';
  import { isWeekend, monthRange, shiftMonth } from '$lib/features/events/calendar';
  import { displaySettings, formatDay, formatMonth, weekdays } from '$lib/display';
  import { dayIn } from '$lib/features/events/time';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';

  import { tasksByDay } from './filters';

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
  const byDay = $derived(tasksByDay(tasks, range.days));
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
  <!-- 방향키로 칸을 옮기지 않으므로 grid 역할을 쓰지 않는다. 날짜 칸마다 이름(요일 포함 날짜·업무 수)이 있어 요일
       머리는 보이기만 한다. -->
  <div
    class="grid grid-cols-7 gap-px rounded-md border bg-border text-xs"
    role="group"
    aria-label={m.tasks_calendar_aria({ month })}
  >
    {#each WEEKDAYS as w (w)}
      <div class="bg-background p-1 text-center font-medium" aria-hidden="true">{w}</div>
    {/each}
    {#each range.days as day (day)}
      {@const list = byDay.get(day) ?? []}
      <div
        class={[
          'group flex min-h-20 min-w-0 flex-col gap-0.5 p-1',
          day.slice(0, 7) !== month
            ? 'bg-muted text-muted-foreground'
            : isWeekend(day)
              ? 'bg-muted/50'
              : 'bg-background',
        ]}
        role="group"
        aria-label={m.tasks_calendar_day_aria({
          day: formatDay(day, { month: 'long', day: 'numeric', weekday: 'long' }, settings),
          count: list.length,
        })}
      >
        <!-- 오늘은 날짜를 채운 원으로(칸 테두리 대신), 만들기 단추는 마우스 화면에서 가리키거나 초점일 때만. 좁은 칸(375px·200%)에서는
             만들기 단추가 다음 줄로 내려간다(가로 넘침 금지). -->
        <span class="flex flex-wrap items-center justify-between">
          <span
            class={[
              'min-w-5 rounded-full px-1.5 text-center tabular-nums',
              day === today && 'bg-primary font-semibold text-primary-foreground',
            ]}>{Number(day.slice(8))}</span
          >
          {#if oncreate !== undefined}
            {@const create = oncreate}
            <button
              type="button"
              class="rounded px-1 text-muted-foreground transition-opacity hover:bg-accent hover:text-foreground focus:opacity-100 focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none pointer-fine:opacity-0 pointer-fine:group-hover:opacity-100"
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
              t.status_category === 'done' ? 'line-through' : '',
              t.start_date !== null &&
                t.due_date !== null &&
                'rounded-none bg-primary/15 border-y border-primary/30',
            ]}
            data-span={t.start_date !== null && t.due_date !== null ? 'range' : 'point'}
            title={`${t.start_date ?? t.due_date ?? ''} – ${t.due_date ?? t.start_date ?? ''} · ${t.reference ?? ''} ${t.title} · ${t.status}`}
            aria-label={`${t.title} · ${t.status}`}
            onclick={() => {
              onopen(t.id);
            }}>{t.start_date === null || t.due_date === null ? '• ' : ''}{t.title}</button
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
