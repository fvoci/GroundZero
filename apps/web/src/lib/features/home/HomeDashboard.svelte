<script lang="ts">
  // 홈(D-19·D-120): 현재 조직 요약. 시간 기록 표시는 앱 상단에서 맡는다.
  import FileText from '@lucide/svelte/icons/file-text';
  import SquareCheck from '@lucide/svelte/icons/square-check';
  import type { Workspace } from '@fvoci/api-client';

  import type { CalendarView, HomeView, MarkedTargetView, NotificationView } from '$lib/api/client';
  import { api } from '$lib/api/client';
  import { RetryKey } from '$lib/api/retry-key';
  import { errorText } from '$lib/api/errors';
  import { me } from '$lib/me.svelte';
  import type { CalendarEntry } from '$lib/features/events/calendar';
  import { entriesByDay, startOfWeek } from '$lib/features/events/calendar';
  import { displaySettings, formatDay, weekdays } from '$lib/display';
  import { org } from '$lib/org.svelte';
  import { notificationText } from '$lib/features/references/notification-text';
  import { dayNumber, fromDayNumber } from '$lib/features/tasks/gantt';
  import ActivityList from '$lib/features/activity/ActivityList.svelte';
  import { notifyDone } from '$lib/features/feedback/feedback.svelte';
  import { readLocal, writeLocal } from '$lib/storage';
  import { m } from '$lib/paraglide/messages.js';
  import { itemPath, itemTaskPath, notificationPath, pathFor, router } from '$lib/router.svelte';
  import { Button } from '$lib/ui/button';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';
  import { NativeSelect } from '$lib/ui/native-select';
  import * as Collapsible from '$lib/ui/collapsible';

  import MyTasksScreen from './MyTasksScreen.svelte';
  import { invitationChanges } from './invitations.svelte';

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
  /** "다시 시도"로 다시 받는 중(앞서 받은 홈이 남아 있어도 진행 중이라고 보인다). */
  let retrying = $state(false);

  const weekDays = $derived.by(() => {
    if (home === undefined) return [];
    const first = dayNumber(startOfWeek(home.today, settings.weekStart));
    return Array.from({ length: 7 }, (_, i) => fromDayNumber(first + i));
  });

  // 빠른 추가 뒤(reloadKey)나 워크스페이스가 바뀌면 다시 불러온다. 늦게 온 이전 응답과 화면이 사라진 뒤의 응답은 버린다.
  let loadSeq = 0;
  const request = $derived({
    list: workspaces,
    key: reloadKey,
    organizationId: org.currentId,
    invitations: invitationChanges.version,
  });
  $effect(() => {
    const { list, organizationId } = request;
    const mine = ++loadSeq;
    home = undefined;
    notifications = [];
    week = new Map();
    error = undefined;
    if (organizationId === undefined) {
      // 홈은 현재 조직의 요약이다(D-19·D-120). 조직을 고르기 전에는 묻지 않는다.
      home = undefined;
      error = m.app_organization_required();
      retrying = false;
      return;
    }
    void (async () => {
      try {
        const h = await api.home(organizationId, zone);
        if (mine !== loadSeq) return;
        const from = startOfWeek(h.today, settings.weekStart);
        const to = fromDayNumber(dayNumber(from) + 6);
        // 워크스페이스 하나의 달력이 실패해도(그사이 나간 경우 등) 나머지는 보인다.
        const settled = await Promise.allSettled(
          list
            .filter((w) => w.organization_id === organizationId)
            .map((w) => api.calendar(w.id, from, to, zone)),
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
        if (mine !== loadSeq) return;
        const notes = await api.notifications(organizationId, undefined, 20);
        if (mine !== loadSeq) return;
        home = h;
        week = merged;
        notifications = notes.notifications.filter((x) => !x.read).slice(0, 5);
        error = undefined;
        retrying = false;
      } catch (e: unknown) {
        if (mine !== loadSeq) return;
        error = errorText(e, m.home_load_failed());
        retrying = false;
      }
    })();
    // 화면이 사라지거나 다시 불러오면 이 요청의 응답은 버린다.
    return () => {
      loadSeq += 1;
    };
  });

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
    title: string;
    item: string;
  }>({
    title: '',
    item: '',
  });
  // 같은 입력의 재시도는 같은 요청 id, 종류·대상·제목을 고치면 새 id.
  const quickKey = new RetryKey();
  let quickOpen = $state(false);
  let adding = $state(false);
  let quickUser = $state('');
  let quickError = $state<string | undefined>(undefined);
  const writableItems = $derived((home?.active_items ?? []).filter((i) => i.can_write));

  $effect(() => {
    quickUser = me.userId;
    const key = org.key('quick-add-item');
    quick.item = readLocal(key) ?? '';
    quick.title = '';
    quickError = undefined;
    quickOpen = false;
  });

  async function addQuick(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (adding || org.currentId === undefined) return;
    const organizationId = org.currentId;
    const storageKey = org.key('quick-add-item');
    quickError = undefined;
    const title = quick.title.trim();
    if (title === '') return;
    // 재시도 키는 실제로 보내는 값으로 만든다.
    const send = { item: quick.item, title };
    if (!writableItems.some((i) => i.id === send.item)) {
      quickError = m.home_pick_item_required();
      return;
    }
    const requestId = quickKey.for({ ...send, organizationId, user: quickUser });
    adding = true;
    try {
      await api.createQuickTask(quickUser, send.item, requestId, title, quickUser);
      quickKey.done(requestId);
      writeLocal(storageKey, send.item);
      if (org.currentId !== organizationId) return;
      notifyDone(m.home_task_created({ title }));
      quick.title = '';
      reloadKey += 1;
    } catch (e: unknown) {
      if (org.currentId === organizationId) quickError = errorText(e, m.home_create_failed());
    } finally {
      adding = false;
    }
  }

  const WEEKDAY = weekdays(settings).map((w) => w.short);
</script>

{#if org.currentId === undefined}
  <p role="status">{m.app_organization_required()}</p>
{:else}
  <section class="flex flex-col gap-6" aria-labelledby="dashboard-heading">
    <Collapsible.Root bind:open={quickOpen}>
      <div class="flex flex-wrap items-baseline gap-2">
        <h1 id="dashboard-heading" class="text-xl font-semibold">{m.home_today_heading()}</h1>
        {#if home !== undefined}<span class="text-muted-foreground"
            >{formatDay(home.today, { dateStyle: 'full' })}</span
          >{/if}
        <span class="text-sm text-muted-foreground">{org.current?.name}</span>
        <Collapsible.Trigger>
          {#snippet child({ props })}<Button {...props} variant="outline"
              >{m.home_quick_add()}</Button
            >{/snippet}
        </Collapsible.Trigger>
      </div>
      <Collapsible.Content>
        <form
          class="mt-3 flex flex-wrap items-end gap-2 rounded-md border bg-card p-2 @md:p-3"
          onsubmit={(e) => void addQuick(e)}
          aria-label={m.home_quick_add()}
        >
          <div class="flex min-w-0 flex-1 flex-col gap-2">
            <Label for="quick-title">{m.home_title()}</Label>
            <Input
              id="quick-title"
              bind:value={quick.title}
              maxlength={200}
              required
              disabled={adding}
            />
            <Label for="quick-item">{m.home_item()}</Label>
            <NativeSelect id="quick-item" bind:value={quick.item} disabled={adding}>
              <option value="">{m.home_pick_item()}</option>
              {#each writableItems as i (i.id)}<option value={i.id}
                  >{i.title} · {workspaceName(i.workspace_id)}</option
                >{/each}
            </NativeSelect>
            <p class="text-sm text-muted-foreground">{m.home_quick_assignee()}</p>
            {#if quickError !== undefined}<p role="alert" class="text-sm text-destructive">
                {quickError}
              </p>{/if}
          </div>
          <Button type="submit" disabled={adding || home === undefined}>{m.home_add()}</Button>
        </form>
      </Collapsible.Content>
    </Collapsible.Root>
    <p role="status" class="text-sm text-muted-foreground empty:hidden">
      {error === undefined && (home === undefined || retrying) ? m.app_loading() : ''}
    </p>
    {#if error !== undefined}
      <!-- 다시 받기: 빠른 추가 뒤와 같은 다시 읽기(reloadKey). 받는 동안은 오류 대신 불러오는 중이라고 보인다. -->
      <div class="flex flex-wrap items-center gap-2">
        <p role="alert" class="text-sm text-destructive">{error}</p>
        <Button
          variant="outline"
          size="sm"
          onclick={() => {
            error = undefined;
            retrying = true;
            reloadKey += 1;
          }}>{m.common_retry()}</Button
        >
      </div>
    {/if}

    <!-- 열 정의가 없으면 암묵 auto 열이 띄어쓰기 없는 긴 글(URL)만큼 늘어나 좁은 화면에서 가로로 넘친다. -->
    <div class="grid grid-cols-1 gap-6 @2xl:grid-cols-2">
      {#if home !== undefined}<div class="rounded-md border bg-card p-2 @md:p-3">
          <MyTasksScreen summary version={reloadKey} />
        </div>{/if}
      <section
        class="flex flex-col gap-2 rounded-md border bg-card p-2 @md:p-3"
        aria-labelledby="week-heading"
      >
        <h2 id="week-heading" class="text-lg font-semibold">{m.home_week()}</h2>
        <ol class="flex flex-col gap-1">
          {#each weekDays as d, i (d)}
            {@const entries = week.get(d) ?? []}
            <li class={['flex gap-2 text-sm', d === home?.today && 'font-semibold']}>
              <span class="w-14 shrink-0 text-muted-foreground"
                >{WEEKDAY[i]} {Number(d.slice(8))}</span
              >
              <span class="min-w-0 flex-1 [overflow-wrap:anywhere]"
                >{entries.map((e) => e.label).join(' · ') || '—'}</span
              >
            </li>
          {/each}
        </ol>
      </section>

      {#each [{ id: 'pins', label: m.home_pins(), empty: m.home_pins_empty(), list: home?.pins ?? [] }, { id: 'favorites', label: m.home_favorites(), empty: m.home_favorites_empty(), list: home?.favorites ?? [] }] as group (group.id)}
        <section
          class="flex flex-col gap-2 rounded-md border bg-card p-2 @md:p-3"
          aria-labelledby={`${group.id}-heading`}
        >
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
            <!-- 빈 칸 안내는 홈을 받은 뒤에만(불러오는 중·실패를 "없음"으로 읽지 않게). -->
            {#if home !== undefined}
              <p class="text-sm text-muted-foreground">{group.empty}</p>
            {/if}
          {/each}
        </section>
      {/each}

      <section
        class="flex flex-col gap-2 rounded-md border bg-card p-2 @md:p-3"
        aria-labelledby="active-heading"
      >
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
          {#if home !== undefined}
            <p class="text-sm text-muted-foreground">{m.home_active_empty()}</p>
          {/if}
        {/each}
      </section>

      <section
        class="flex flex-col gap-2 rounded-md border bg-card p-2 @md:p-3"
        aria-labelledby="recent-heading"
      >
        <h2 id="recent-heading" class="text-lg font-semibold">{m.home_recent()}</h2>
        {#each home?.recent_documents ?? [] as d (d.id)}
          <button
            type="button"
            class="rounded px-2 py-1 text-left text-sm hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
            title={d.title}
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
            title={t.title}
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

      <section
        class="flex flex-col gap-2 rounded-md border bg-card p-2 @md:p-3"
        aria-labelledby="notes-heading"
      >
        <h2 id="notes-heading" class="text-lg font-semibold">{m.home_notifications()}</h2>
        {#each notifications as n (n.id)}
          <a
            class="text-sm underline break-words"
            href={notificationPath(n.id)}
            onclick={(e) => {
              e.preventDefault();
              router.navigate(notificationPath(n.id));
            }}>{notificationText(n)}</a
          >
        {:else}
          {#if home !== undefined}
            <p class="text-sm text-muted-foreground">{m.home_notifications_empty()}</p>
          {/if}
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
    <section class="rounded-md border bg-card p-3" aria-label={m.home_activity_heading()}>
      <h2 class="mb-3 text-lg font-semibold">{m.home_activity_heading()}</h2>
      {#if home !== undefined}<ActivityList version={reloadKey} />{/if}
    </section>
  </section>
{/if}
