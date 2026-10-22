<script lang="ts">
  // 일정 상세 대화상자: 보기·고치기·지우기. 고치기는 기준 버전으로 충돌을 막고, 충돌하면 입력을 두고 서버 내용을
  // 보여 준다(D-53과 같은 원칙). 충돌 뒤 다시 저장하면 내가 고친 칸만 서버 최신 일정에 얹고, 같은 칸을 다른 사람도
  // 고쳤으면 양쪽 값을 보여 덮어쓸지 묻는다(D-94). 지우기는 한 번 더 묻는다(행은 남고 휴지통 조각에서 되살린다).
  import { tick } from 'svelte';

  import type { EventInput, EventView, RecurrenceRule } from '$lib/api/client';
  import { api, ApiError, RuleConflict } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import { formatDay } from '$lib/display';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';
  import * as Dialog from '$lib/ui/dialog';

  import type { EventUnit } from './conflict';
  import { mergeEventInput, sameUnit, trimLikeServer } from './conflict';
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
  let conflict = $state<{ current: EventView } | undefined>(undefined);
  // 충돌 뒤 다시 저장하려는데 같은 칸을 양쪽이 서로 다른 값으로 고쳤다(D-94). 물은 때의 서버 일정·내 입력·겹친 칸.
  let clash = $state<{ current: EventView; mine: EventInput; units: EventUnit[] } | undefined>(
    undefined,
  );
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
  // 일정 입력을 만든 일정(D-94). 충돌 뒤 다시 저장할 때 내가 무엇을 고쳤는지 이것과 비교한다. 입력을 다시 그릴 때만
  // 바꾼다(반복 규칙 저장은 입력을 다시 그리지 않으므로 그대로 둔다).
  let formBase = $state.raw<EventView | undefined>(undefined);
  // 일정 입력을 감싼 요소. 다시 저장은 이 안의 입력 양식을 제출해 지금 입력(알림 뒤에 고친 것 포함)을 받는다.
  let formWrap = $state<HTMLDivElement | undefined>(undefined);
  let clashBox = $state<HTMLDivElement | undefined>(undefined);
  let saveMineButton = $state<HTMLButtonElement | null>(null);
  let heading = $state<HTMLElement | null>(null);
  // 지금 입력 양식 제출이 충돌 알림의 단추에서 시작됐는지. 양식은 제출 이벤트 안에서 곧바로(첫 await 전에) onsubmit을
  // 부르므로 requestSubmit() 동안만 둔다.
  let submitMode: 'merge' | 'overwrite' | undefined;
  const zone = localTimeZone();

  $effect(() => {
    const id = eventId;
    shown += 1;
    event = undefined;
    formBase = undefined;
    error = undefined;
    conflict = undefined;
    clash = undefined;
    confirmDelete = false;
    if (id === undefined) return;
    let cancelled = false;
    api
      .event(id)
      .then((e) => {
        if (cancelled) return;
        event = e;
        formBase = e;
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
  // 충돌 알림이 보이고 다시 저장이 합칠 서버 일정(D-94). 알림 뒤 반복 규칙을 저장했으면 `event`가 더 새 판이다(옛 판에
  // 합치면 보내자마자 다시 409이고, "서버 내용 불러오기"가 방금 저장한 규칙을 되돌린다).
  const latest = $derived(
    conflict === undefined
      ? undefined
      : event !== undefined && event.version > conflict.current.version
        ? event
        : conflict.current,
  );

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
      // 다시 저장하는 사이 또 바뀌었으면 알림이 더 새 일정을 보인다. 입력과 그 기준(`formBase`)은 그대로 두어 다음
      // 다시 저장이 가장 새 일정에 합치게 한다(D-94).
      conflict = { current: result.current };
      clash = undefined;
      return;
    }
    conflict = undefined;
    clash = undefined;
    // 보통 저장은 기준 버전이 맞아야 되므로 규칙이 그대로다. 충돌 뒤 "내 내용으로 저장"이면 저장된 일정에 다른 사람이
    // 바꾼 규칙이 들어 있으니 반복 편집기도 그 규칙으로 다시 만든다(요약만 바뀌고 칸은 옛 규칙이면 옛 규칙을 다시 보냄).
    if (!sameRule(result.event.recurrence, event.recurrence)) ruleEpoch += 1;
    event = result.event;
    formBase = result.event;
    formVersion = result.event.version;
    epoch += 1;
    onchanged();
  }

  /**
   * 충돌 알림의 단추로 일정 입력 양식을 제출한다. 양식 자신의 확인(필수 칸·없는 시각·시간대)을 거친 지금 입력이
   * `submitted`로 온다. 확인에 걸리면 제출되지 않고 아무것도 보내지 않는다.
   */
  function submitForm(mode: 'merge' | 'overwrite'): void {
    const form = formWrap?.querySelector('form');
    if (!(form instanceof HTMLFormElement)) return;
    submitMode = mode;
    try {
      form.requestSubmit();
    } finally {
      submitMode = undefined;
    }
  }

  /** 일정 입력 양식의 제출. 충돌 알림의 단추에서 시작됐으면 서버 최신 일정에 합쳐 보낸다. */
  function submitted(input: EventInput): Promise<void> {
    const mode = submitMode;
    submitMode = undefined;
    if (mode !== undefined && latest !== undefined && formBase !== undefined)
      return saveMine(input, formBase, latest, mode === 'overwrite');
    return save(input, formVersion);
  }

  /**
   * 누른 단추가 사라진 뒤 초점을 옮긴다. 대화상자의 초점 가두기가 먼저 첫 칸으로 옮기므로 그 뒤 프레임에서
   * 옮긴다(TaskDetail과 같음). 그사이 다른 일정을 열었으면 옮기지 않는다.
   */
  async function focusLater(target: () => HTMLElement | null | undefined): Promise<void> {
    const mine = shown;
    await tick();
    requestAnimationFrame(() => {
      if (mine === shown) target()?.focus();
    });
  }

  /**
   * "내 입력으로 다시 저장"(D-94): 입력을 만든 일정(`base`)과 비교해 내가 고친 칸만 서버 최신 일정에 얹어 최신 판
   * 번호로 보낸다(그사이 또 바뀌었으면 서버가 다시 409). 같은 칸을 서버도 다른 값으로 바꿨으면 보내지 않고 양쪽 값을
   * 보여 덮어쓸지 묻는다. `overwrite`는 물은 뒤 "덮어쓰고 저장"을 누른 경우로, 물은 칸과 그때 보인 양쪽 값이 그대로일
   * 때만 덮어쓴다(그사이 입력을 고쳐 겹친 칸이나 값이 달라졌으면 다시 묻는다).
   */
  async function saveMine(
    input: EventInput,
    base: EventView,
    current: EventView,
    overwrite: boolean,
  ): Promise<void> {
    const mine = shown;
    const merged = mergeEventInput(base, input, current);
    const asked = clash;
    const confirmed =
      overwrite &&
      asked !== undefined &&
      merged.clashes.every(
        (u) =>
          asked.units.includes(u) &&
          sameUnit(u, current, asked.current) &&
          sameUnit(u, input, asked.mine),
      );
    if (merged.clashes.length > 0 && !confirmed) {
      clash = { current, mine: input, units: merged.clashes };
      await tick();
      if (mine === shown) clashBox?.focus();
      return;
    }
    await save(merged.input, current.version);
    if (mine !== shown) return;
    // 또 바뀌어 다시 409면 덮어쓰기 단추가 사라졌으니 새 알림의 다시 저장 단추로, 저장됐으면 알림이 사라지니
    // 머리말(저장된 제목)로 초점을 옮긴다.
    await focusLater(() => (conflict !== undefined ? saveMineButton : heading));
  }

  /** 덮어쓰지 않는다: 아무것도 보내지 않고 알림과 입력을 그대로 둔다. */
  async function cancelClash(): Promise<void> {
    clash = undefined;
    await tick();
    saveMineButton?.focus();
  }

  /** 겹친 칸의 이름(입력 칸의 이름을 쓴다). */
  function unitLabel(unit: EventUnit): string {
    switch (unit) {
      case 'kind':
        return m.events_kind();
      case 'title':
        return m.events_title();
      case 'description':
        return m.events_description();
      case 'when':
        return m.events_clash_when();
    }
  }

  /** 겹친 칸의 값. 일시는 머리말과 같은 글로, 공백만인 설명은 서버가 저장할 대로 비었다고 보인다. */
  function unitText(unit: EventUnit, e: EventView): string {
    switch (unit) {
      case 'kind':
        return KIND_LABEL[e.kind];
      case 'title':
        return e.title;
      case 'description':
        return trimLikeServer(e.description) === '' ? m.events_clash_empty() : e.description;
      case 'when':
        return describeWhen(e, zone);
    }
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
      // 서버가 거절하며 함께 준 지금 규칙을 요약에 보이고 다음 저장의 기준으로 삼는다(따로 다시 읽지 않으니 다시 읽기가
      // 실패해 옛 기준으로 같은 409를 되풀이할 일이 없다). 입력한 칸은 그대로 둔다(다시 저장하면 덮어씀).
      // 일정 입력의 기준 판은 두어, 일정 저장이 다른 사람의 변경을 충돌로 드러내게 한다.
      if (e instanceof RuleConflict && mine === shown) event = e.current;
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
    onEscapeKeydown={(e: KeyboardEvent) => {
      // 덮어쓸지 묻는 중이면 Esc는 그 물음만 거둔다(취소와 같음: 보내지 않고 입력을 두며 대화상자도 닫지 않는다).
      if (clash === undefined) return;
      e.preventDefault();
      void cancelClash();
    }}
    onCloseAutoFocus={(e: Event) => {
      const target = returnFocus?.() ?? null;
      if (target !== null) {
        e.preventDefault();
        target.focus();
      }
    }}
  >
    <Dialog.Header>
      <Dialog.Title bind:ref={heading} tabindex={-1}
        >{event?.title ?? m.events_event()}</Dialog.Title
      >
      <Dialog.Description>
        {#if event !== undefined}{KIND_LABEL[event.kind]} · {describeWhen(event, zone)}{/if}
      </Dialog.Description>
    </Dialog.Header>
    {#if error !== undefined}
      <p role="alert" class="text-sm text-destructive">{error}</p>
    {/if}
    {#if event !== undefined}
      {@const current = event}
      {#if latest !== undefined}
        {@const c = latest}
        <div class="rounded-md border border-amber-500/50 p-3 text-sm">
          <div role="alert">
            <p>
              {m.events_conflict({ title: c.title, when: describeWhen(c, zone) })}
            </p>
            <div class="mt-2 flex gap-2">
              <Button
                size="sm"
                variant="outline"
                onclick={() => {
                  event = c;
                  formBase = c;
                  formVersion = c.version;
                  conflict = undefined;
                  clash = undefined;
                  epoch += 1;
                  ruleEpoch += 1;
                }}>{m.events_load_server()}</Button
              >
              <Button
                size="sm"
                bind:ref={saveMineButton}
                onclick={() => {
                  submitForm('merge');
                }}>{m.events_save_mine()}</Button
              >
            </div>
          </div>
          {#if clash !== undefined}
            {@const k = clash}
            {@const mineView = { ...k.current, ...k.mine }}
            <!-- 같은 칸을 양쪽이 고쳤다: 서버 값과 내 값을 보이고 덮어쓸지 묻는다(D-94). 취소하면 아무것도 보내지 않는다. -->
            <div
              bind:this={clashBox}
              role="alertdialog"
              aria-labelledby="event-clash-question"
              aria-describedby="event-clash-list"
              tabindex="-1"
              class="mt-3 flex flex-col gap-2 border-t border-amber-500/50 pt-3 outline-none focus-visible:ring-2 focus-visible:ring-ring"
            >
              <p id="event-clash-question" class="font-medium">{m.events_clash_question()}</p>
              <dl id="event-clash-list" class="flex flex-col gap-2">
                {#each k.units as unit (unit)}
                  <div>
                    <dt class="font-medium">{unitLabel(unit)}</dt>
                    <dd class="break-words whitespace-pre-wrap">
                      {m.events_clash_server({ value: unitText(unit, k.current) })}
                    </dd>
                    <dd class="break-words whitespace-pre-wrap">
                      {m.events_clash_mine({ value: unitText(unit, mineView) })}
                    </dd>
                  </div>
                {/each}
              </dl>
              <div class="flex gap-2">
                <Button
                  size="sm"
                  variant="destructive"
                  onclick={() => {
                    submitForm('overwrite');
                  }}>{m.events_clash_overwrite()}</Button
                >
                <Button size="sm" variant="outline" onclick={() => void cancelClash()}
                  >{m.common_cancel()}</Button
                >
              </div>
            </div>
          {/if}
        </div>
      {/if}
      {#if canEdit}
        <!-- 입력 양식을 감싼다(다시 저장이 이 안의 양식을 제출한다). 배치에는 끼지 않는다. 입력은 그것을 만든 일정에서
             시작한다(반복 규칙 저장으로 `event`가 바뀌어도 손대지 않은 시각은 그 일정의 값으로 보낸다). -->
        <div class="contents" bind:this={formWrap}>
          {#key epoch}
            <EventForm
              idPrefix="event-edit"
              initial={formBase ?? current}
              submitLabel={m.common_save()}
              onsubmit={submitted}
            />
          {/key}
        </div>
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
