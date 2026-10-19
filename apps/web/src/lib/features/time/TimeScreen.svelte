<script lang="ts">
  // 내 시간 (D-32): 한 주의 기록과 날짜·종류별 합계, 이번 달 합계, 수동 기록, 합계 공유 선택.
  // 기록은 본인만 본다(워크스페이스 관리자에게도 보이지 않는다). 날짜는 보는 사람 시간대, 기록은 시작한 날에 센다.
  import { untrack } from 'svelte';

  import type { Workspace } from '@fvoci/api-client';

  import type { TimeEntryView, TimeKind, TimeSummaryRowView } from '$lib/api/client';
  import { api, TIME_KINDS } from '$lib/api/client';
  import { RetryKey } from '$lib/api/retry-key';
  import { errorText } from '$lib/api/errors';
  import { displaySettings, formatClock, weekdays } from '$lib/display';
  import { startOfWeek } from '$lib/features/events/calendar';
  import { dayIn, instantOf } from '$lib/features/events/time';
  import { dayNumber, fromDayNumber } from '$lib/features/tasks/gantt';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';
  import { Checkbox } from '$lib/ui/checkbox';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';
  import * as Select from '$lib/ui/select';

  import { durationText, TIME_KIND_LABEL } from './format';
  import TimeEntryDialog from './TimeEntryDialog.svelte';
  import { timer } from './timer.svelte';

  interface Props {
    workspaces: Workspace[];
  }

  const { workspaces }: Props = $props();

  // 오늘·주 시작·시각 표시는 사용자 표시 설정(D-81).
  const settings = displaySettings();
  const zone = settings.timeZone;
  const today = dayIn(new Date().toISOString(), zone);
  const thisWeek = startOfWeek(today, settings.weekStart);

  let weekStart = $state(thisWeek);
  const weekEnd = $derived(fromDayNumber(dayNumber(weekStart) + 6));
  const days = $derived(
    Array.from({ length: 7 }, (_, i) => fromDayNumber(dayNumber(weekStart) + i)),
  );
  let entries = $state<TimeEntryView[]>([]);
  let daily = $state<TimeSummaryRowView[]>([]);
  let monthly = $state<TimeSummaryRowView[]>([]);
  let sharing = $state<Record<string, boolean>>({});
  let error = $state<string | undefined>(undefined);
  let editing = $state<TimeEntryView | undefined>(undefined);
  let reloadKey = $state(0);

  // 머리글 타이머가 시작·멈춤·종료되면 목록·합계를 다시 불러온다.
  let seenTimer: string | null | undefined;
  $effect(() => {
    const key = timer.entry === null ? null : `${timer.entry.id}:${String(timer.entry.version)}`;
    if (seenTimer !== undefined && key !== seenTimer) untrack(() => (reloadKey += 1));
    seenTimer = key;
  });

  const request = $derived({ from: weekStart, to: weekEnd, key: reloadKey });
  $effect(() => {
    const { from, to } = request;
    const monthFrom = `${today.slice(0, 7)}-01`;
    let cancelled = false;
    Promise.all([
      api.timeEntries(from, to, zone),
      api.timeSummary(from, to, zone, 'day'),
      api.timeSummary(monthFrom, today, zone, 'month'),
    ])
      .then(([e, d, mo]) => {
        if (cancelled) return;
        entries = e;
        daily = d;
        monthly = mo;
        error = undefined;
      })
      .catch((e: unknown) => {
        if (!cancelled) error = errorText(e, m.time_load_failed());
      });
    return () => {
      cancelled = true;
    };
  });

  $effect(() => {
    const list = workspaces;
    void Promise.all(list.map(async (w) => [w.id, await api.timeSharing(w.id)] as const))
      .then((pairs) => {
        sharing = Object.fromEntries(pairs);
      })
      .catch(() => undefined);
  });

  const total = (rows: TimeSummaryRowView[], filter: (r: TimeSummaryRowView) => boolean): number =>
    rows.filter(filter).reduce((s, r) => s + r.seconds, 0);
  const kindsInWeek = $derived(TIME_KINDS.filter((k) => daily.some((r) => r.kind === k)));

  // 수동 기록
  interface Manual {
    day: string;
    start: string;
    end: string;
    kind: TimeKind;
    memo: string;
  }
  const manual = $state<Manual>({
    day: today,
    start: '09:00',
    end: '10:00',
    kind: 'work',
    memo: '',
  });
  // 같은 입력의 재시도는 같은 요청 id, 고친 입력은 새 id.
  const manualKey = new RetryKey();
  let manualError = $state<string | undefined>(undefined);
  let manualStatus = $state('');

  async function addManual(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    manualError = undefined;
    try {
      const started = instantOf(`${manual.day}T${manual.start}`, zone);
      let ended = instantOf(`${manual.day}T${manual.end}`, zone);
      // 끝이 시작보다 이르면 다음 날로 넘어간 기록으로 본다.
      if (ended <= started)
        ended = instantOf(`${fromDayNumber(dayNumber(manual.day) + 1)}T${manual.end}`, zone);
      const entry = {
        started_at: started,
        ended_at: ended,
        paused_seconds: 0,
        kind: manual.kind,
        memo: manual.memo,
        target: null,
      };
      await api.addTimeEntry(manualKey.for(entry), entry);
      manualKey.done();
      manual.memo = '';
      manualStatus = m.time_manual_added();
      reloadKey += 1;
    } catch (e: unknown) {
      manualError = errorText(e, m.time_manual_failed());
    }
  }

  async function toggleSharing(ws: string, on: boolean): Promise<void> {
    try {
      sharing = { ...sharing, [ws]: await api.setTimeSharing(ws, on) };
    } catch (e: unknown) {
      error = errorText(e, m.time_sharing_failed());
    }
  }

  const WEEKDAY = weekdays(settings).map((w) => w.short);
  const timeRange = (e: TimeEntryView): string =>
    `${formatClock(e.started_at, settings)}–${e.ended_at === null ? m.time_in_progress() : formatClock(e.ended_at, settings)}`;
</script>

<section class="flex flex-col gap-4" aria-labelledby="time-heading">
  <div class="flex flex-wrap items-center gap-2">
    <h1 id="time-heading" class="text-xl font-semibold">{m.time_my_time()}</h1>
    <div class="ml-auto flex gap-1" role="group" aria-label={m.time_week_nav()}>
      <Button
        size="sm"
        variant="outline"
        onclick={() => (weekStart = fromDayNumber(dayNumber(weekStart) - 7))}
        >{m.time_prev_week()}</Button
      >
      <Button size="sm" variant="outline" onclick={() => (weekStart = thisWeek)}
        >{m.time_this_week()}</Button
      >
      <Button
        size="sm"
        variant="outline"
        onclick={() => (weekStart = fromDayNumber(dayNumber(weekStart) + 7))}
        >{m.time_next_week()}</Button
      >
    </div>
  </div>
  <p class="text-sm text-muted-foreground">
    {m.time_week_hint({ from: weekStart, to: weekEnd, zone })}
  </p>
  {#if error !== undefined}
    <p role="alert" class="text-sm text-destructive">{error}</p>
  {/if}

  <div class="overflow-x-auto">
    <table class="w-full min-w-[32rem] text-sm" data-testid="time-week">
      <caption class="sr-only">{m.time_week_caption()}</caption>
      <thead class="text-left text-muted-foreground">
        <tr>
          <th scope="col" class="py-1 font-medium">{m.time_kind()}</th>
          {#each days as d, i (d)}
            <th scope="col" class="font-medium">{WEEKDAY[i]} {Number(d.slice(8))}</th>
          {/each}
          <th scope="col" class="font-medium">{m.time_total()}</th>
        </tr>
      </thead>
      <tbody>
        {#each kindsInWeek as k (k)}
          <tr class="border-t">
            <th scope="row" class="py-1 text-left font-normal">{TIME_KIND_LABEL[k]}</th>
            {#each days as d (d)}
              <td
                >{total(daily, (r) => r.kind === k && r.period === d) > 0
                  ? durationText(total(daily, (r) => r.kind === k && r.period === d))
                  : ''}</td
              >
            {/each}
            <td class="font-medium">{durationText(total(daily, (r) => r.kind === k))}</td>
          </tr>
        {:else}
          <tr><td colspan="9" class="py-2 text-muted-foreground">{m.time_week_empty()}</td></tr>
        {/each}
      </tbody>
    </table>
  </div>
  <p class="text-sm">
    {m.time_this_month()}
    {#each TIME_KINDS.filter((k) => monthly.some((r) => r.kind === k)) as k, i (k)}{i > 0
        ? ' · '
        : ''}{TIME_KIND_LABEL[k]}
      {durationText(total(monthly, (r) => r.kind === k))}{:else}{m.time_month_empty()}{/each}
  </p>

  <section class="flex flex-col gap-2" aria-labelledby="time-entries-heading">
    <h2 id="time-entries-heading" class="text-lg font-semibold">{m.time_entries_heading()}</h2>
    {#each [...days].reverse() as d (d)}
      {@const list = entries.filter((e) => dayIn(e.started_at, zone) === d)}
      {#if list.length > 0}
        <h3 class="text-sm font-semibold">{d}</h3>
        <ul class="flex flex-col gap-1">
          {#each list as e (e.id)}
            <li>
              <button
                type="button"
                data-time-entry={e.id}
                class="flex w-full flex-wrap items-baseline gap-x-2 rounded px-2 py-1 text-left text-sm hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
                onclick={() => (editing = e)}
              >
                <span class="font-mono text-xs">{timeRange(e)}</span>
                <span>{TIME_KIND_LABEL[e.kind]}</span>
                <span class="text-muted-foreground"
                  >{e.target === null
                    ? m.time_no_target()
                    : (e.target.label ?? m.time_target_hidden())}</span
                >
                {#if e.memo !== ''}<span class="text-muted-foreground">· {e.memo}</span>{/if}
                <span class="ml-auto font-medium"
                  >{durationText(e.id === timer.entry?.id ? timer.seconds : e.seconds)}</span
                >
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    {:else}
      <p class="text-sm text-muted-foreground">{m.time_entries_empty()}</p>
    {/each}
  </section>

  <section class="flex flex-col gap-2" aria-labelledby="time-manual-heading">
    <h2 id="time-manual-heading" class="text-lg font-semibold">{m.time_manual_heading()}</h2>
    <form class="flex flex-wrap items-end gap-2" onsubmit={(e) => void addManual(e)}>
      <div class="flex flex-col gap-1">
        <Label for="time-manual-day">{m.time_date()}</Label>
        <Input id="time-manual-day" type="date" bind:value={manual.day} required />
      </div>
      <div class="flex flex-col gap-1">
        <Label for="time-manual-start">{m.time_start()}</Label>
        <Input id="time-manual-start" type="time" bind:value={manual.start} required />
      </div>
      <div class="flex flex-col gap-1">
        <Label for="time-manual-end">{m.time_end()}</Label>
        <Input id="time-manual-end" type="time" bind:value={manual.end} required />
      </div>
      <div class="flex flex-col gap-1">
        <span class="text-sm font-medium" id="time-manual-kind-label">{m.time_kind()}</span>
        <Select.Root
          type="single"
          value={manual.kind}
          onValueChange={(v: string) => {
            const k = TIME_KINDS.find((x) => x === v);
            if (k !== undefined) manual.kind = k;
          }}
        >
          <Select.Trigger aria-labelledby="time-manual-kind-label"
            >{TIME_KIND_LABEL[manual.kind]}</Select.Trigger
          >
          <Select.Content>
            {#each TIME_KINDS as k (k)}
              <Select.Item value={k} label={TIME_KIND_LABEL[k]}>{TIME_KIND_LABEL[k]}</Select.Item>
            {/each}
          </Select.Content>
        </Select.Root>
      </div>
      <div class="flex flex-1 flex-col gap-1">
        <Label for="time-manual-memo">{m.time_memo()}</Label>
        <Input id="time-manual-memo" bind:value={manual.memo} maxlength={2000} />
      </div>
      <Button type="submit">{m.time_add_entry()}</Button>
    </form>
    {#if manualError !== undefined}
      <p role="alert" class="text-sm text-destructive">{manualError}</p>
    {/if}
    <p class="sr-only" aria-live="polite">{manualStatus}</p>
  </section>

  <section class="flex flex-col gap-2" aria-labelledby="time-sharing-heading">
    <h2 id="time-sharing-heading" class="text-lg font-semibold">{m.time_sharing_heading()}</h2>
    <p class="text-sm text-muted-foreground">{m.time_sharing_hint()}</p>
    {#each workspaces as w (w.id)}
      <div class="flex items-center gap-2">
        <Checkbox
          id={`time-share-${w.id}`}
          checked={sharing[w.id] === true}
          onCheckedChange={(v) => void toggleSharing(w.id, v === true)}
        />
        <Label for={`time-share-${w.id}`}>{w.name}</Label>
      </div>
    {:else}
      <p class="text-sm text-muted-foreground">{m.time_no_workspaces()}</p>
    {/each}
  </section>
</section>

<TimeEntryDialog
  entry={editing}
  {zone}
  onclose={() => (editing = undefined)}
  onchanged={() => {
    reloadKey += 1;
    void timer.load();
  }}
/>
