<script lang="ts">
  // 간트 보기 (D-31): 업무 막대·마일스톤(◆)·선행 관계. 막대는 보조 표시이고 각 줄의 단추가 같은 내용을 글로
  // 말한다(화면 낭독기·키보드). 쓸 수 있으면 막대를 끌어 일정을 옮기고 양 끝을 끌어 시작·마감을 바꾼다. 키보드는 줄
  // 단추에서 Alt+←·→(하루 옮기기), Alt+Shift+←·→(마감 하루). 놓으면 부르는 쪽이 최신 업무로 후속 업무 영향을
  // 미리 보고 승인을 받은 뒤 저장한다(상세의 날짜 편집과 같은 흐름). 끝날 때까지 막대는 바꾼 자리를 미리 보인다.
  import type { TaskView } from '$lib/api/client';
  import { dayIn, localTimeZone } from '$lib/features/events/time';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';

  import type { GanttEdit, GanttLink } from './gantt';
  import { dayNumber, editDates, fromDayNumber, ganttLayout, ganttLinks } from './gantt';
  import { STATUS_LABEL } from './labels';

  interface Props {
    tasks: TaskView[];
    onopen: (taskId: string) => void;
    /** 일정 바꾸기(끝나면 돌아온다). 없으면 보기만 한다. */
    reschedule?:
      | ((
          task: TaskView,
          dates: { start_date: string | null; due_date: string | null },
        ) => Promise<void>)
      | undefined;
  }

  const { tasks, onopen, reschedule }: Props = $props();

  const editable = (t: TaskView): boolean => reschedule !== undefined && t.access !== 'read';
  // 끄는 중이거나 저장을 기다리는 막대의 미리 보기(열 번호).
  let draft = $state<{ id: string; from: number; to: number } | undefined>(undefined);
  let busy = $state(false);
  let drag:
    | {
        id: string;
        edit: GanttEdit;
        pointer: number;
        x: number;
        from: number;
        to: number;
        days: number;
      }
    | undefined;

  function preview(
    from: number,
    to: number,
    edit: GanttEdit,
    days: number,
  ): { from: number; to: number } {
    if (edit === 'move') return { from: from + days, to: to + days };
    if (edit === 'start') return { from: Math.min(from + days, to), to };
    return { from, to: Math.max(to + days, from) };
  }

  async function commit(task: TaskView, edit: GanttEdit, days: number): Promise<void> {
    const dates = editDates(task, edit, days);
    if (dates === undefined || reschedule === undefined) {
      draft = undefined;
      return;
    }
    busy = true;
    try {
      await reschedule(task, dates);
    } finally {
      busy = false;
      draft = undefined;
    }
  }

  function start(
    event: PointerEvent,
    row: { task: TaskView; from: number; to: number },
    edit: GanttEdit,
  ): void {
    if (!editable(row.task) || busy || event.button !== 0) return;
    event.preventDefault();
    event.stopPropagation();
    if (event.currentTarget instanceof Element)
      event.currentTarget.setPointerCapture(event.pointerId);
    drag = {
      id: row.task.id,
      edit,
      pointer: event.pointerId,
      x: event.clientX,
      from: row.from,
      to: row.to,
      days: 0,
    };
    draft = { id: row.task.id, from: row.from, to: row.to };
  }
  function move(event: PointerEvent): void {
    const d = drag;
    if (d?.pointer !== event.pointerId) return;
    if (event.buttons === 0) {
      cancel();
      return;
    }
    d.days = Math.round((event.clientX - d.x) / columnPx());
    draft = { id: d.id, ...preview(d.from, d.to, d.edit, d.days) };
  }
  function end(event: PointerEvent): void {
    const d = drag;
    if (d?.pointer !== event.pointerId) return;
    drag = undefined;
    const task = tasks.find((t) => t.id === d.id);
    if (task === undefined || d.days === 0) {
      draft = undefined;
      return;
    }
    void commit(task, d.edit, d.days);
  }
  function cancel(): void {
    if (drag === undefined) return;
    drag = undefined;
    draft = undefined;
  }
  $effect(() => {
    const onKey = (e: KeyboardEvent): void => {
      if (e.key === 'Escape') cancel();
    };
    window.addEventListener('keydown', onKey);
    window.addEventListener('blur', cancel);
    return () => {
      window.removeEventListener('keydown', onKey);
      window.removeEventListener('blur', cancel);
    };
  });

  function onRowKey(event: KeyboardEvent, task: TaskView): void {
    if (!event.altKey || !editable(task) || busy) return;
    const step = event.key === 'ArrowRight' ? 1 : event.key === 'ArrowLeft' ? -1 : 0;
    if (step === 0) return;
    event.preventDefault();
    void commit(task, event.shiftKey ? 'end' : 'move', step);
  }

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
  let width = $state(0);
  // 보이는 폭을 따라간다. 크기 관찰을 못 하는 환경에서는 기본 열 너비로 둔다.
  $effect(() => {
    const el = scroller;
    if (el === undefined || typeof ResizeObserver === 'undefined') return;
    const observer = new ResizeObserver(() => {
      width = el.clientWidth;
    });
    observer.observe(el);
    return () => {
      observer.disconnect();
    };
  });
  const remPx = (): number =>
    Number.parseFloat(getComputedStyle(document.documentElement).fontSize || '16') || 16;
  // 날짜 열 너비(rem). 기본 1.75rem이고, 기간이 짧아 오른쪽이 비면 남는 폭을 날짜 열에 나눈다(최대 5rem).
  const col = $derived(
    layout.days === 0 || width === 0
      ? 1.75
      : Math.min(5, Math.max(1.75, (width / remPx() - 14) / layout.days)),
  );
  const columnPx = (): number => col * remPx();
  const weekend = (day: string): boolean => {
    const d = new Date(`${day}T00:00:00Z`).getUTCDay();
    return d === 0 || d === 6;
  };
  // 월 머리글: 날짜 칸 위에 따로 한 줄로 둬서 "10/21" 같은 글이 좁은 날짜 칸을 넘치지 않게 한다.
  const months = $derived.by(() => {
    const out: { key: string; label: string; start: number; span: number }[] = [];
    columns.forEach((day, i) => {
      const key = day.slice(0, 7);
      const last = out.at(-1);
      if (last?.key === key) last.span += 1;
      else
        out.push({
          key,
          label: m.tasks_gantt_month({
            year: day.slice(0, 4),
            month: String(Number(day.slice(5, 7))),
          }),
          start: i,
          span: 1,
        });
    });
    return out;
  });
  function scrollBy(days: number): void {
    scroller?.scrollBy({ left: days * columnPx(), behavior: 'smooth' });
  }
  function scrollToToday(): void {
    scroller?.scrollTo({ left: Math.max(0, todayColumn - 3) * columnPx(), behavior: 'smooth' });
  }

  // 줄 높이(rem). 열 너비와 같은 단위로 고정해 연결선이 막대와 같은 좌표계를 쓴다(창 크기·확대와 상관없이 맞음).
  const ROW = 1.75;
  // 연결선(끝나야 시작, D-31): 끄는 중이거나 승인을 기다리는 막대는 미리 보기 자리로 잇는다(막대와 같은 값).
  const linkRows = $derived(
    layout.rows.map((r) =>
      draft?.id === r.task.id ? { ...r, from: draft.from, to: draft.to } : r,
    ),
  );
  const linked = $derived(ganttLinks(linkRows, { col, row: ROW, days: layout.days }, tasks));
  // 줄 이름에 초점을 두거나 가리키면 그 업무에 닿는 선을 굵게 하고 나머지는 옅게(색만으로 구별하지 않음).
  let focusId = $state<string | undefined>(undefined);
  let hoverId = $state<string | undefined>(undefined);
  // 가리킨 줄이 있으면 그 줄, 없으면 초점이 있는 줄(따로 두어 한쪽이 끝나도 다른 쪽 강조가 남는다).
  const activeId = $derived(hoverId ?? focusId);
  const touches = (link: GanttLink): boolean => link.from === activeId || link.to === activeId;

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
      t.milestone ? m.tasks_gantt_milestone({ date: when }) : when,
      STATUS_LABEL[t.status],
      preds.length > 0 ? m.tasks_gantt_predecessors({ refs: preds.join(', ') }) : '',
    ]
      .filter((s) => s !== '')
      .join(', ');
  }
</script>

{#if layout.rows.length === 0}
  <p class="text-sm text-muted-foreground">{m.tasks_gantt_empty()}</p>
{:else}
  <div class="flex flex-wrap gap-1" role="group" aria-label={m.tasks_gantt_nav()}>
    <Button
      size="sm"
      variant="outline"
      onclick={() => {
        scrollBy(-14);
      }}>{m.tasks_gantt_back()}</Button
    >
    <Button
      size="sm"
      variant="outline"
      onclick={scrollToToday}
      disabled={todayColumn < 0 || todayColumn >= layout.days}>{m.tasks_today()}</Button
    >
    <Button
      size="sm"
      variant="outline"
      onclick={() => {
        scrollBy(14);
      }}>{m.tasks_gantt_forward()}</Button
    >
  </div>
  <div class="overflow-x-auto rounded-md border" data-testid="task-gantt" bind:this={scroller}>
    <div
      class="grid min-w-max text-xs"
      style:grid-template-columns={`14rem repeat(${String(layout.days)}, ${String(col)}rem)`}
      style:grid-template-rows={`auto auto repeat(${String(layout.rows.length)}, ${String(ROW)}rem)`}
    >
      <div
        class="sticky left-0 z-10 flex items-end border-b bg-background p-1 font-medium"
        style:grid-row="1 / span 2"
        style:grid-column="1"
      >
        {m.tasks_task()}
      </div>
      {#each months as seg (seg.key)}
        <div
          aria-hidden="true"
          class="truncate border-l px-1 pt-1 font-medium text-muted-foreground"
          style:grid-row="1"
          style:grid-column={`${String(seg.start + 2)} / span ${String(seg.span)}`}
        >
          {seg.label}
        </div>
      {/each}
      {#each columns as day, i (day)}
        <div
          aria-hidden="true"
          class={[
            'flex justify-center overflow-hidden border-b border-l py-0.5 text-muted-foreground tabular-nums',
            weekend(day) && 'bg-muted/60',
          ]}
          style:grid-row="2"
          style:grid-column={String(i + 2)}
          title={day}
          data-gantt-day={day}
        >
          <span
            class={[
              'min-w-5 rounded-full px-1 text-center',
              i === todayColumn && 'bg-primary font-semibold text-primary-foreground',
            ]}>{Number(day.slice(8))}</span
          >
        </div>
      {/each}

      {#each layout.rows as row, index (row.task.id)}
        {@const span = draft?.id === row.task.id ? draft : row}
        <!-- 줄 번호를 정해 둔다: 연결선 층이 같은 칸을 덮어도 격자가 줄을 다른 자리로 밀지 않게. -->
        <div
          class="sticky left-0 z-10 overflow-hidden border-b bg-background p-1"
          style:grid-row={String(index + 3)}
          style:grid-column="1"
        >
          <button
            type="button"
            data-task-card={row.task.id}
            class="w-full truncate rounded px-1 text-left hover:underline focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
            aria-label={describe(row.task)}
            onfocus={() => (focusId = row.task.id)}
            onblur={() => (focusId = undefined)}
            onmouseenter={() => (hoverId = row.task.id)}
            onmouseleave={() => (hoverId = undefined)}
            aria-keyshortcuts={editable(row.task)
              ? 'Alt+ArrowLeft Alt+ArrowRight Alt+Shift+ArrowLeft Alt+Shift+ArrowRight'
              : undefined}
            onclick={() => {
              onopen(row.task.id);
            }}
            onkeydown={(e: KeyboardEvent) => {
              onRowKey(e, row.task);
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
          style:grid-row={String(index + 3)}
          style:grid-column={`2 / span ${String(layout.days)}`}
        >
          {#each columns as day, i (day)}
            {#if weekend(day)}
              <div
                class="absolute inset-y-0 bg-muted/60"
                style:left={`${String(i * col)}rem`}
                style:width={`${String(col)}rem`}
              ></div>
            {/if}
          {/each}
          {#if todayColumn >= 0 && todayColumn < layout.days}
            <div
              class="absolute inset-y-0 w-px bg-primary/40"
              style:left={`calc(${String(todayColumn)} * ${String(col)}rem + ${String(col / 2)}rem)`}
            ></div>
          {/if}
          {#if row.task.milestone}
            <div
              class={[
                'absolute top-1/2 z-[2] size-3 -translate-x-1/2 -translate-y-1/2 rotate-45 bg-primary',
                editable(row.task) && 'cursor-grab touch-none',
              ]}
              style:left={`calc(${String(span.from)} * ${String(col)}rem + ${String(col / 2)}rem)`}
              data-gantt-bar={row.task.id}
              role="presentation"
              onpointerdown={(e: PointerEvent) => {
                start(e, row, 'move');
              }}
              onpointermove={move}
              onpointerup={end}
              onpointercancel={cancel}
            ></div>
          {:else}
            <div
              class={[
                'absolute inset-y-1 z-[2] rounded-sm',
                row.task.status === 'done' ? 'bg-muted-foreground/40' : 'bg-primary/70',
                editable(row.task) && 'cursor-grab touch-none',
                draft?.id === row.task.id && 'ring-2 ring-ring',
              ]}
              style:left={`calc(${String(span.from)} * ${String(col)}rem + 0.125rem)`}
              style:width={`calc(${String(span.to - span.from + 1)} * ${String(col)}rem - 0.25rem)`}
              data-gantt-bar={row.task.id}
              role="presentation"
              onpointerdown={(e: PointerEvent) => {
                start(e, row, 'move');
              }}
              onpointermove={move}
              onpointerup={end}
              onpointercancel={cancel}
            >
              {#if editable(row.task)}
                <div
                  class="absolute inset-y-0 left-0 w-2 cursor-ew-resize touch-none"
                  data-gantt-edge="start"
                  role="presentation"
                  onpointerdown={(e: PointerEvent) => {
                    start(e, row, 'start');
                  }}
                  onpointermove={move}
                  onpointerup={end}
                  onpointercancel={cancel}
                ></div>
                <div
                  class="absolute inset-y-0 right-0 w-2 cursor-ew-resize touch-none"
                  data-gantt-edge="end"
                  role="presentation"
                  onpointerdown={(e: PointerEvent) => {
                    start(e, row, 'end');
                  }}
                  onpointermove={move}
                  onpointerup={end}
                  onpointercancel={cancel}
                ></div>
              {/if}
            </div>
          {/if}
        </div>
      {/each}
      {#if linked.links.length > 0}
        <!-- 선행 → 후속 연결선. 막대 줄들과 같은 격자 칸을 덮고, 막대(z-2) 아래·주말 음영과 오늘 선 위에 그린다. 누르기·끌기·
             스크롤은 그대로 아래로 지나간다(pointer-events-none). 같은 내용은 줄 단추의 설명("선행: …")이 말하므로 낭독하지
             않는다. 좌표는 rem이고 viewBox 단위 하나가 1rem이다(선 두께는 확대와 상관없이 일정). -->
        <div
          class="pointer-events-none relative z-[1]"
          style:grid-row={`3 / span ${String(layout.rows.length)}`}
          style:grid-column={`2 / span ${String(layout.days)}`}
          data-testid="gantt-links"
        >
          <svg
            class="absolute inset-0 size-full overflow-visible"
            viewBox={`0 0 ${String(layout.days * col)} ${String(layout.rows.length * ROW)}`}
            preserveAspectRatio="none"
            aria-hidden="true"
            focusable="false"
          >
            {#each linked.links as link (link.key)}
              {@const strong = activeId !== undefined && touches(link)}
              {@const faint = activeId !== undefined && !touches(link)}
              <g
                class={[
                  'transition-opacity',
                  strong ? 'text-foreground' : 'text-foreground/60',
                  faint && 'opacity-25',
                ]}
                data-gantt-link={link.key}
                data-strong={strong ? 'true' : undefined}
              >
                <path
                  d={link.d}
                  fill="none"
                  stroke="currentColor"
                  stroke-width={strong ? 2.5 : 1.5}
                  stroke-linejoin="round"
                  vector-effect="non-scaling-stroke"
                />
                <path d={link.arrow} fill="currentColor" />
              </g>
            {/each}
          </svg>
        </div>
      {/if}
    </div>
  </div>
  {#if linked.links.length > 0}
    <p class="text-xs text-muted-foreground">{m.tasks_gantt_links_hint()}</p>
  {/if}
  {#if linked.hidden > 0}
    <p class="text-xs text-muted-foreground" data-testid="gantt-links-hidden">
      {m.tasks_gantt_links_hidden({ count: linked.hidden })}
    </p>
  {/if}
{/if}
{#if reschedule !== undefined && layout.rows.length > 0}
  <p class="text-xs text-muted-foreground">{m.tasks_gantt_edit_hint()}</p>
{/if}
{#if layout.later.length > 0}
  <p class="text-sm text-muted-foreground">
    {m.tasks_gantt_later({ days: layout.days, count: layout.later.length })}
  </p>
{/if}
{#if layout.undated.length > 0}
  <p class="text-sm text-muted-foreground">
    {m.tasks_gantt_undated({ count: layout.undated.length })}
  </p>
{/if}
