<script lang="ts">
  // 홈 (D-19): 오늘 날짜, 빠른 추가(메모·업무·일정), 실행 중인 스톱워치, 이번 주 일정, Up Next, 고정·즐겨찾기,
  // 진행 중인 항목(업무 완료 비율), 최근 문서·작업, 알림. 모든 워크스페이스에서 볼 수 있는 것만 보인다.
  import type { Workspace } from '@fvoci/api-client';

  import type {
    CalendarView,
    HomeTaskView,
    HomeView,
    MarkedTargetView,
    NotificationView,
  } from '$lib/api/client';
  import { api, ApiError, newRequestId } from '$lib/api/client';
  import type { CalendarEntry } from '$lib/features/events/calendar';
  import { entriesByDay } from '$lib/features/events/calendar';
  import { localTimeZone } from '$lib/features/events/time';
  import { dayNumber, fromDayNumber } from '$lib/features/tasks/gantt';
  import { clockText } from '$lib/features/time/format';
  import { timer } from '$lib/features/time/timer.svelte';
  import { itemPath, itemTaskPath, pathFor, router } from '$lib/router.svelte';
  import { Button } from '$lib/ui/button';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';
  import * as Select from '$lib/ui/select';

  interface Props {
    workspaces: Workspace[];
  }

  const { workspaces }: Props = $props();

  const zone = localTimeZone();
  let home = $state<HomeView | undefined>(undefined);
  let week = $state<Map<string, CalendarEntry[]>>(new Map());
  let notifications = $state<NotificationView[]>([]);
  let error = $state<string | undefined>(undefined);
  let reloadKey = $state(0);

  const weekDays = $derived.by(() => {
    if (home === undefined) return [];
    const n = dayNumber(home.today);
    const monday = n - ((n + 3) % 7);
    return Array.from({ length: 7 }, (_, i) => fromDayNumber(monday + i));
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
        const n = dayNumber(h.today);
        const from = fromDayNumber(n - ((n + 3) % 7));
        const to = fromDayNumber(dayNumber(from) + 6);
        const calendars: CalendarView[] = await Promise.all(
          list.map((w) => api.calendar(w.id, from, to, zone)),
        );
        const merged = entriesByDay(
          calendars.flatMap((c) => c.tasks),
          calendars.flatMap((c) => c.projected_tasks),
          calendars.flatMap((c) => c.events),
          calendars.flatMap((c) => c.occurrences),
          from,
          to,
          zone,
        );
        const notes = await api.notifications();
        if (mine !== loadSeq) return;
        home = h;
        week = merged;
        notifications = notes.filter((x) => !x.read).slice(0, 5);
        error = undefined;
      } catch (e: unknown) {
        if (mine === loadSeq)
          error = e instanceof ApiError ? e.message : '홈을 불러오지 못했습니다.';
      }
    })();
  });

  const BUCKET_LABEL = { overdue: '기한 지남', today: '오늘', upcoming: '다가오는 일' } as const;
  const bucketOf = (b: 'overdue' | 'today' | 'upcoming'): HomeTaskView[] =>
    home?.up_next.filter((t) => t.bucket === b) ?? [];
  const memberOf = (ws: string): boolean => workspaces.some((w) => w.id === ws);
  const workspaceName = (ws: string | null): string =>
    ws === null ? '개인' : (workspaces.find((w) => w.id === ws)?.name ?? '');

  function openTask(t: { workspace_id: string; item_id: string; id: string }): void {
    router.navigate(itemTaskPath(t.workspace_id, t.item_id, t.id));
  }

  function openMarked(m: MarkedTargetView): void {
    if (m.kind === 'item' && m.workspace_id !== null)
      router.navigate(itemPath(m.workspace_id, m.id));
    else if (m.kind === 'task' && m.workspace_id !== null && m.item_id !== null)
      router.navigate(itemTaskPath(m.workspace_id, m.item_id, m.id));
    else if (m.kind === 'document')
      router.navigate(
        pathFor({ id: m.id, workspace_id: m.workspace_id, item_id: m.item_id }, memberOf),
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
  let quickRequest = newRequestId();
  let quickStatus = $state('');
  let quickError = $state<string | undefined>(undefined);
  const writableItems = $derived(home?.active_items ?? []);

  async function addQuick(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    quickError = undefined;
    const title = quick.title.trim();
    if (title === '') return;
    try {
      if (quick.what === 'memo') {
        const note = await api.createNote(quickRequest, title);
        quickStatus = `메모를 만들었습니다: “${note.title}”`;
      } else if (quick.item === '') {
        quickError = '어느 항목에 넣을지 고르세요.';
        return;
      } else if (quick.what === 'task') {
        await api.createTask(quick.item, quickRequest, title);
        quickStatus = `업무를 만들었습니다: “${title}”`;
      } else {
        const date = quick.date === '' ? (home?.today ?? '') : quick.date;
        await api.createEvent(quick.item, quickRequest, {
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
        quickStatus = `일정을 만들었습니다: “${title}”(${date})`;
      }
      quickRequest = newRequestId();
      quick.title = '';
      reloadKey += 1;
    } catch (e: unknown) {
      quickError = e instanceof ApiError ? e.message : '만들지 못했습니다.';
    }
  }

  const WHAT_LABEL = { memo: '메모', task: '업무', event: '일정(종일)' } as const;
  const WEEKDAY = ['월', '화', '수', '목', '금', '토', '일'];
</script>

<section class="flex flex-col gap-6" aria-labelledby="dashboard-heading">
  <div class="flex flex-wrap items-baseline gap-2">
    <h1 id="dashboard-heading" class="text-xl font-semibold">오늘</h1>
    {#if home !== undefined}<span class="text-muted-foreground">{home.today}</span>{/if}
  </div>
  {#if error !== undefined}
    <p role="alert" class="text-sm text-destructive">{error}</p>
  {/if}

  <form
    class="flex flex-wrap items-end gap-2"
    onsubmit={(e) => void addQuick(e)}
    aria-label="빠른 추가"
  >
    <div class="flex flex-col gap-1">
      <span class="text-sm font-medium" id="quick-what-label">빠른 추가</span>
      <Select.Root
        type="single"
        value={quick.what}
        onValueChange={(v: string) => {
          if (v === 'memo' || v === 'task' || v === 'event') quick.what = v;
        }}
      >
        <Select.Trigger aria-labelledby="quick-what-label">{WHAT_LABEL[quick.what]}</Select.Trigger>
        <Select.Content>
          {#each ['memo', 'task', 'event'] as const as w (w)}
            <Select.Item value={w} label={WHAT_LABEL[w]}>{WHAT_LABEL[w]}</Select.Item>
          {/each}
        </Select.Content>
      </Select.Root>
    </div>
    {#if quick.what !== 'memo'}
      <div class="flex flex-col gap-1">
        <span class="text-sm font-medium" id="quick-item-label">항목</span>
        <Select.Root
          type="single"
          value={quick.item}
          onValueChange={(v: string) => (quick.item = v)}
        >
          <Select.Trigger aria-labelledby="quick-item-label"
            >{writableItems.find((i) => i.id === quick.item)?.title ??
              '항목 고르기'}</Select.Trigger
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
        <Label for="quick-date">날짜</Label>
        <Input id="quick-date" type="date" bind:value={quick.date} />
      </div>
    {/if}
    <div class="flex min-w-48 flex-1 flex-col gap-1">
      <Label for="quick-title">제목</Label>
      <Input id="quick-title" bind:value={quick.title} maxlength={200} />
    </div>
    <Button type="submit">추가</Button>
  </form>
  {#if quickError !== undefined}<p role="alert" class="text-sm text-destructive">
      {quickError}
    </p>{/if}
  <p class="sr-only" aria-live="polite">{quickStatus}</p>

  {#if timer.entry !== null}
    <p class="rounded-md border p-2 text-sm">
      실행 중인 스톱워치: <span class="font-mono">{clockText(timer.seconds)}</span>
      {timer.entry.target?.label ?? ''}{timer.entry.paused_at === null ? '' : ' (일시정지)'} ·
      <a
        class="underline"
        href="/time"
        onclick={(e: MouseEvent) => {
          e.preventDefault();
          router.navigate('/time');
        }}>내 시간</a
      >
    </p>
  {/if}

  <div class="grid gap-6 md:grid-cols-2">
    <section class="flex flex-col gap-2" aria-labelledby="up-next-heading">
      <h2 id="up-next-heading" class="text-lg font-semibold">Up Next</h2>
      {#each ['overdue', 'today', 'upcoming'] as const as b (b)}
        {@const list = bucketOf(b)}
        {#if list.length > 0}
          <h3 class={['text-sm font-semibold', b === 'overdue' && 'text-destructive']}>
            {BUCKET_LABEL[b]}
            {list.length}
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
                  <span class="text-xs text-muted-foreground">{t.due_date}</span>
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      {/each}
      {#if home?.up_next.length === 0}
        <p class="text-sm text-muted-foreground">이번 주까지 마감인 내 업무가 없습니다.</p>
      {/if}
    </section>

    <section class="flex flex-col gap-2" aria-labelledby="week-heading">
      <h2 id="week-heading" class="text-lg font-semibold">이번 주</h2>
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

    {#each [{ id: 'pins', label: '고정', list: home?.pins ?? [] }, { id: 'favorites', label: '즐겨찾기', list: home?.favorites ?? [] }] as group (group.id)}
      <section class="flex flex-col gap-2" aria-labelledby={`${group.id}-heading`}>
        <h2 id={`${group.id}-heading`} class="text-lg font-semibold">{group.label}</h2>
        {#each group.list as m (`${m.kind}:${m.id}`)}
          <button
            type="button"
            class="rounded px-2 py-1 text-left text-sm hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
            onclick={() => {
              openMarked(m);
            }}>{m.label}</button
          >
        {:else}
          <p class="text-sm text-muted-foreground">
            항목·문서·업무 화면의 “{group.label === '고정' ? '홈에 고정' : '즐겨찾기'}”로 더할 수
            있습니다.
          </p>
        {/each}
      </section>
    {/each}

    <section class="flex flex-col gap-2" aria-labelledby="active-heading">
      <h2 id="active-heading" class="text-lg font-semibold">진행 중인 항목</h2>
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
              업무 {i.tasks_done}/{i.tasks_total} 완료
            </span>
          {/if}
        </button>
      {:else}
        <p class="text-sm text-muted-foreground">진행 중인 항목이 없습니다.</p>
      {/each}
    </section>

    <section class="flex flex-col gap-2" aria-labelledby="recent-heading">
      <h2 id="recent-heading" class="text-lg font-semibold">최근 작업</h2>
      {#each home?.recent_documents ?? [] as d (d.id)}
        <button
          type="button"
          class="rounded px-2 py-1 text-left text-sm hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
          onclick={() => {
            router.navigate(pathFor(d, memberOf));
          }}>📄 {d.reference ?? ''} {d.title}</button
        >
      {/each}
      {#each home?.recent_tasks ?? [] as t (t.id)}
        <button
          type="button"
          class="rounded px-2 py-1 text-left text-sm hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
          onclick={() => {
            openTask(t);
          }}>☑ {t.reference ?? ''} {t.title}</button
        >
      {/each}
      {#if home?.recent_documents.length === 0 && home.recent_tasks.length === 0}
        <p class="text-sm text-muted-foreground">최근에 고친 문서·업무가 없습니다.</p>
      {/if}
    </section>

    <section class="flex flex-col gap-2" aria-labelledby="notes-heading">
      <h2 id="notes-heading" class="text-lg font-semibold">알림</h2>
      {#each notifications as n (n.id)}
        <p class="text-sm">
          {n.actor_name ?? '누군가'}님이 {n.document_reference ?? ''} “{n.document_title}”에서 나를
          언급했습니다.
        </p>
      {:else}
        <p class="text-sm text-muted-foreground">읽지 않은 알림이 없습니다.</p>
      {/each}
      <a
        class="text-sm underline"
        href="/notifications"
        onclick={(e: MouseEvent) => {
          e.preventDefault();
          router.navigate('/notifications');
        }}>알림 모두 보기</a
      >
    </section>
  </div>
</section>
