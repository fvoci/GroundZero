<script lang="ts">
  // 달력 (D-31, D-50, D-80): 볼 수 있는 업무(마감·시작)·일정·강의·스터디 계획일·투고·산출물 날짜를 달 단위로.
  // 같은 달력을 워크스페이스 전체(모듈·항목 거르기), 모듈 화면(그 모듈), 항목 화면(그 항목)에서 범위만 바꿔 쓴다.
  // 넓은 화면은 달 격자, 좁은 화면은 날짜별 목록으로 같은 내용을 보인다. 업무는 그 업무 상세로, 일정은 일정 대화상자로,
  // 그 밖의 날짜는 그 항목으로 연다.
  import { tick } from 'svelte';

  import type { Module, Workspace } from '@fvoci/api-client';

  import type { CalendarView, ItemView, TaskDetailView } from '$lib/api/client';
  import { api } from '$lib/api/client';
  import { notifyDone } from '$lib/features/feedback/feedback.svelte';
  import { errorText } from '$lib/api/errors';
  import { displaySettings, formatDay, formatMonth, weekdays } from '$lib/display';
  import { m } from '$lib/paraglide/messages.js';
  import { itemPath, itemTaskPath, router } from '$lib/router.svelte';
  import { Button } from '$lib/ui/button';

  import type { CalendarEntry } from './calendar';
  import { entriesByDay, monthRange, shiftMonth } from './calendar';
  import CalendarEntryButton from './CalendarEntryButton.svelte';
  import CalendarTaskDialog from './CalendarTaskDialog.svelte';
  import EventDialog from './EventDialog.svelte';
  import { dayIn } from './time';

  interface Props {
    workspace: Workspace;
    /** 정한 범위(모듈 화면·항목 화면). 없으면 워크스페이스 전체이고 모듈·항목 거르기를 보인다. */
    scope?: { moduleId?: string | undefined; itemId?: string | undefined } | undefined;
    /** 다른 화면 안에 넣을 때(제목 단계를 낮춘다). */
    embedded?: boolean;
    /** 같은 화면의 다른 곳(항목 일정 목록)에서 일정이 바뀔 때마다 늘어난다(다시 읽기). */
    version?: number;
    /** 달력에서 일정을 고치거나 지웠다(같은 화면의 일정 목록이 다시 읽게). */
    onchanged?: () => void;
    /** 항목 범위에서 날짜 칸으로 업무를 만들 항목. 쓸 수 없으면 만들기 단추를 보이지 않는다. */
    item?: ItemView | undefined;
  }

  const { workspace, scope, embedded = false, version = 0, onchanged, item }: Props = $props();

  // 날짜 칸에서 업무 만들기(D-31·D-80). 항목 범위면 그 항목(쓸 수 있을 때만), 모듈·워크스페이스 범위면 창에서 고른다.
  const canCreate = $derived(
    scope?.itemId === undefined
      ? true
      : item !== undefined && item.access !== 'read' && !item.archived && !item.is_template,
  );
  let createDay = $state<string | undefined>(undefined);

  // 워크스페이스 달력의 거르기: 모듈 → 그 모듈의 항목.
  let modules = $state<Module[]>([]);
  let moduleItems = $state<ItemView[]>([]);
  let filterModule = $state('');
  let filterItem = $state('');
  $effect(() => {
    if (scope !== undefined) return;
    const ws = workspace.id;
    let cancelled = false;
    api
      .modules(ws)
      .then((list) => {
        if (!cancelled) modules = list;
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  });
  $effect(() => {
    const id = filterModule;
    filterItem = '';
    moduleItems = [];
    if (id === '') return;
    let cancelled = false;
    api
      .items(id)
      .then((list) => {
        if (!cancelled) moduleItems = list;
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  });
  const effectiveScope = $derived(
    scope ?? {
      moduleId: filterModule === '' ? undefined : filterModule,
      itemId: filterItem === '' ? undefined : filterItem,
    },
  );

  // 오늘·주 시작·달 이름은 사용자 표시 설정을 따른다(D-81). 브라우저 지역 시간에 기대지 않는다.
  const settings = displaySettings();
  const zone = settings.timeZone;
  const today = dayIn(new Date().toISOString(), zone);
  let month = $state(today.slice(0, 7));
  let data = $state<CalendarView | undefined>(undefined);
  let error = $state<string | undefined>(undefined);
  let openEvent = $state<string | undefined>(undefined);
  let openOccurrence = $state<string | undefined>(undefined);
  let returnTo: string | undefined;
  let changed = false;
  let reloadKey = $state(0);

  const range = $derived(monthRange(month, settings.weekStart));
  const byDay = $derived(
    data === undefined
      ? new Map<string, CalendarEntry[]>()
      : entriesByDay(
          data.tasks,
          data.projected_tasks,
          data.events,
          data.occurrences,
          range.from,
          range.to,
          zone,
          data.dates,
        ),
  );
  const itemTitle = $derived(new Map((data?.items ?? []).map((i) => [i.id, i.title])));
  const weeks = $derived(Array.from({ length: 6 }, (_, w) => range.days.slice(w * 7, w * 7 + 7)));
  const title = $derived(formatMonth(month, settings));

  // 달·워크스페이스가 바뀌거나 일정이 바뀌면(reloadKey) 다시 불러온다.
  const request = $derived({
    from: range.from,
    to: range.to,
    ws: workspace.id,
    scope: effectiveScope,
    key: reloadKey + version,
  });

  $effect(() => {
    const { from, to, ws, scope: within } = request;
    let cancelled = false;
    api
      .calendar(ws, from, to, zone, within)
      .then((c) => {
        if (cancelled) return;
        data = c;
        error = undefined;
        // 지운 일정의 칸처럼 초점이 있던 요소가 사라졌으면 달력 제목으로 돌린다.
        void tick().then(() => {
          if (changed && document.activeElement === document.body)
            document.getElementById('calendar-heading')?.focus();
          changed = false;
        });
      })
      .catch((e: unknown) => {
        changed = false;
        if (!cancelled) error = errorText(e, m.events_load_failed());
      });
    return () => {
      cancelled = true;
    };
  });

  function open(entry: CalendarEntry, key: string): void {
    returnTo = key;
    if (entry.kind === 'task')
      router.navigate(itemTaskPath(workspace.id, entry.task.item_id, entry.task.id));
    // 예정 회차는 아직 업무가 아니다. 반복의 첫 회차(틀)를 연다(쓰기 권한으로 항목을 열면 회차가 만들어진다).
    else if (entry.kind === 'projected')
      router.navigate(
        itemTaskPath(workspace.id, entry.projected.item_id, entry.projected.root_task_id),
      );
    else if (entry.kind === 'date') router.navigate(itemPath(workspace.id, entry.date.item_id));
    else {
      openEvent = entry.event.id;
      openOccurrence = entry.event.recurrence === null ? undefined : entry.occurrence;
    }
  }

  const entryKey = (day: string, entry: CalendarEntry): string =>
    `${day}:${
      entry.kind === 'task'
        ? entry.task.id
        : entry.kind === 'projected'
          ? `${entry.projected.series_id}:${entry.projected.occurrence}`
          : entry.kind === 'date'
            ? `${entry.date.kind}:${entry.date.id}`
            : `${entry.event.id}:${entry.occurrence}`
    }`;
  const itemOf = (entry: CalendarEntry): string =>
    itemTitle.get(
      entry.kind === 'task'
        ? entry.task.item_id
        : entry.kind === 'projected'
          ? entry.projected.item_id
          : entry.kind === 'date'
            ? entry.date.item_id
            : entry.event.item_id,
    ) ?? '';
  const fieldBox =
    'h-8 rounded-md border border-input bg-transparent px-2 text-sm focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none';
  const WEEKDAYS = weekdays(settings).map((w) => w.short);
</script>

<section class="flex flex-col gap-3" aria-labelledby="calendar-heading">
  <div class="flex flex-wrap items-center gap-2">
    {#if embedded}
      <h2 id="calendar-heading" class="text-lg font-semibold" tabindex="-1">
        {m.events_calendar_embedded({ title })}
      </h2>
    {:else}
      <h1 id="calendar-heading" class="text-xl font-semibold" tabindex="-1">{title}</h1>
    {/if}
    {#if canCreate}
      <Button size="sm" variant="outline" onclick={() => (createDay = today)}
        >{m.events_task_create()}</Button
      >
    {/if}
    <div class="ml-auto flex gap-1" role="group" aria-label={m.events_month_nav()}>
      <Button size="sm" variant="outline" onclick={() => (month = shiftMonth(month, -1))}
        >{m.events_prev_month()}</Button
      >
      <Button size="sm" variant="outline" onclick={() => (month = today.slice(0, 7))}
        >{m.events_this_month()}</Button
      >
      <Button size="sm" variant="outline" onclick={() => (month = shiftMonth(month, 1))}
        >{m.events_next_month()}</Button
      >
    </div>
  </div>
  {#if scope === undefined}
    <div class="flex flex-wrap items-end gap-2" role="group" aria-label={m.events_scope()}>
      <label class="flex flex-col gap-1 text-xs text-muted-foreground">
        {m.events_scope_module()}
        <select class={fieldBox} bind:value={filterModule}>
          <option value="">{m.events_scope_all()}</option>
          {#each modules as mod (mod.id)}<option value={mod.id}>{mod.name} ({mod.key})</option
            >{/each}
        </select>
      </label>
      {#if filterModule !== ''}
        <label class="flex flex-col gap-1 text-xs text-muted-foreground">
          {m.events_scope_item()}
          <select class={fieldBox} bind:value={filterItem}>
            <option value="">{m.events_scope_module_all()}</option>
            {#each moduleItems as it (it.id)}<option value={it.id}>{it.title}</option>{/each}
          </select>
        </label>
      {/if}
    </div>
  {/if}
  <p class="text-sm text-muted-foreground">{m.events_calendar_hint({ zone })}</p>
  {#if error !== undefined}
    <p role="alert" class="text-sm text-destructive">{error}</p>
  {/if}

  <table
    class="hidden w-full table-fixed border-collapse text-sm md:table"
    data-testid="calendar-grid"
  >
    <caption class="sr-only">{m.events_calendar_caption({ title })}</caption>
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
                <span class="sr-only">{day === today ? m.events_day_today({ day }) : day}</span>
              </div>
              {#if canCreate}
                <button
                  type="button"
                  class="float-right -mt-5 rounded px-1 text-xs text-muted-foreground hover:bg-accent hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
                  aria-label={m.events_task_create_on({ day })}
                  title={m.events_task_create_on({ day })}
                  data-create-day={day}
                  onclick={() => (createDay = day)}>+</button
                >
              {/if}
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
        <h2 class="text-sm font-semibold">{day === today ? m.events_day_today({ day }) : day}</h2>
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
      <li class="text-sm text-muted-foreground">{m.events_month_empty()}</li>
    {/each}
  </ol>
</section>

<CalendarTaskDialog
  workspaceId={workspace.id}
  day={createDay}
  item={scope?.itemId === undefined || item === undefined
    ? undefined
    : { id: item.id, title: item.title }}
  moduleId={scope?.itemId === undefined ? scope?.moduleId : undefined}
  onclose={() => {
    const day = createDay;
    createDay = undefined;
    if (day !== undefined)
      void tick().then(() =>
        document.querySelector<HTMLElement>(`[data-create-day="${day}"]`)?.focus(),
      );
  }}
  oncreated={(task: TaskDetailView) => {
    createDay = undefined;
    notifyDone(
      m.events_task_created({
        title: task.title,
        day: task.due_date === null ? '' : formatDay(task.due_date),
      }),
    );
    reloadKey += 1;
    onchanged?.();
  }}
/>

<EventDialog
  eventId={openEvent}
  occurrence={openOccurrence}
  onclose={() => (openEvent = undefined)}
  onchanged={() => {
    changed = true;
    reloadKey += 1;
    onchanged?.();
  }}
  returnFocus={() =>
    returnTo === undefined
      ? null
      : document.querySelector<HTMLElement>(`[data-calendar-entry="${CSS.escape(returnTo)}"]`)}
/>
