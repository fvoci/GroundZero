<script lang="ts">
  // 간트 보기 (D-31): 업무 막대·마일스톤(◆)·선행 관계. 막대는 보조 표시이고 각 줄의 단추가 같은 내용을 글로
  // 말한다(화면 낭독기·키보드). 날짜를 바꾸는 일은 상세에서 하며, 후속 업무 영향은 저장 전에 묻는다.
  import type { TaskView } from '$lib/api/client';
  import { dayIn, localTimeZone } from '$lib/features/events/time';
  import { Button } from '$lib/ui/button';

  import { dayNumber, fromDayNumber, ganttLayout } from './gantt';
  import { STATUS_LABEL } from './labels';

  interface Props {
    tasks: TaskView[];
    onopen: (taskId: string) => void;
  }

  const { tasks, onopen }: Props = $props();

  const layout = $derived(ganttLayout(tasks));
  const byId = $derived(new Map(tasks.map((t) => [t.id, t])));
  const firstDay = $derived(layout.days === 0 ? 0 : dayNumber(layout.first));
  // 오늘은 사용자 시간대(D-81). 브라우저 지역 시간에 기대지 않는다.
  const today = dayIn(new Date().toISOString(), localTimeZone());
  const todayColumn = $derived(layout.days === 0 ? -1 : dayNumber(today) - firstDay);
  const columns = $derived(
    Array.from({ length: layout.days }, (_, i) => fromDayNumber(firstDay + i)),
  );

  // 가로 스크롤을 키보드로도 할 수 있게 기간 이동 단추를 둔다(막대 영역 자체는 초점을 받지 않는다).
  let scroller = $state<HTMLElement | undefined>(undefined);
  const columnPx = (): number =>
    1.75 * Number.parseFloat(getComputedStyle(document.documentElement).fontSize || '16');
  function scrollBy(days: number): void {
    scroller?.scrollBy({ left: days * columnPx(), behavior: 'smooth' });
  }
  function scrollToToday(): void {
    scroller?.scrollTo({ left: Math.max(0, todayColumn - 3) * columnPx(), behavior: 'smooth' });
  }

  function describe(t: TaskView): string {
    const when =
      t.milestone || t.start_date === null || t.start_date === t.due_date
        ? (t.due_date ?? t.start_date ?? '')
        : `${t.start_date} ~ ${t.due_date ?? t.start_date}`;
    const preds = t.predecessor_ids
      .map((id) => byId.get(id)?.reference ?? '')
      .filter((r) => r !== '');
    return [
      `${t.reference ?? ''} ${t.title}`.trim(),
      t.milestone ? `마일스톤 ${when}` : when,
      STATUS_LABEL[t.status],
      preds.length > 0 ? `선행 ${preds.join(', ')}` : '',
    ]
      .filter((s) => s !== '')
      .join(', ');
  }
</script>

{#if layout.rows.length === 0}
  <p class="text-sm text-muted-foreground">
    날짜가 있는 업무가 없습니다. 업무 상세에서 시작일·마감일을 넣으면 여기에 막대로 보입니다.
  </p>
{:else}
  <div class="flex flex-wrap gap-1" role="group" aria-label="간트 기간 이동">
    <Button
      size="sm"
      variant="outline"
      onclick={() => {
        scrollBy(-14);
      }}>← 2주 앞</Button
    >
    <Button
      size="sm"
      variant="outline"
      onclick={scrollToToday}
      disabled={todayColumn < 0 || todayColumn >= layout.days}>오늘</Button
    >
    <Button
      size="sm"
      variant="outline"
      onclick={() => {
        scrollBy(14);
      }}>2주 뒤 →</Button
    >
  </div>
  <div class="overflow-x-auto rounded-md border" data-testid="task-gantt" bind:this={scroller}>
    <div
      class="grid min-w-max text-xs"
      style:grid-template-columns={`14rem repeat(${String(layout.days)}, 1.75rem)`}
    >
      <div class="sticky left-0 z-10 border-b bg-background p-1 font-medium">업무</div>
      {#each columns as day, i (day)}
        <div
          aria-hidden="true"
          class={[
            'border-b border-l p-0.5 text-center text-muted-foreground',
            i === todayColumn && 'bg-accent font-semibold text-foreground',
          ]}
          title={day}
        >
          {day.endsWith('-01') || i === 0 ? `${String(Number(day.slice(5, 7)))}/` : ''}{Number(
            day.slice(8),
          )}
        </div>
      {/each}

      {#each layout.rows as row (row.task.id)}
        <div class="sticky left-0 z-10 border-b bg-background p-1">
          <button
            type="button"
            data-task-card={row.task.id}
            class="w-full truncate rounded px-1 text-left hover:underline focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
            aria-label={describe(row.task)}
            onclick={() => {
              onopen(row.task.id);
            }}
          >
            {#if row.task.parent_id !== null}↳{/if}
            <span class="font-mono text-muted-foreground">{row.task.reference ?? ''}</span>
            {row.task.title}
          </button>
        </div>
        <div
          aria-hidden="true"
          class="relative border-b"
          style:grid-column={`2 / span ${String(layout.days)}`}
        >
          {#if todayColumn >= 0 && todayColumn < layout.days}
            <div
              class="absolute inset-y-0 w-px bg-primary/40"
              style:left={`calc(${String(todayColumn)} * 1.75rem + 0.875rem)`}
            ></div>
          {/if}
          {#if row.task.milestone}
            <div
              class="absolute top-1/2 size-3 -translate-x-1/2 -translate-y-1/2 rotate-45 bg-primary"
              style:left={`calc(${String(row.from)} * 1.75rem + 0.875rem)`}
            ></div>
          {:else}
            <div
              class={[
                'absolute inset-y-1 rounded-sm',
                row.task.status === 'done' ? 'bg-muted-foreground/40' : 'bg-primary/70',
              ]}
              style:left={`calc(${String(row.from)} * 1.75rem + 0.125rem)`}
              style:width={`calc(${String(row.to - row.from + 1)} * 1.75rem - 0.25rem)`}
            ></div>
          {/if}
        </div>
      {/each}
    </div>
  </div>
{/if}
{#if layout.later.length > 0}
  <p class="text-sm text-muted-foreground">
    {layout.days}일 뒤에 시작하는 업무 {layout.later.length}개는 목록 보기에서 볼 수 있습니다.
  </p>
{/if}
{#if layout.undated.length > 0}
  <p class="text-sm text-muted-foreground">
    날짜 없는 업무 {layout.undated.length}개는 목록 보기에서 볼 수 있습니다.
  </p>
{/if}
