<script lang="ts">
  // 일정 입력 (D-31, D-50). 시각은 일정에 지정한 시간대의 벽시계 시각으로 넣고 UTC로 바꿔 보낸다.
  // 종일 일정은 날짜 그대로 보낸다.
  import type { EventInput, EventKind, EventView } from '$lib/api/client';
  import { EVENT_KINDS } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';
  import { Checkbox } from '$lib/ui/checkbox';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';
  import { Textarea } from '$lib/ui/textarea';
  import * as Select from '$lib/ui/select';

  import { trimLikeServer } from './conflict';
  import { KIND_LABEL } from './labels';
  import type { EditedWall } from './repeated';
  import { describedBy, readEditedWall, repeatedOptions } from './repeated';
  import RepeatedTimeChoice from './RepeatedTimeChoice.svelte';
  import { dayIn, instantOf, isMissingLocalTime, localTimeZone, wallTime } from './time';

  interface Props {
    idPrefix: string;
    initial: EventView | undefined;
    submitLabel: string;
    onsubmit: (input: EventInput) => Promise<void>;
    oncancel?: (() => void) | undefined;
    /** 연 때와 다르게 고친 칸이 있는지 바뀔 때마다 알린다. */
    onunsaved?: ((unsaved: boolean) => void) | undefined;
  }

  const { idPrefix, initial, submitLabel, onsubmit, oncancel, onunsaved }: Props = $props();

  /** 오늘(사용자 시간대, D-81). */
  function today(): string {
    return dayIn(new Date().toISOString(), localTimeZone());
  }

  function initialState(): {
    kind: EventKind;
    title: string;
    description: string;
    allDay: boolean;
    startDate: string;
    endDate: string;
    startWall: string;
    endWall: string;
    timezone: string;
  } {
    const e = initial;
    const tz = e?.timezone ?? localTimeZone();
    // 종일 ↔ 시각 전환 때도 그 일정의 날짜를 쓴다(오늘이 아니라).
    const startWall = e?.starts_at != null ? wallTime(e.starts_at, tz) : undefined;
    const endWall = e?.ends_at != null ? wallTime(e.ends_at, tz) : undefined;
    const startDay = e?.start_date ?? startWall?.slice(0, 10) ?? today();
    const endDay = e?.end_date ?? endWall?.slice(0, 10) ?? startDay;
    return {
      kind: e?.kind ?? 'meeting',
      title: e?.title ?? '',
      description: e?.description ?? '',
      allDay: e?.all_day ?? false,
      startDate: startDay,
      endDate: endDay,
      startWall: startWall ?? `${startDay}T09:00`,
      endWall: endWall ?? `${startDay}T10:00`,
      timezone: tz,
    };
  }

  // 연 때의 입력값. 고칠 때(D-93) 끝마다 따로 정한다: 그 끝의 시각과 시간대를 고치지 않았으면 저장된 RFC3339 값을 그대로
  // 보낸다. 입력란은 분 단위라 다시 변환하면 초가 지워지고, 두 번 있는 시각(여름시간 끝)은 다른 쪽으로 바뀔 수 있다.
  // 시간대를 바꾸면 두 끝 모두 새 시간대로 다시 읽는다. 끝이 시작보다 앞서면 서버가 거절한다(400, 화면은 그 오류를 보인다).
  const base = initialState();
  const f = $state({ ...base });
  let busy = $state(false);
  let error = $state<string | undefined>(undefined);
  /** 이미 있는 일정을 고치는지(새 일정은 D-81: 두 번 있는 시각은 앞의 것). */
  const editing = $derived(initial !== undefined);
  // 고친 끝이 두 번 있는 시각일 때 고른 UTC 시각. 그 칸이나 시간대를 고치면 지운다(저장 때 지금 후보인지 다시 본다).
  let startChoice = $state<string | undefined>(undefined);
  let endChoice = $state<string | undefined>(undefined);
  /** 두 번 있는 시각인데 고르지 않고 저장하려 한 칸(칸 옆에 알린다, D-82). */
  const unchosen = $state({ start: false, end: false });
  // 고르지 않고 저장하려 한 횟수(다시 막힐 때마다 칸 옆 알림을 새로 그린다).
  let attempt = $state(0);

  /** 그 끝을 고쳤는지: 저장된 시점이 없거나(새 일정·종일에서 바꿈) 시간대나 그 칸의 시각이 연 때와 다르면. */
  function edited(
    wall: string,
    baseWall: string,
    stored: string | null | undefined,
    zone: string,
  ): boolean {
    return stored == null || zone !== base.timezone || wall !== baseWall;
  }

  const zoneNow = $derived(f.timezone.trim());
  const startOptions = $derived(
    editing && !f.allDay && edited(f.startWall, base.startWall, initial?.starts_at, zoneNow)
      ? repeatedOptions(f.startWall, zoneNow)
      : [],
  );
  const endOptions = $derived(
    editing && !f.allDay && edited(f.endWall, base.endWall, initial?.ends_at, zoneNow)
      ? repeatedOptions(f.endWall, zoneNow)
      : [],
  );
  const startInvalid = $derived(unchosen.start && startOptions.length === 2);
  const endInvalid = $derived(unchosen.end && endOptions.length === 2);

  /** 한 끝을 보낼 값으로 읽는다. 모르는 시간대면 RangeError. */
  function readEnd(
    wall: string,
    baseWall: string,
    stored: string | null | undefined,
    chosen: string | undefined,
    zone: string,
  ): EditedWall {
    // 손대지 않은 끝은 저장된 시점(초 포함) 그대로.
    if (stored != null && !edited(wall, baseWall, stored, zone)) {
      return { kind: 'instant', instant: stored };
    }
    // 새 일정은 D-81 그대로: 없는 시각은 거절, 두 번 있는 시각은 앞의 것.
    if (!editing) {
      return isMissingLocalTime(wall, zone)
        ? { kind: 'missing' }
        : { kind: 'instant', instant: instantOf(wall, zone) };
    }
    return readEditedWall(wall, zone, chosen);
  }

  // onsubmit은 제출 이벤트 안에서 곧바로(이 함수의 첫 await 전에) 불러야 한다. EventDialog는 충돌 알림의 단추가
  // 시작한 제출인지를 requestSubmit() 동안만 두는 표시로 가린다(D-94).
  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (busy) return;
    error = undefined;
    unchosen.start = false;
    unchosen.end = false;
    let input: EventInput;
    try {
      const zone = f.timezone.trim();
      let startsAt: string | null = null;
      let endsAt: string | null = null;
      if (!f.allDay) {
        const start = readEnd(f.startWall, base.startWall, initial?.starts_at, startChoice, zone);
        const end = readEnd(f.endWall, base.endWall, initial?.ends_at, endChoice, zone);
        if (start.kind === 'missing' || end.kind === 'missing') {
          error = m.events_missing_time({ zone });
          return;
        }
        // 두 번 있는 시각은 고르기 전에는 보내지 않는다(조용히 한쪽으로 정하지 않음).
        if (start.kind === 'unchosen' || end.kind === 'unchosen') {
          unchosen.start = start.kind === 'unchosen';
          unchosen.end = end.kind === 'unchosen';
          attempt += 1;
          return;
        }
        startsAt = start.instant;
        endsAt = end.instant;
      }
      input = {
        kind: f.kind,
        title: f.title,
        description: f.description,
        all_day: f.allDay,
        start_date: f.allDay ? f.startDate : null,
        end_date: f.allDay ? f.endDate : null,
        starts_at: startsAt,
        ends_at: endsAt,
        timezone: zone,
      };
    } catch {
      error = m.events_zone_invalid();
      return;
    }
    busy = true;
    try {
      await onsubmit(input);
    } catch (e: unknown) {
      error = errorText(e, m.events_save_failed());
    } finally {
      busy = false;
    }
  }

  // 연 때와 다르게 고친 칸이 있는지(대화상자가 닫기 전에 버릴지 묻는다). 서버가 저장할 값으로 본다: 제목·설명은
  // 서버처럼 앞뒤 공백을 지우고(D-94 비교와 같음), 시간대는 보낼 때처럼 지운다. 보내지 않는 칸(종일이면 시각, 아니면
  // 날짜)은 보지 않는다.
  const unsaved = $derived(
    f.kind !== base.kind ||
      trimLikeServer(f.title) !== trimLikeServer(base.title) ||
      trimLikeServer(f.description) !== trimLikeServer(base.description) ||
      f.allDay !== base.allDay ||
      (f.allDay
        ? f.startDate !== base.startDate || f.endDate !== base.endDate
        : f.startWall !== base.startWall ||
          f.endWall !== base.endWall ||
          f.timezone.trim() !== base.timezone),
  );
  $effect(() => {
    onunsaved?.(unsaved);
    return () => {
      onunsaved?.(false);
    };
  });
</script>

<form class="grid gap-3 md:grid-cols-2" onsubmit={(e) => void submit(e)}>
  <div class="flex flex-col gap-1 md:col-span-2">
    <Label for={`${idPrefix}-title`}>{m.events_title()}</Label>
    <Input id={`${idPrefix}-title`} bind:value={f.title} maxlength={200} required />
  </div>
  <div class="flex flex-col gap-1">
    <span class="text-sm font-medium" id={`${idPrefix}-kind-label`}>{m.events_kind()}</span>
    <Select.Root
      type="single"
      value={f.kind}
      onValueChange={(v: string) => {
        const k = EVENT_KINDS.find((x) => x === v);
        if (k !== undefined) f.kind = k;
      }}
    >
      <Select.Trigger aria-labelledby={`${idPrefix}-kind-label`}
        >{KIND_LABEL[f.kind]}</Select.Trigger
      >
      <Select.Content>
        {#each EVENT_KINDS as k (k)}
          <Select.Item value={k} label={KIND_LABEL[k]}>{KIND_LABEL[k]}</Select.Item>
        {/each}
      </Select.Content>
    </Select.Root>
  </div>
  <div class="flex items-center gap-2 self-end">
    <Checkbox
      id={`${idPrefix}-all-day`}
      checked={f.allDay}
      onCheckedChange={(v) => {
        f.allDay = v === true;
        // 종일로 바꿨다 되돌리면 시각 칸을 다시 고르는 것이므로 앞서 고른 것과 알림을 지운다.
        startChoice = undefined;
        endChoice = undefined;
        unchosen.start = false;
        unchosen.end = false;
      }}
    />
    <Label for={`${idPrefix}-all-day`}>{m.events_all_day()}</Label>
  </div>
  {#if f.allDay}
    <div class="flex flex-col gap-1">
      <Label for={`${idPrefix}-start-date`}>{m.events_start_date()}</Label>
      <Input id={`${idPrefix}-start-date`} type="date" bind:value={f.startDate} required />
    </div>
    <div class="flex flex-col gap-1">
      <Label for={`${idPrefix}-end-date`}>{m.events_end_date()}</Label>
      <Input id={`${idPrefix}-end-date`} type="date" bind:value={f.endDate} required />
    </div>
  {:else}
    <div class="flex flex-col gap-1">
      <Label for={`${idPrefix}-starts`}>{m.events_starts()}</Label>
      <Input
        id={`${idPrefix}-starts`}
        type="datetime-local"
        bind:value={f.startWall}
        required
        oninput={() => {
          startChoice = undefined;
          unchosen.start = false;
        }}
        aria-invalid={startInvalid}
        aria-describedby={describedBy(
          startOptions.length === 2 && `${idPrefix}-starts-repeated-legend`,
          startInvalid && `${idPrefix}-starts-repeated-error`,
        )}
      />
      {#if startOptions.length === 2}
        <RepeatedTimeChoice
          id={`${idPrefix}-starts`}
          field={m.events_starts()}
          zone={zoneNow}
          options={startOptions}
          chosen={startChoice}
          invalid={startInvalid}
          {attempt}
          onchoose={(instant: string) => {
            startChoice = instant;
            unchosen.start = false;
          }}
        />
      {/if}
    </div>
    <div class="flex flex-col gap-1">
      <Label for={`${idPrefix}-ends`}>{m.events_ends()}</Label>
      <Input
        id={`${idPrefix}-ends`}
        type="datetime-local"
        bind:value={f.endWall}
        required
        oninput={() => {
          endChoice = undefined;
          unchosen.end = false;
        }}
        aria-invalid={endInvalid}
        aria-describedby={describedBy(
          endOptions.length === 2 && `${idPrefix}-ends-repeated-legend`,
          endInvalid && `${idPrefix}-ends-repeated-error`,
        )}
      />
      {#if endOptions.length === 2}
        <RepeatedTimeChoice
          id={`${idPrefix}-ends`}
          field={m.events_ends()}
          zone={zoneNow}
          options={endOptions}
          chosen={endChoice}
          invalid={endInvalid}
          {attempt}
          onchoose={(instant: string) => {
            endChoice = instant;
            unchosen.end = false;
          }}
        />
      {/if}
    </div>
    <div class="flex flex-col gap-1 md:col-span-2">
      <Label for={`${idPrefix}-tz`}>{m.events_time_zone()}</Label>
      <!-- 시간대를 바꾸면 두 끝을 새 시간대로 다시 읽으므로 고른 것도 지운다(D-93). -->
      <Input
        id={`${idPrefix}-tz`}
        bind:value={f.timezone}
        autocomplete="off"
        required
        oninput={() => {
          startChoice = undefined;
          endChoice = undefined;
          unchosen.start = false;
          unchosen.end = false;
        }}
      />
    </div>
  {/if}
  <div class="flex flex-col gap-1 md:col-span-2">
    <Label for={`${idPrefix}-description`}>{m.events_description()}</Label>
    <Textarea id={`${idPrefix}-description`} bind:value={f.description} rows={2} />
  </div>
  {#if error !== undefined}
    <p role="alert" class="text-sm text-destructive md:col-span-2">{error}</p>
  {/if}
  <div class="flex gap-2 md:col-span-2">
    <Button type="submit" disabled={busy}>{submitLabel}</Button>
    {#if oncancel !== undefined}
      <Button type="button" variant="ghost" onclick={oncancel}>{m.common_cancel()}</Button>
    {/if}
  </div>
</form>
