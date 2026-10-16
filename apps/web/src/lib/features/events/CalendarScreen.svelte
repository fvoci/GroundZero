<script lang="ts">
  // 워크스페이스 달력 (D-31, D-50): 볼 수 있는 업무(마감·시작)와 일정을 달 단위로. 넓은 화면은 달 격자, 좁은 화면은
  // 날짜별 목록으로 같은 내용을 보인다. 업무는 그 업무 상세로, 일정은 일정 대화상자로 연다.
  import type { Workspace } from '@fvoci/api-client';

  import type { CalendarView } from '$lib/api/client';
  import { api, ApiError } from '$lib/api/client';
  import { itemTaskPath, router } from '$lib/router.svelte';
  import { Button } from '$lib/ui/button';

  import type { CalendarEntry } from './calendar';
  import { entriesByDay, monthRange, shiftMonth } from './calendar';
  import CalendarEntryButton from './CalendarEntryButton.svelte';
  import EventDialog from './EventDialog.svelte';
  import { localTimeZone } from './time';

  interface Props {
    workspace: Workspace;
  }

  const { workspace }: Props = $props();

  const zone = localTimeZone();
  const now = new Date();
  const today = `${String(now.getFullYear())}-${String(now.getMonth() + 1).padStart(2, '0')}-${String(now.getDate()).padStart(2, '0')}`;
  let month = $state(today.slice(0, 7));
  let data = $state<CalendarView | undefined>(undefined);
  let error = $state<string | undefined>(undefined);
  let openEvent = $state<string | undefined>(undefined);
  let returnTo: string | undefined;
  let reloadKey = $state(0);

  const range = $derived(monthRange(month));
  const byDay = $derived(
    data === undefined
      ? new Map<string, CalendarEntry[]>()
      : entriesByDay(data.tasks, data.events, range.from, range.to, zone),
  );
  const itemTitle = $derived(new Map((data?.items ?? []).map((i) => [i.id, i.title])));
  const weeks = $derived(Array.from({ length: 6 }, (_, w) => range.days.slice(w * 7, w * 7 + 7)));
  const title = $derived(`${month.slice(0, 4)}년 ${String(Number(month.slice(5, 7)))}월`);

  // 달·워크스페이스가 바뀌거나 일정이 바뀌면(reloadKey) 다시 불러온다.
  const request = $derived({ from: range.from, to: range.to, ws: workspace.id, key: reloadKey });

  $effect(() => {
    const { from, to, ws } = request;
    let cancelled = false;
    api
      .calendar(ws, from, to, zone)
      .then((c) => {
        if (cancelled) return;
        data = c;
        error = undefined;
      })
      .catch((e: unknown) => {
        if (!cancelled) error = e instanceof ApiError ? e.message : '달력을 불러오지 못했습니다.';
      });
    return () => {
      cancelled = true;
    };
  });

  function open(entry: CalendarEntry, key: string): void {
    returnTo = key;
    if (entry.kind === 'task')
      router.navigate(itemTaskPath(workspace.id, entry.task.item_id, entry.task.id));
    else openEvent = entry.event.id;
  }

  const entryKey = (day: string, entry: CalendarEntry): string =>
    `${day}:${entry.kind === 'task' ? entry.task.id : entry.event.id}`;
  const itemOf = (entry: CalendarEntry): string =>
    itemTitle.get(entry.kind === 'task' ? entry.task.item_id : entry.event.item_id) ?? '';
  const WEEKDAYS = ['일', '월', '화', '수', '목', '금', '토'];
</script>

<section class="flex flex-col gap-3" aria-labelledby="calendar-heading">
  <div class="flex flex-wrap items-center gap-2">
    <h1 id="calendar-heading" class="text-xl font-semibold">{title}</h1>
    <div class="ml-auto flex gap-1" role="group" aria-label="달 이동">
      <Button size="sm" variant="outline" onclick={() => (month = shiftMonth(month, -1))}
        >← 이전 달</Button
      >
      <Button size="sm" variant="outline" onclick={() => (month = today.slice(0, 7))}
        >이번 달</Button
      >
      <Button size="sm" variant="outline" onclick={() => (month = shiftMonth(month, 1))}
        >다음 달 →</Button
      >
    </div>
  </div>
  <p class="text-sm text-muted-foreground">
    볼 수 있는 업무의 마감일(◆ 마일스톤)과 일정을 보입니다. 시각은 {zone} 기준입니다.
  </p>
  {#if error !== undefined}
    <p role="alert" class="text-sm text-destructive">{error}</p>
  {/if}

  <table
    class="hidden w-full table-fixed border-collapse text-sm md:table"
    data-testid="calendar-grid"
  >
    <caption class="sr-only">{title} 달력</caption>
    <thead>
      <tr>
        {#each WEEKDAYS as w (w)}
          <th scope="col" class="border-b p-1 text-center font-medium text-muted-foreground">{w}</th
          >
        {/each}
      </tr>
    </thead>
    <tbody>
      {#each weeks as week (week[0])}
        <tr>
          {#each week as day (day)}
            {@const entries = byDay.get(day) ?? []}
            <td
              class={[
                'h-24 border p-1 align-top',
                day.slice(0, 7) !== month && 'bg-muted/40 text-muted-foreground',
              ]}
            >
              <div
                class={[
                  'mb-1 text-xs',
                  day === today &&
                    'inline-block rounded-full bg-primary px-1.5 text-primary-foreground',
                ]}
              >
                <span aria-hidden="true">{Number(day.slice(8))}</span>
                <span class="sr-only">{day}{day === today ? ' 오늘' : ''}</span>
              </div>
              <div class="flex flex-col gap-0.5">
                {#each entries as entry (entryKey(day, entry))}
                  <CalendarEntryButton
                    {entry}
                    entryKey={entryKey(day, entry)}
                    itemTitle={itemOf(entry)}
                    onopen={() => {
                      open(entry, entryKey(day, entry));
                    }}
                  />
                {/each}
              </div>
            </td>
          {/each}
        </tr>
      {/each}
    </tbody>
  </table>

  <ol class="flex flex-col gap-2 md:hidden" data-testid="calendar-agenda">
    {#each range.days.filter((d) => d.slice(0, 7) === month && (byDay.get(d)?.length ?? 0) > 0) as day (day)}
      <li class="flex flex-col gap-1">
        <h2 class="text-sm font-semibold">{day}{day === today ? ' (오늘)' : ''}</h2>
        {#each byDay.get(day) ?? [] as entry (entryKey(day, entry))}
          <CalendarEntryButton
            {entry}
            entryKey={entryKey(`m${day}`, entry)}
            itemTitle={itemOf(entry)}
            onopen={() => {
              open(entry, entryKey(`m${day}`, entry));
            }}
          />
        {/each}
      </li>
    {:else}
      <li class="text-sm text-muted-foreground">이 달에는 업무 마감이나 일정이 없습니다.</li>
    {/each}
  </ol>
</section>

<EventDialog
  eventId={openEvent}
  onclose={() => (openEvent = undefined)}
  onchanged={() => (reloadKey += 1)}
  returnFocus={() =>
    returnTo === undefined
      ? null
      : document.querySelector<HTMLElement>(`[data-calendar-entry="${CSS.escape(returnTo)}"]`)}
/>
