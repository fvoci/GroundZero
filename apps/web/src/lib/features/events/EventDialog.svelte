<script lang="ts">
  // 일정 상세 대화상자: 보기·고치기·지우기. 고치기는 기준 버전으로 충돌을 막고, 충돌하면 입력을 두고 서버 내용을
  // 보여 준다(D-53과 같은 원칙). 지우기는 한 번 더 묻는다(행은 남고 휴지통 조각에서 되살린다).
  import type { EventInput, EventView, RecurrenceRule } from '$lib/api/client';
  import { api, ApiError } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
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
  const zone = localTimeZone();

  $effect(() => {
    const id = eventId;
    event = undefined;
    error = undefined;
    conflict = undefined;
    confirmDelete = false;
    if (id === undefined) return;
    let cancelled = false;
    api
      .event(id)
      .then((e) => {
        if (!cancelled) event = e;
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

  async function save(input: EventInput, baseVersion: number): Promise<void> {
    if (event === undefined) return;
    const result = await api.updateEvent(event.id, baseVersion, input);
    if (result.kind === 'conflict') {
      conflict = { current: result.current, input };
      return;
    }
    conflict = undefined;
    event = result.event;
    epoch += 1;
    onchanged();
  }

  async function saveRule(rule: RecurrenceRule | null): Promise<void> {
    if (event === undefined) return;
    // 편집 중인 일정 입력과 반복 편집기의 상태(저장 알림·초점)를 지우지 않게 다시 그리지 않는다.
    event = await api.setEventRecurrence(event.id, rule);
    onchanged();
  }

  async function skipOccurrence(date: string): Promise<void> {
    if (event === undefined) return;
    try {
      await api.excludeOccurrence(event.id, date);
      onchanged();
      onclose();
    } catch (e: unknown) {
      error = errorText(e, m.events_skip_failed());
    }
  }

  async function remove(): Promise<void> {
    if (event === undefined) return;
    try {
      await api.deleteEvent(event.id);
      onchanged();
      onclose();
    } catch (e: unknown) {
      error = errorText(e, m.events_delete_failed());
    }
  }
</script>

<Dialog.Root
  open={eventId !== undefined}
  onOpenChange={(o: boolean) => {
    if (!o) onclose();
  }}
>
  <Dialog.Content
    class="max-h-[90vh] overflow-y-auto sm:max-w-2xl"
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
                conflict = undefined;
                epoch += 1;
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
            onsubmit={(input: EventInput) => save(input, current.version)}
          />
        {/key}
        <RecurrenceEditor
          idPrefix="event-recurrence"
          rule={current.recurrence}
          canEdit={true}
          zoneNote={m.events_zone_note({ zone: current.timezone })}
          onsave={saveRule}
        />
        {#if occurrence !== undefined && current.recurrence !== null}
          <div class="flex items-center gap-2 text-sm">
            <span>{m.events_occurrence_only({ date: occurrence })}</span>
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
