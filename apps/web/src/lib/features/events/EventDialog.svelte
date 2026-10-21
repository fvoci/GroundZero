<script lang="ts">
  // 일정 상세 대화상자: 보기·고치기·지우기. 고치기는 기준 버전으로 충돌을 막고, 충돌하면 입력을 두고 서버 내용을
  // 보여 준다(D-53과 같은 원칙). 지우기는 한 번 더 묻는다(행은 남고 휴지통 조각에서 되살린다).
  import type { EventInput, EventView, RecurrenceRule } from '$lib/api/client';
  import { api, ApiError } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import { formatDay } from '$lib/display';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';
  import * as Dialog from '$lib/ui/dialog';

  import EventForm from './EventForm.svelte';
  import RecurrenceEditor from './RecurrenceEditor.svelte';
  import { describeRule, describeWhen, KIND_LABEL } from './labels';
  import { localTimeZone } from './time';

  interface Props {
    eventId: string | undefined;
    onclose: () => void;
    onchanged: () => void;
    /** 닫은 뒤 초점을 돌려줄 요소(주소나 목록에서 열어 여는 단추를 대화상자가 모르는 경우). */
    returnFocus?: (() => HTMLElement | null) | undefined;
    /** 달력에서 반복 일정의 한 회차를 열었으면 그 회차 날짜(일정 시간대 기준). */
    occurrence?: string | undefined;
  }

  const { eventId, onclose, onchanged, returnFocus, occurrence }: Props = $props();

  let event = $state<EventView | undefined>(undefined);
  let error = $state<string | undefined>(undefined);
  let conflict = $state<{ current: EventView; input: EventInput } | undefined>(undefined);
  let confirmDelete = $state(false);
  let epoch = $state(0);
  // 반복 편집기는 일정 저장 때 다시 그리지 않는다(저장 안 한 규칙 입력을 지우지 않게). 서버 규칙이 화면 규칙과 달라질
  // 때("서버 내용 불러오기", 충돌 뒤 "내 내용으로 저장")만 서버 규칙으로 다시 만든다(그러지 않으면 요약은 서버 규칙인데
  // 칸은 옛 규칙이라, 저장하면 옛 규칙을 다시 보낸다).
  let ruleEpoch = $state(0);
  // 보내는 중인 요청 수. 보내는 동안은 닫지 않는다(늦게 온 결과가 다시 연 창에 들어가지 않게, CalendarTaskDialog와 같음).
  let pending = $state(0);
  const busy = $derived(pending > 0);
  // 지금 보이는 일정을 연 차례. 요청을 보낸 뒤 다른 일정을 열었으면(또는 닫았다 다시 열었으면) 늦은 결과를 버린다.
  let shown = 0;
  // 일정 입력이 바탕으로 삼은 버전. 반복 규칙 저장은 입력을 다시 그리지 않으므로 `event.version`과 따로 둔다.
  let formVersion = 0;
  const zone = localTimeZone();

  $effect(() => {
    const id = eventId;
    shown += 1;
    event = undefined;
    error = undefined;
    conflict = undefined;
    confirmDelete = false;
    if (id === undefined) return;
    let cancelled = false;
    api
      .event(id)
      .then((e) => {
        if (cancelled) return;
        event = e;
        formVersion = e.version;
      })
      .catch((e: unknown) => {
        if (!cancelled)
          error =
            e instanceof ApiError && e.status === 404
              ? m.events_not_found()
              : errorText(e, m.events_event_load_failed());
      });
    return () => {
      cancelled = true;
    };
  });

  const canEdit = $derived(event !== undefined && event.access !== 'read');

  function sameRule(a: RecurrenceRule | null, b: RecurrenceRule | null): boolean {
    if (a === null || b === null) return a === b;
    return (
      a.freq === b.freq &&
      a.interval === b.interval &&
      a.until === b.until &&
      a.count === b.count &&
      a.weekdays.length === b.weekdays.length &&
      a.weekdays.every((d, i) => d === b.weekdays[i])
    );
  }

  /** 보내는 동안 닫지 않게 센다. */
  async function sending<T>(request: Promise<T>): Promise<T> {
    pending += 1;
    try {
      return await request;
    } finally {
      pending -= 1;
    }
  }

  async function save(input: EventInput, baseVersion: number): Promise<void> {
    if (event === undefined) return;
    const mine = shown;
    const result = await sending(api.updateEvent(event.id, baseVersion, input));
    if (mine !== shown) {
      // 그사이 다른 일정을 열었다. 고친 일정은 목록에서 다시 읽게만 한다.
      if (result.kind === 'updated') onchanged();
      return;
    }
    if (result.kind === 'conflict') {
      conflict = { current: result.current, input };
      return;
    }
    conflict = undefined;
    // 보통 저장은 기준 버전이 맞아야 되므로 규칙이 그대로다. 충돌 뒤 "내 내용으로 저장"이면 저장된 일정에 다른 사람이
    // 바꾼 규칙이 들어 있으니 반복 편집기도 그 규칙으로 다시 만든다(요약만 바뀌고 칸은 옛 규칙이면 옛 규칙을 다시 보냄).
    if (!sameRule(result.event.recurrence, event.recurrence)) ruleEpoch += 1;
    event = result.event;
    formVersion = result.event.version;
    epoch += 1;
    onchanged();
  }

  async function saveRule(rule: RecurrenceRule | null): Promise<void> {
    if (event === undefined) return;
    const mine = shown;
    const before = event.version;
    const id = event.id;
    // 편집 중인 일정 입력과 반복 편집기의 상태(저장 알림·초점)를 지우지 않게 다시 그리지 않는다. 기준은 화면이 보이는
    // 규칙이다(그사이 바뀌었으면 서버가 409, 감사 W21).
    let saved: EventView;
    try {
      saved = await sending(api.setEventRecurrence(id, rule, event.recurrence));
    } catch (e: unknown) {
      if (e instanceof ApiError && e.code === 'rule_conflict' && mine === shown) {
        // 지금 규칙을 요약에 보이고 다음 저장의 기준으로 삼는다. 입력한 칸은 그대로 둔다(다시 저장하면 덮어씀).
        // 일정 입력의 기준 판은 두어, 일정 저장이 다른 사람의 변경을 충돌로 드러내게 한다.
        try {
          const fresh = await sending(api.event(id));
          if (mine === shown) event = fresh;
        } catch {
          // 다시 읽지 못했어도 이유는 반복 편집기가 보인다.
        }
      }
      throw e;
    }
    if (mine !== shown) {
      onchanged();
      return;
    }
    // 이 저장 하나만큼 버전이 올랐으면 입력의 기준도 따라간다. 그 사이 다른 사람이 일정을 고쳤으면 기준을 두어
    // 입력 저장이 충돌로 드러나게 한다(새 버전을 기준으로 보내면 그 사람의 변경을 모르고 덮어쓴다).
    if (formVersion === before && saved.version === before + 1) formVersion = saved.version;
    event = saved;
    onchanged();
  }

  async function skipOccurrence(date: string): Promise<void> {
    if (event === undefined) return;
    const mine = shown;
    try {
      await sending(api.excludeOccurrence(event.id, date));
      onchanged();
      if (mine === shown) onclose();
    } catch (e: unknown) {
      if (mine === shown) error = errorText(e, m.events_skip_failed());
    }
  }

  async function remove(): Promise<void> {
    if (event === undefined) return;
    const mine = shown;
    try {
      await sending(api.deleteEvent(event.id));
      onchanged();
      if (mine === shown) onclose();
    } catch (e: unknown) {
      if (mine === shown) error = errorText(e, m.events_delete_failed());
    }
  }

  function close(): void {
    if (busy) return;
    onclose();
  }
</script>

<Dialog.Root
  open={eventId !== undefined}
  onOpenChange={(o: boolean) => {
    if (!o) close();
  }}
>
  <Dialog.Content
    class="max-h-[90vh] overflow-y-auto sm:max-w-2xl"
    escapeKeydownBehavior={busy ? 'ignore' : 'close'}
    interactOutsideBehavior={busy ? 'ignore' : 'close'}
    showCloseButton={!busy}
    onCloseAutoFocus={(e: Event) => {
      const target = returnFocus?.() ?? null;
      if (target !== null) {
        e.preventDefault();
        target.focus();
      }
    }}
  >
    <Dialog.Header>
      <Dialog.Title>{event?.title ?? m.events_event()}</Dialog.Title>
      <Dialog.Description>
        {#if event !== undefined}{KIND_LABEL[event.kind]} · {describeWhen(event, zone)}{/if}
      </Dialog.Description>
    </Dialog.Header>
    {#if error !== undefined}
      <p role="alert" class="text-sm text-destructive">{error}</p>
    {/if}
    {#if event !== undefined}
      {@const current = event}
      {#if conflict !== undefined}
        {@const c = conflict}
        <div class="rounded-md border border-amber-500/50 p-3 text-sm" role="alert">
          <p>
            {m.events_conflict({ title: c.current.title, when: describeWhen(c.current, zone) })}
          </p>
          <div class="mt-2 flex gap-2">
            <Button
              size="sm"
              variant="outline"
              onclick={() => {
                event = c.current;
                formVersion = c.current.version;
                conflict = undefined;
                epoch += 1;
                ruleEpoch += 1;
              }}>{m.events_load_server()}</Button
            >
            <Button size="sm" onclick={() => void save(c.input, c.current.version)}
              >{m.events_save_mine()}</Button
            >
          </div>
        </div>
      {/if}
      {#if canEdit}
        {#key epoch}
          <EventForm
            idPrefix="event-edit"
            initial={current}
            submitLabel={m.common_save()}
            onsubmit={(input: EventInput) => save(input, formVersion)}
          />
        {/key}
        {#key ruleEpoch}
          <RecurrenceEditor
            idPrefix="event-recurrence"
            rule={current.recurrence}
            canEdit={true}
            zoneNote={m.events_zone_note({ zone: current.timezone })}
            onsave={saveRule}
          />
        {/key}
        {#if occurrence !== undefined && current.recurrence !== null}
          <div class="flex items-center gap-2 text-sm">
            <span>{m.events_occurrence_only({ date: formatDay(occurrence) })}</span>
            <Button size="sm" variant="outline" onclick={() => void skipOccurrence(occurrence)}
              >{m.events_skip_occurrence()}</Button
            >
          </div>
        {/if}
        <div class="flex items-center gap-2 border-t pt-3">
          {#if confirmDelete}
            <span class="text-sm" role="status">{m.events_delete_confirm()}</span>
            <Button size="sm" variant="destructive" onclick={() => void remove()}
              >{m.events_delete()}</Button
            >
            <Button size="sm" variant="ghost" onclick={() => (confirmDelete = false)}
              >{m.common_cancel()}</Button
            >
          {:else}
            <Button size="sm" variant="outline" onclick={() => (confirmDelete = true)}
              >{m.events_delete_event()}</Button
            >
          {/if}
        </div>
      {:else}
        {#if current.recurrence !== null}
          <p class="text-sm text-muted-foreground">
            {m.events_recurrence_line({ rule: describeRule(current.recurrence) })}
          </p>
        {/if}
        {#if current.description !== ''}
          <p class="text-sm whitespace-pre-wrap">{current.description}</p>
        {/if}
      {/if}
    {/if}
  </Dialog.Content>
</Dialog.Root>
