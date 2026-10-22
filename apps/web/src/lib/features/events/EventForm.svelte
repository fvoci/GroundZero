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
  import * as Select from '$lib/ui/select';

  import { trimLikeServer } from './conflict';
  import { KIND_LABEL } from './labels';
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

  // 연 때의 입력값. 시각·시간대를 고치지 않았으면 원래 RFC3339 값을 그대로 보낸다: 입력란은 분 단위라 다시 변환하면 초가
  // 지워지고, 두 번 있는 시각(여름시간 끝)은 앞의 것으로 바뀐다(제목만 고쳤는데 시각이 바뀌는 일이 없게).
  const base = initialState();
  const f = $state({ ...base });
  let busy = $state(false);
  let error = $state<string | undefined>(undefined);

  // onsubmit은 제출 이벤트 안에서 곧바로(이 함수의 첫 await 전에) 불러야 한다. EventDialog는 충돌 알림의 단추가
  // 시작한 제출인지를 requestSubmit() 동안만 두는 표시로 가린다(D-94).
  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (busy) return;
    error = undefined;
    let input: EventInput;
    try {
      const zone = f.timezone.trim();
      if (
        !f.allDay &&
        (isMissingLocalTime(f.startWall, zone) || isMissingLocalTime(f.endWall, zone))
      ) {
        error = m.events_missing_time({ zone });
        return;
      }
      const kept = (wall: string, baseWall: string, stored: string | null): string | null =>
        zone === base.timezone && wall === baseWall ? stored : null;
      input = {
        kind: f.kind,
        title: f.title,
        description: f.description,
        all_day: f.allDay,
        start_date: f.allDay ? f.startDate : null,
        end_date: f.allDay ? f.endDate : null,
        starts_at: f.allDay
          ? null
          : (kept(f.startWall, base.startWall, initial?.starts_at ?? null) ??
            instantOf(f.startWall, zone)),
        ends_at: f.allDay
          ? null
          : (kept(f.endWall, base.endWall, initial?.ends_at ?? null) ?? instantOf(f.endWall, zone)),
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
      <Input id={`${idPrefix}-starts`} type="datetime-local" bind:value={f.startWall} required />
    </div>
    <div class="flex flex-col gap-1">
      <Label for={`${idPrefix}-ends`}>{m.events_ends()}</Label>
      <Input id={`${idPrefix}-ends`} type="datetime-local" bind:value={f.endWall} required />
    </div>
    <div class="flex flex-col gap-1 md:col-span-2">
      <Label for={`${idPrefix}-tz`}>{m.events_time_zone()}</Label>
      <Input id={`${idPrefix}-tz`} bind:value={f.timezone} autocomplete="off" required />
    </div>
  {/if}
  <div class="flex flex-col gap-1 md:col-span-2">
    <Label for={`${idPrefix}-description`}>{m.events_description()}</Label>
    <textarea
      id={`${idPrefix}-description`}
      bind:value={f.description}
      rows={2}
      class="rounded-md border border-input bg-transparent px-3 py-2 text-sm focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
    ></textarea>
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
