<script lang="ts">
  import type { TimeEntryView, TimeKind, TimeSummaryRowView } from '$lib/features/time/model';
  import { TIME_KINDS } from '$lib/features/time/model';
  import { timeData } from '$lib/features/time/data';
  // 내 시간 (D-117): 본인 기록과 날짜·종류별 합계, 수동 기록.
  // 기록은 본인만 본다(워크스페이스 관리자에게도 보이지 않는다). 날짜는 보는 사람 시간대, 기록은 시작한 날에 센다.
  import { onMount, untrack } from 'svelte';

  import { newRequestId } from '$lib/api/client';
  import { createFormDraft } from '$lib/features/common/form-draft.svelte';
  import { me } from '$lib/me.svelte';
  import DiscardChangesConfirm from '$lib/features/common/DiscardChangesConfirm.svelte';
  import { errorText } from '$lib/api/errors';
  import { displaySettings, formatClock, weekdays } from '$lib/display';
  import { startOfWeek } from '$lib/features/events/calendar';
  import { dayIn, instantOf, isMissingLocalTime } from '$lib/features/events/time';
  import { dayNumber, fromDayNumber } from '$lib/features/tasks/gantt';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';
  import { org } from '$lib/org.svelte';
  import { notifyDone } from '$lib/features/feedback/feedback.svelte';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';
  import * as Select from '$lib/ui/select';

  import { durationText, TIME_KIND_LABEL } from './format';
  import TimeEntryDialog from './TimeEntryDialog.svelte';
  import { timer } from './timer.svelte';

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
  // 첫 목록을 받기 전에는 빈 표·"기록이 없습니다" 대신 불러오는 중이라고 보인다.
  let loaded = $state(false);
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
  let listOwner: string | undefined;
  let listOrganization: string | undefined;
  $effect(() => {
    const { from, to } = request;
    const userId = me.userId;
    const organizationId = org.currentId;
    if (listOwner !== userId || listOrganization !== organizationId) {
      entries = [];
      daily = [];
      monthly = [];
      loaded = false;
      error = undefined;
      editing = undefined;
      listOwner = userId;
      listOrganization = organizationId;
    }
    if (organizationId === undefined) return;
    const monthFrom = `${today.slice(0, 7)}-01`;
    let cancelled = false;
    Promise.all([
      timeData.timeEntries(from, to, zone),
      timeData.timeSummary(from, to, zone, 'day'),
      timeData.timeSummary(monthFrom, today, zone, 'month'),
    ])
      .then(([e, d, mo]) => {
        if (cancelled || userId !== me.userId) return;
        entries = e;
        daily = d;
        monthly = mo;
        loaded = true;
        error = undefined;
      })
      .catch((e: unknown) => {
        if (!cancelled) error = errorText(e, m.time_load_failed());
      });
    return () => {
      cancelled = true;
    };
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
  let cleanManual = $state<Manual>({ ...manual });
  // 같은 입력의 재시도는 같은 요청 id, 고친 입력은 새 id.
  let pending = $state<{ id: string; signature: string } | null>(null);
  let manualBusy = $state(false);
  let manualError = $state<string | undefined>(undefined);
  /** 그 시간대에 없는 시각을 넣은 칸(칸 옆에 알린다, D-82). */
  let manualMissing = $state<'start' | 'end' | undefined>(undefined);

  interface ManualDraft {
    form: Manual;
    zone: string;
    pending: { id: string; signature: string } | null;
  }
  let manualZone = $state(zone);
  let manualOwner = me.userId;
  function resetManual(): void {
    if (manualOwner !== me.userId) {
      cleanManual = { day: today, start: '09:00', end: '10:00', kind: 'work', memo: '' };
      manualOwner = me.userId;
    }
    Object.assign(manual, cleanManual);
    pending = null;
    manualZone = zone;
    manualError = undefined;
    manualMissing = undefined;
    manualBusy = false;
  }
  const manualDraft = createFormDraft<ManualDraft>({
    // 시간 기록은 조직 인자가 없는 본인 기록이고(D-117) 수동 기록 입력에 조직 범위 값이 없다. 초안은 사용자별·탭별이라
    // (D-27) 이름에 조직을 넣지 않는다. 조직을 고르기 전에는 폼이 없으므로 보존하지 않는다.
    name: () => (org.currentId === undefined ? undefined : 'time-manual'),
    snapshot: () => ({ form: { ...manual }, zone: manualZone, pending }),
    parse: (value) => {
      if (
        typeof value !== 'object' ||
        value === null ||
        !('form' in value) ||
        !('zone' in value) ||
        typeof value.zone !== 'string' ||
        !('pending' in value)
      )
        return undefined;
      const f = value.form;
      if (
        typeof f !== 'object' ||
        f === null ||
        !('day' in f) ||
        typeof f.day !== 'string' ||
        !('start' in f) ||
        typeof f.start !== 'string' ||
        !('end' in f) ||
        typeof f.end !== 'string' ||
        !('memo' in f) ||
        typeof f.memo !== 'string' ||
        !('kind' in f)
      )
        return undefined;
      const kind = TIME_KINDS.find((k) => k === f.kind);
      if (kind === undefined) return undefined;
      const p = value.pending;
      let retry: ManualDraft['pending'] = null;
      if (p !== null) {
        if (
          typeof p !== 'object' ||
          !('id' in p) ||
          typeof p.id !== 'string' ||
          !('signature' in p) ||
          typeof p.signature !== 'string'
        )
          return undefined;
        retry = { id: p.id, signature: p.signature };
      }
      return {
        form: { day: f.day, start: f.start, end: f.end, memo: f.memo, kind },
        zone: value.zone,
        pending: retry,
      };
    },
    restore: (value) => {
      Object.assign(manual, value.form);
      manualZone = value.zone;
      pending = value.pending;
    },
    reset: resetManual,
    dirty: () =>
      pending !== null ||
      manualZone !== zone ||
      manual.day !== cleanManual.day ||
      manual.start !== cleanManual.start ||
      manual.end !== cleanManual.end ||
      manual.kind !== cleanManual.kind ||
      manual.memo !== cleanManual.memo,
    busy: () => manualBusy,
  });
  onMount(manualDraft.attach);

  async function addManual(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (manualBusy || !manualDraft.active || manualDraft.blocked) return;
    manualError = undefined;
    let attempt: ReturnType<typeof manualDraft.capture>;
    try {
      // 그 시간대에 없는 시각(여름시간 시작일의 건너뛴 시각, D-81)은 조용히 옮겨 저장하지 않는다. 변환 전에 본다(없는
      // 시각을 변환하면 틈만큼 뒤로 밀려 엉뚱한 길이가 된다). 끝은 실제로 쓰일 날의 시각만 본다: 입력한 끝이 시작보다
      // 이르면(벽시계 기준. 밀린 값으로 비교하면 틈 안의 끝이 시작보다 늦어 보인다) 다음 날로 넘어간 기록이므로 다음 날의
      // 끝을 본다. 두 값 모두 type="time"의 0을 채운 `HH:mm`이라 문자열 비교가 시각 순서와 같다.
      manualMissing = undefined;
      const startWall = `${manual.day}T${manual.start}`;
      if (isMissingLocalTime(startWall, manualZone)) {
        manualMissing = 'start';
        return;
      }
      const endDay =
        manual.end <= manual.start ? fromDayNumber(dayNumber(manual.day) + 1) : manual.day;
      const endWall = `${endDay}T${manual.end}`;
      if (isMissingLocalTime(endWall, manualZone)) {
        manualMissing = 'end';
        return;
      }
      const started = instantOf(startWall, manualZone);
      const ended = instantOf(endWall, manualZone);
      const entry = {
        started_at: started,
        ended_at: ended,
        paused_seconds: 0,
        kind: manual.kind,
        memo: manual.memo,
        target: null,
      };
      const signature = JSON.stringify(entry);
      if (pending?.signature !== signature) pending = { id: newRequestId(), signature };
      const requestId = pending.id;
      attempt = manualDraft.capture();
      if (attempt === undefined) return;
      manualBusy = true;
      await timeData.addTimeEntry(requestId, entry);
      if (!manualDraft.isOwner(attempt)) return;
      // 입력은 보내는 동안 잠겨 있다. 그래도 바뀌었으면 지우지 않고 남긴다.
      if (manualDraft.isCurrent(attempt)) {
        cleanManual = { ...manual, memo: '' };
        manualDraft.complete(attempt);
      }
      notifyDone(m.time_manual_added());
      reloadKey += 1;
    } catch (e: unknown) {
      // 같은 사용자의 실패는 입력이 바뀌었어도 알린다(계정이 바뀌었으면 숨긴다).
      if (attempt === undefined || manualDraft.isOwner(attempt))
        manualError = errorText(e, m.time_manual_failed());
    } finally {
      if (attempt !== undefined && manualDraft.isOwner(attempt)) manualBusy = false;
    }
  }

  const WEEKDAY = weekdays(settings).map((w) => w.short);
  const timeRange = (e: TimeEntryView): string =>
    `${formatClock(e.started_at, settings)}–${e.ended_at === null ? m.time_in_progress() : formatClock(e.ended_at, settings)}`;
</script>

{#if org.currentId === undefined}
  <p role="status">{m.app_organization_required()}</p>
{:else}
  <section class="flex flex-col gap-4" aria-labelledby="time-heading">
    <div class="flex flex-wrap items-center gap-2">
      <h1 id="time-heading" class="text-xl font-semibold">{m.time_my_time()}</h1>
      <!-- 375px·200%에서는 세 단추가 한 줄에 들지 않아 줄바꿈한다(가로 넘침 금지, D-110). -->
      <div
        class="ml-auto flex flex-wrap justify-end gap-1"
        role="group"
        aria-label={m.time_week_nav()}
      >
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
      <p role="alert" class="text-sm text-destructive">
        {error}
        {#if !loaded}
          <Button
            size="sm"
            variant="outline"
            class="ml-2"
            onclick={() => {
              error = undefined;
              reloadKey += 1;
            }}>{m.common_retry()}</Button
          >
        {/if}
      </p>
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
            <tr
              ><td colspan="9" class="py-2 text-muted-foreground"
                >{loaded ? m.time_week_empty() : error === undefined ? m.app_loading() : ''}</td
              ></tr
            >
          {/each}
        </tbody>
      </table>
    </div>
    <!-- 이번 달 합계 한 줄은 불러오는 중에서 결과로 바뀔 때 낭독기가 읽는다(주간 표는 칸이 많아 알림 영역으로 두지 않는다). -->
    <p class="text-sm" aria-live="polite">
      {m.time_this_month()}
      {#each TIME_KINDS.filter((k) => monthly.some((r) => r.kind === k)) as k, i (k)}{i > 0
          ? ' · '
          : ''}{TIME_KIND_LABEL[k]}
        {durationText(total(monthly, (r) => r.kind === k))}{:else}{loaded
          ? m.time_month_empty()
          : error === undefined
            ? m.app_loading()
            : ''}{/each}
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
      {/each}
      <!-- 날짜는 늘 7개라 위 목록의 {:else}로는 빈 주를 알릴 수 없다. 기록이 없을 때 따로 알린다. -->
      {#if entries.length === 0}
        <p class="text-sm text-muted-foreground">
          {loaded ? m.time_entries_empty() : error === undefined ? m.app_loading() : ''}
        </p>
      {/if}
    </section>

    <section class="flex flex-col gap-2" aria-labelledby="time-manual-heading">
      <h2 id="time-manual-heading" class="text-lg font-semibold">{m.time_manual_heading()}</h2>
      <p class="text-sm text-muted-foreground">{m.settings_time_zone()}: {manualZone}</p>
      <!-- 보내는 동안 입력과 되살리기를 잠근다(D-97). 그사이 바뀐 입력이 결과(성공·실패) 알림을 가리지 않게 한다. -->
      {#each manualDraft.offers as offer (offer.key)}
        <fieldset
          class="flex flex-wrap items-center gap-2"
          disabled={manualBusy}
          data-testid="time-manual-draft-offer"
        >
          <p>{m.common_draft_available()}</p>
          <Button
            size="sm"
            onclick={() => {
              manualDraft.restore(offer);
            }}>{m.common_draft_restore()}</Button
          >
          <Button
            size="sm"
            variant="outline"
            onclick={() => {
              manualDraft.discard(offer);
            }}>{m.common_draft_discard()}</Button
          >
        </fieldset>
      {/each}
      <fieldset class="min-w-0" disabled={manualBusy}>
        <form class="flex flex-wrap items-end gap-2" onsubmit={(e) => void addManual(e)}>
          <div class="flex flex-col gap-1">
            <Label for="time-manual-day">{m.time_date()}</Label>
            <!-- 없는 시각인지는 날짜와 시각을 함께 보고 정하므로 날짜만 바꿔도 칸 옆 오류를 지운다. -->
            <Input
              id="time-manual-day"
              type="date"
              bind:value={manual.day}
              required
              oninput={() => (manualMissing = undefined)}
            />
          </div>
          <div class="flex flex-col gap-1">
            <Label for="time-manual-start">{m.time_start()}</Label>
            <Input
              id="time-manual-start"
              type="time"
              bind:value={manual.start}
              required
              oninput={() => (manualMissing = undefined)}
              aria-invalid={manualMissing === 'start'}
              aria-describedby={manualMissing === 'start' ? 'time-manual-start-error' : undefined}
            />
            {#if manualMissing === 'start'}
              <p id="time-manual-start-error" class="text-xs text-destructive" role="alert">
                {m.events_missing_time({ zone: manualZone })}
              </p>
            {/if}
          </div>
          <div class="flex flex-col gap-1">
            <Label for="time-manual-end">{m.time_end()}</Label>
            <Input
              id="time-manual-end"
              type="time"
              bind:value={manual.end}
              required
              oninput={() => (manualMissing = undefined)}
              aria-invalid={manualMissing === 'end'}
              aria-describedby={manualMissing === 'end' ? 'time-manual-end-error' : undefined}
            />
            {#if manualMissing === 'end'}
              <p id="time-manual-end-error" class="text-xs text-destructive" role="alert">
                {m.events_missing_time({ zone: manualZone })}
              </p>
            {/if}
          </div>
          <div class="flex flex-col gap-1">
            <!-- bits-ui Select는 조상 fieldset의 disabled를 보지 않고 열리므로 보내는 중에는 따로 잠근다. -->
            <span class="text-sm font-medium" id="time-manual-kind-label">{m.time_kind()}</span>
            <Select.Root
              type="single"
              disabled={manualBusy}
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
                  <Select.Item value={k} label={TIME_KIND_LABEL[k]}
                    >{TIME_KIND_LABEL[k]}</Select.Item
                  >
                {/each}
              </Select.Content>
            </Select.Root>
          </div>
          <div class="flex flex-1 flex-col gap-1">
            <Label for="time-manual-memo">{m.time_memo()}</Label>
            <Input id="time-manual-memo" bind:value={manual.memo} maxlength={2000} />
          </div>
          <Button type="submit" disabled={manualBusy || !manualDraft.active || manualDraft.blocked}
            >{m.time_add_entry()}</Button
          >
        </form>
      </fieldset>
      {#if manualError !== undefined}
        <p role="alert" class="text-sm text-destructive">{manualError}</p>
      {/if}
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
{/if}

<DiscardChangesConfirm
  open={manualDraft.leave.open}
  onkeep={manualDraft.leave.keep}
  ondiscard={manualDraft.discardAndLeave}
/>
