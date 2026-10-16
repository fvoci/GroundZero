<script lang="ts">
  // 일정 상세 대화상자: 보기·고치기·지우기. 고치기는 기준 버전으로 충돌을 막고, 충돌하면 입력을 두고 서버 내용을
  // 보여 준다(D-53과 같은 원칙). 지우기는 한 번 더 묻는다(행은 남고 휴지통 조각에서 되살린다).
  import type { EventInput, EventView, RecurrenceRule } from '$lib/api/client';
  import { api, ApiError } from '$lib/api/client';
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
              ? '일정이 없거나 볼 권한이 없습니다.'
              : '일정을 불러오지 못했습니다.';
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
    event = await api.setEventRecurrence(event.id, rule);
    epoch += 1;
    onchanged();
  }

  async function skipOccurrence(date: string): Promise<void> {
    if (event === undefined) return;
    try {
      await api.excludeOccurrence(event.id, date);
      onchanged();
      onclose();
    } catch (e: unknown) {
      error = e instanceof ApiError ? e.message : '회차를 빼지 못했습니다.';
    }
  }

  async function remove(): Promise<void> {
    if (event === undefined) return;
    try {
      await api.deleteEvent(event.id);
      onchanged();
      onclose();
    } catch (e: unknown) {
      error = e instanceof ApiError ? e.message : '지우지 못했습니다.';
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
      <Dialog.Title>{event?.title ?? '일정'}</Dialog.Title>
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
            그사이 다른 사람이 이 일정을 바꿨습니다(지금: “{c.current.title}”, {describeWhen(
              c.current,
              zone,
            )}). 내 입력은 그대로 있습니다.
          </p>
          <div class="mt-2 flex gap-2">
            <Button
              size="sm"
              variant="outline"
              onclick={() => {
                event = c.current;
                conflict = undefined;
                epoch += 1;
              }}>서버 내용 불러오기</Button
            >
            <Button size="sm" onclick={() => void save(c.input, c.current.version)}
              >내 입력으로 다시 저장</Button
            >
          </div>
        </div>
      {/if}
      {#if canEdit}
        {#key epoch}
          <EventForm
            idPrefix="event-edit"
            initial={current}
            submitLabel="저장"
            onsubmit={(input: EventInput) => save(input, current.version)}
          />
        {/key}
        {#key epoch}
          <RecurrenceEditor
            idPrefix="event-recurrence"
            rule={current.recurrence}
            canEdit={true}
            zoneNote={`${current.timezone} 기준`}
            onsave={saveRule}
          />
        {/key}
        {#if occurrence !== undefined && current.recurrence !== null}
          <div class="flex items-center gap-2 text-sm">
            <span>{occurrence} 회차만</span>
            <Button size="sm" variant="outline" onclick={() => void skipOccurrence(occurrence)}
              >이 회차 빼기</Button
            >
          </div>
        {/if}
        <div class="flex items-center gap-2 border-t pt-3">
          {#if confirmDelete}
            <span class="text-sm" role="status">이 일정을 지울까요?</span>
            <Button size="sm" variant="destructive" onclick={() => void remove()}>지우기</Button>
            <Button size="sm" variant="ghost" onclick={() => (confirmDelete = false)}>취소</Button>
          {:else}
            <Button size="sm" variant="outline" onclick={() => (confirmDelete = true)}
              >일정 지우기</Button
            >
          {/if}
        </div>
      {:else}
        {#if current.recurrence !== null}
          <p class="text-sm text-muted-foreground">반복: {describeRule(current.recurrence)}</p>
        {/if}
        {#if current.description !== ''}
          <p class="text-sm whitespace-pre-wrap">{current.description}</p>
        {/if}
      {/if}
    {/if}
  </Dialog.Content>
</Dialog.Root>
