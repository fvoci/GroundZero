<script lang="ts">
  // 일정 입력 (D-31, D-50). 시각은 일정에 지정한 시간대의 벽시계 시각으로 넣고 UTC로 바꿔 보낸다.
  // 종일 일정은 날짜 그대로 보낸다.
  import type { EventInput, EventKind, EventView } from '$lib/api/client';
  import { EVENT_KINDS } from '$lib/api/client';
  import { Button } from '$lib/ui/button';
  import { Checkbox } from '$lib/ui/checkbox';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';
  import * as Select from '$lib/ui/select';

  import { KIND_LABEL } from './labels';
  import { dayIn, instantOf, localTimeZone, wallTime } from './time';

  interface Props {
    idPrefix: string;
    initial: EventView | undefined;
    submitLabel: string;
    onsubmit: (input: EventInput) => Promise<void>;
    oncancel?: (() => void) | undefined;
  }

  const { idPrefix, initial, submitLabel, onsubmit, oncancel }: Props = $props();

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

  const f = $state(initialState());
  let busy = $state(false);
  let error = $state<string | undefined>(undefined);

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (busy) return;
    error = undefined;
    let input: EventInput;
    try {
      input = {
        kind: f.kind,
        title: f.title,
        description: f.description,
        all_day: f.allDay,
        start_date: f.allDay ? f.startDate : null,
        end_date: f.allDay ? f.endDate : null,
        starts_at: f.allDay ? null : instantOf(f.startWall, f.timezone.trim()),
        ends_at: f.allDay ? null : instantOf(f.endWall, f.timezone.trim()),
        timezone: f.timezone.trim(),
      };
    } catch {
      error = '시간대 이름을 확인하세요(예: Asia/Seoul).';
      return;
    }
    busy = true;
    try {
      await onsubmit(input);
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : '저장하지 못했습니다.';
    } finally {
      busy = false;
    }
  }
</script>

<form class="grid gap-3 md:grid-cols-2" onsubmit={(e) => void submit(e)}>
  <div class="flex flex-col gap-1 md:col-span-2">
    <Label for={`${idPrefix}-title`}>제목</Label>
    <Input id={`${idPrefix}-title`} bind:value={f.title} maxlength={200} required />
  </div>
  <div class="flex flex-col gap-1">
    <span class="text-sm font-medium" id={`${idPrefix}-kind-label`}>종류</span>
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
    <Label for={`${idPrefix}-all-day`}>종일</Label>
  </div>
  {#if f.allDay}
    <div class="flex flex-col gap-1">
      <Label for={`${idPrefix}-start-date`}>시작일</Label>
      <Input id={`${idPrefix}-start-date`} type="date" bind:value={f.startDate} required />
    </div>
    <div class="flex flex-col gap-1">
      <Label for={`${idPrefix}-end-date`}>종료일</Label>
      <Input id={`${idPrefix}-end-date`} type="date" bind:value={f.endDate} required />
    </div>
  {:else}
    <div class="flex flex-col gap-1">
      <Label for={`${idPrefix}-starts`}>시작</Label>
      <Input id={`${idPrefix}-starts`} type="datetime-local" bind:value={f.startWall} required />
    </div>
    <div class="flex flex-col gap-1">
      <Label for={`${idPrefix}-ends`}>종료</Label>
      <Input id={`${idPrefix}-ends`} type="datetime-local" bind:value={f.endWall} required />
    </div>
    <div class="flex flex-col gap-1 md:col-span-2">
      <Label for={`${idPrefix}-tz`}>시간대 (시작·종료 시각의 기준)</Label>
      <Input id={`${idPrefix}-tz`} bind:value={f.timezone} autocomplete="off" required />
    </div>
  {/if}
  <div class="flex flex-col gap-1 md:col-span-2">
    <Label for={`${idPrefix}-description`}>설명</Label>
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
      <Button type="button" variant="ghost" onclick={oncancel}>취소</Button>
    {/if}
  </div>
</form>
