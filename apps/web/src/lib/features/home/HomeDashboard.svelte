<script lang="ts">
  // 홈 (D-19): 오늘 날짜, 빠른 추가(메모·업무·일정), 실행 중인 스톱워치, 이번 주 일정, Up Next, 고정·즐겨찾기,
  // 진행 중인 항목(업무 완료 비율), 최근 문서·작업, 알림. 모든 워크스페이스에서 볼 수 있는 것만 보인다.
  import FileText from '@lucide/svelte/icons/file-text';
  import SquareCheck from '@lucide/svelte/icons/square-check';
  import type { Workspace } from '@fvoci/api-client';

  import type {
    CalendarView,
    HomeTaskView,
    HomeView,
    MarkedTargetView,
    NotificationView,
  } from '$lib/api/client';
  import { api } from '$lib/api/client';
  import { RetryKey } from '$lib/api/retry-key';
  import { errorText } from '$lib/api/errors';
  import { me } from '$lib/me.svelte';
  import type { CalendarEntry } from '$lib/features/events/calendar';
  import { entriesByDay, startOfWeek } from '$lib/features/events/calendar';
  import { displaySettings, formatDay, weekdays } from '$lib/display';
  import { notificationText } from '$lib/features/references/notification-text';
  import { dayNumber, fromDayNumber } from '$lib/features/tasks/gantt';
  import { clockText } from '$lib/features/time/format';
  import { timer } from '$lib/features/time/timer.svelte';
  import { m } from '$lib/paraglide/messages.js';
  import { itemPath, itemTaskPath, pathFor, router } from '$lib/router.svelte';
  import { Button } from '$lib/ui/button';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';
  import * as Select from '$lib/ui/select';

  import { quickSend } from './quick';

  interface Props {
    workspaces: Workspace[];
  }

  const { workspaces }: Props = $props();

  // 오늘·주 시작은 사용자 표시 설정(D-81).
  const settings = displaySettings();
  const zone = settings.timeZone;
  let home = $state<HomeView | undefined>(undefined);
  let week = $state<Map<string, CalendarEntry[]>>(new Map());
  let notifications = $state<NotificationView[]>([]);
  let error = $state<string | undefined>(undefined);
  let reloadKey = $state(0);

  const weekDays = $derived.by(() => {
    if (home === undefined) return [];
    const first = dayNumber(startOfWeek(home.today, settings.weekStart));
    return Array.from({ length: 7 }, (_, i) => fromDayNumber(first + i));
  });

  // 빠른 추가 뒤(reloadKey)나 워크스페이스가 바뀌면 다시 불러온다. 늦게 온 이전 응답은 버린다.
  let loadSeq = 0;
  const request = $derived({ list: workspaces, key: reloadKey });
  $effect(() => {
    const { list } = request;
    const mine = ++loadSeq;
    void (async () => {
      try {
        const h = await api.home(zone);
        const from = startOfWeek(h.today, settings.weekStart);
        const to = fromDayNumber(dayNumber(from) + 6);
        // 워크스페이스 하나의 달력이 실패해도(그사이 나간 경우 등) 나머지는 보인다.
        const settled = await Promise.allSettled(
          list.map((w) => api.calendar(w.id, from, to, zone)),
        );
        const calendars: CalendarView[] = settled.flatMap((s) =>
          s.status === 'fulfilled' ? [s.value] : [],
        );
        const merged = entriesByDay(
          calendars.flatMap((c) => c.tasks),
          calendars.flatMap((c) => c.projected_tasks),
          calendars.flatMap((c) => c.events),
          calendars.flatMap((c) => c.occurrences),
          from,
          to,
          zone,
          // 업무·일정 밖 날짜(D-80)도 달력과 같이 보인다.
          calendars.flatMap((c) => c.dates),
        );
        const notes = await api.notifications();
        if (mine !== loadSeq) return;
        home = h;
        week = merged;
        notifications = notes.filter((x) => !x.read).slice(0, 5);
        error = undefined;
      } catch (e: unknown) {
        if (mine === loadSeq) error = errorText(e, m.home_load_failed());
      }
    })();
  });

  function bucketLabel(b: 'overdue' | 'today' | 'upcoming'): string {
    if (b === 'overdue') return m.home_bucket_overdue();
    return b === 'today' ? m.home_bucket_today() : m.home_bucket_upcoming();
  }
  const bucketOf = (b: 'overdue' | 'today' | 'upcoming'): HomeTaskView[] =>
    home?.up_next.filter((t) => t.bucket === b) ?? [];
  // 묶음의 전체 수는 서버가 센다(목록은 묶음마다 50개까지).
  const totalOf = (b: 'overdue' | 'today' | 'upcoming'): number => home?.up_next_counts[b] ?? 0;
  const memberOf = (ws: string): boolean => workspaces.some((w) => w.id === ws);
  const workspaceName = (ws: string | null): string =>
    ws === null ? m.home_personal() : (workspaces.find((w) => w.id === ws)?.name ?? '');

  function openTask(t: { workspace_id: string; item_id: string; id: string }): void {
    router.navigate(itemTaskPath(t.workspace_id, t.item_id, t.id));
  }

  function openMarked(mark: MarkedTargetView): void {
    if (mark.kind === 'item' && mark.workspace_id !== null)
      router.navigate(itemPath(mark.workspace_id, mark.id));
    else if (mark.kind === 'task' && mark.workspace_id !== null && mark.item_id !== null)
      router.navigate(itemTaskPath(mark.workspace_id, mark.item_id, mark.id));
    else if (mark.kind === 'document')
      router.navigate(
        pathFor({ id: mark.id, workspace_id: mark.workspace_id, item_id: mark.item_id }, memberOf),
      );
  }

  // 빠른 추가
  const quick = $state<{
    what: 'memo' | 'task' | 'event';
    title: string;
    item: string;
    date: string;
  }>({
    what: 'memo',
    title: '',
    item: '',
    date: '',
  });
  // 같은 입력의 재시도는 같은 요청 id, 종류·대상·제목을 고치면 새 id.
  const quickKey = new RetryKey();
  let quickStatus = $state('');
  let quickError = $state<string | undefined>(undefined);
  const writableItems = $derived((home?.active_items ?? []).filter((i) => i.can_write));

  async function addQuick(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    quickError = undefined;
    const title = quick.title.trim();
    if (title === '') return;
    // 재시도 키는 실제로 보내는 값으로 만든다.
    const send = quickSend(quick, home?.today ?? '');
    if (send === undefined) {
      quickError = m.home_pick_item_required();
      return;
    }
    const requestId = quickKey.for(send);
    try {
      if (send.what === 'memo') {
        // 이 창을 연 사용자. 다른 탭에서 계정을 바꿨으면 서버가 409 account_changed로 막는다(#60).
        const note = await api.createNote(me.userId, requestId, title);
        quickStatus = m.home_memo_created({ title: note.title });
      } else if (send.what === 'task') {
        await api.createTask(send.item, requestId, title);
        quickStatus = m.home_task_created({ title });
      } else {
        const date = send.date;
        await api.createEvent(send.item, requestId, {
          kind: 'event',
          title,
          description: '',
          all_day: true,
          start_date: date,
          end_date: date,
          starts_at: null,
          ends_at: null,
          timezone: zone,
        });
        quickStatus = m.home_event_created({ title, date: date === '' ? '' : formatDay(date) });
      }
      quickKey.done(requestId);
      quick.title = '';
      reloadKey += 1;
    } catch (e: unknown) {
      quickError = errorText(e, m.home_create_failed());
    }
  }

  function whatLabel(what: 'memo' | 'task' | 'event'): string {
    if (what === 'memo') return m.home_what_memo();
    return what === 'task' ? m.home_what_task() : m.home_what_event();
  }
  const WEEKDAY = weekdays(settings).map((w) => w.short);
</script>

<section class="flex flex-col gap-6" aria-labelledby="dashboard-heading">
  <div class="flex flex-wrap items-baseline gap-2">
    <h1 id="dashboard-heading" class="text-xl font-semibold">{m.home_today_heading()}</h1>
    {#if home !== undefined}<span class="text-muted-foreground"
        >{formatDay(home.today, { dateStyle: 'full' })}</span
      >{/if}
  </div>
  {#if error !== undefined}
    <p role="alert" class="text-sm text-destructive">{error}</p>
  {/if}

  <form
    class="flex flex-wrap items-end gap-2"
    onsubmit={(e) => void addQuick(e)}
    aria-label={m.home_quick_add()}
  >
    <div class="flex flex-col gap-1">
      <span class="text-sm font-medium" id="quick-what-label">{m.home_quick_add()}</span>
      <Select.Root
        type="single"
        value={quick.what}
        onValueChange={(v: string) => {
          if (v === 'memo' || v === 'task' || v === 'event') quick.what = v;
        }}
      >
        <Select.Trigger aria-labelledby="quick-what-label">{whatLabel(quick.what)}</Select.Trigger>
        <Select.Content>
          {#each ['memo', 'task', 'event'] as const as w (w)}
            <Select.Item value={w} label={whatLabel(w)}>{whatLabel(w)}</Select.Item>
          {/each}
        </Select.Content>
      </Select.Root>
    </div>
    {#if quick.what !== 'memo'}
      <div class="flex flex-col gap-1">
        <span class="text-sm font-medium" id="quick-item-label">{m.home_item()}</span>
        <Select.Root
          type="single"
          value={quick.item}
          onValueChange={(v: string) => (quick.item = v)}
        >
          <Select.Trigger aria-labelledby="quick-item-label"
            >{writableItems.find((i) => i.id === quick.item)?.title ??
              m.home_pick_item()}</Select.Trigger
          >
          <Select.Content>
            {#each writableItems as i (i.id)}
              <Select.Item value={i.id} label={i.title}
                >{i.reference ?? ''} {i.title} · {workspaceName(i.workspace_id)}</Select.Item
              >
            {/each}
          </Select.Content>
        </Select.Root>
      </div>
    {/if}
    {#if quick.what === 'event'}
      <div class="flex flex-col gap-1">
        <Label for="quick-date">{m.home_date()}</Label>
        <Input id="quick-date" type="date" bind:value={quick.date} />
      </div>
    {/if}
    <div class="flex min-w-48 flex-1 flex-col gap-1">
      <Label for="quick-title">{m.home_title()}</Label>
      <Input id="quick-title" bind:value={quick.title} maxlength={200} />
    </div>
    <Button type="submit">{m.home_add()}</Button>
  </form>
  {#if quickError !== undefined}<p role="alert" class="text-sm text-destructive">
      {quickError}
    </p>{/if}
  <p class="sr-only" aria-live="polite">{quickStatus}</p>

  {#if timer.entry !== null}
    <p class="rounded-md border p-2 text-sm">
      {m.home_timer_running()} <span class="font-mono">{clockText(timer.seconds)}</span>
      {timer.entry.target?.label ?? ''}{timer.entry.paused_at === null
        ? ''
        : ` ${m.home_timer_paused()}`} ·
      <a
        class="underline"
        href="/time"
        onclick={(e: MouseEvent) => {
          e.preventDefault();
          router.navigate('/time');
        }}>{m.app_nav_time()}</a
      >
    </p>
  {/if}

  <div class="grid gap-6 md:grid-cols-2">
    <section class="flex flex-col gap-2" aria-labelledby="up-next-heading">
      <h2 id="up-next-heading" class="text-lg font-semibold">{m.home_up_next()}</h2>
      {#each ['overdue', 'today', 'upcoming'] as const as b (b)}
        {@const list = bucketOf(b)}
        {@const total = totalOf(b)}
        {#if list.length > 0}
          <h3 class={['text-sm font-semibold', b === 'overdue' && 'text-destructive']}>
            {bucketLabel(b)}
            {total}
          </h3>
          <ul class="flex flex-col gap-0.5">
            {#each list as t (t.id)}
              <li>
                <button
                  type="button"
                  class="flex w-full items-baseline gap-2 rounded px-2 py-1 text-left text-sm hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
                  onclick={() => {
                    openTask(t);
                  }}
                >
                  <span class="font-mono text-xs text-muted-foreground">{t.reference ?? ''}</span>
                  <span class="min-w-0 flex-1 truncate">{t.title}</span>
                  <span class="text-xs text-muted-foreground"
                    >{t.due_date === null ? '' : formatDay(t.due_date)}</span
                  >
                </button>
              </li>
            {/each}
          </ul>
          {#if total > list.length}
            <p class="px-2 text-xs text-muted-foreground">
              {m.home_bucket_more({ shown: list.length, total })}
            </p>
          {/if}
        {/if}
      {/each}
      {#if home?.up_next.length === 0}
        <p class="text-sm text-muted-foreground">{m.home_up_next_empty()}</p>
      {/if}
    </section>

    <section class="flex flex-col gap-2" aria-labelledby="week-heading">
      <h2 id="week-heading" class="text-lg font-semibold">{m.home_week()}</h2>
      <ol class="flex flex-col gap-1">
        {#each weekDays as d, i (d)}
          {@const entries = week.get(d) ?? []}
          <li class={['flex gap-2 text-sm', d === home?.today && 'font-semibold']}>
            <span class="w-14 shrink-0 text-muted-foreground"
              >{WEEKDAY[i]} {Number(d.slice(8))}</span
            >
            <span class="min-w-0 flex-1">{entries.map((e) => e.label).join(' · ') || '—'}</span>
          </li>
        {/each}
      </ol>
    </section>

    {#each [{ id: 'pins', label: m.home_pins(), empty: m.home_pins_empty(), list: home?.pins ?? [] }, { id: 'favorites', label: m.home_favorites(), empty: m.home_favorites_empty(), list: home?.favorites ?? [] }] as group (group.id)}
      <section class="flex flex-col gap-2" aria-labelledby={`${group.id}-heading`}>
        <h2 id={`${group.id}-heading`} class="text-lg font-semibold">{group.label}</h2>
        {#each group.list as mark (`${mark.kind}:${mark.id}`)}
          <button
            type="button"
            class="rounded px-2 py-1 text-left text-sm hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
            onclick={() => {
              openMarked(mark);
            }}>{mark.label}</button
          >
        {:else}
          <p class="text-sm text-muted-foreground">
            {group.empty}
          </p>
        {/each}
      </section>
    {/each}

    <section class="flex flex-col gap-2" aria-labelledby="active-heading">
      <h2 id="active-heading" class="text-lg font-semibold">{m.home_active()}</h2>
      {#each home?.active_items ?? [] as i (i.id)}
        <button
          type="button"
          class="flex flex-col gap-1 rounded px-2 py-1 text-left text-sm hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
          onclick={() => {
            router.navigate(itemPath(i.workspace_id, i.id));
          }}
        >
          <span
            >{i.reference ?? ''}
            {i.title}
            <span class="text-muted-foreground">· {workspaceName(i.workspace_id)}</span></span
          >
          {#if i.tasks_total > 0}
            <span class="flex items-center gap-2 text-xs text-muted-foreground">
              <span class="h-1.5 w-32 overflow-hidden rounded bg-muted" aria-hidden="true">
                <span
                  class="block h-full bg-primary"
                  style:width={`${String(Math.round((i.tasks_done / i.tasks_total) * 100))}%`}
                ></span>
              </span>
              {m.home_tasks_done({ done: i.tasks_done, total: i.tasks_total })}
            </span>
          {/if}
        </button>
      {:else}
        <p class="text-sm text-muted-foreground">{m.home_active_empty()}</p>
      {/each}
    </section>

    <section class="flex flex-col gap-2" aria-labelledby="recent-heading">
      <h2 id="recent-heading" class="text-lg font-semibold">{m.home_recent()}</h2>
      {#each home?.recent_documents ?? [] as d (d.id)}
        <button
          type="button"
          class="rounded px-2 py-1 text-left text-sm hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
          onclick={() => {
            router.navigate(pathFor(d, memberOf));
          }}
          ><span class="flex min-w-0 items-center gap-2"
            ><FileText class="size-4 shrink-0 text-muted-foreground" aria-hidden="true" /><span
              class="shrink-0 font-mono text-xs text-muted-foreground">{d.reference ?? ''}</span
            ><span class="truncate">{d.title}</span></span
          ></button
        >
      {/each}
      {#each home?.recent_tasks ?? [] as t (t.id)}
        <button
          type="button"
          class="rounded px-2 py-1 text-left text-sm hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
          onclick={() => {
            openTask(t);
          }}
          ><span class="flex min-w-0 items-center gap-2"
            ><SquareCheck class="size-4 shrink-0 text-muted-foreground" aria-hidden="true" /><span
              class="shrink-0 font-mono text-xs text-muted-foreground">{t.reference ?? ''}</span
            ><span class="truncate">{t.title}</span></span
          ></button
        >
      {/each}
      {#if home?.recent_documents.length === 0 && home.recent_tasks.length === 0}
        <p class="text-sm text-muted-foreground">{m.home_recent_empty()}</p>
      {/if}
    </section>

    <section class="flex flex-col gap-2" aria-labelledby="notes-heading">
      <h2 id="notes-heading" class="text-lg font-semibold">{m.home_notifications()}</h2>
      {#each notifications as n (n.id)}
        <p class="text-sm">{notificationText(n)}</p>
      {:else}
        <p class="text-sm text-muted-foreground">{m.home_notifications_empty()}</p>
      {/each}
      <a
        class="text-sm underline"
        href="/notifications"
        onclick={(e: MouseEvent) => {
          e.preventDefault();
          router.navigate('/notifications');
        }}>{m.home_all_notifications()}</a
      >
    </section>
  </div>
</section>
