<script lang="ts">
  import type { EventInput, EventView } from './model';
  import { eventsApi, RuleConflict } from './api';
  import { getContext, onDestroy, onMount, tick, untrack } from 'svelte';
  import {
    accountSafety,
    accountSwitching,
    notifyDone,
  } from '$lib/features/feedback/feedback.svelte';
  import type { RecurrenceRule } from '$lib/api/client';
  import { ApiError } from '$lib/api/client';
  import { errorText, untilBeforeStart } from '$lib/api/errors';
  import { formatDay } from '$lib/display';
  import DiscardChangesConfirm from '$lib/features/common/DiscardChangesConfirm.svelte';
  import { createLeaveGuard, leavingWithDrafts } from '$lib/features/common/leave-guard.svelte';
  import { focusConfirm, focusConfirmStart } from '$lib/features/common/inline-confirm';
  import FieldConflict from '$lib/features/common/FieldConflict.svelte';
  import {
    fieldDifferences,
    initialFieldChoices,
    selectedFields,
  } from '$lib/features/common/field-conflict';
  import {
    DiscardedTaskRead,
    readTaskSessionGeneration,
    sameContext,
    startCapturedRead,
    type ContextCapture,
  } from '$lib/features/tasks/context-read';
  import {
    TASK_SESSION_RECOVERY,
    readTaskWithRecovery,
    sameTaskRequest,
    type TaskSessionRecovery,
  } from '$lib/features/tasks/session-recovery';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';
  import * as Dialog from '$lib/ui/dialog';
  import { EVENT_UNITS, eventConflictFields, inputOf, sameRule, sameUnit } from './conflict';
  import type { EventComparison, EventRawDraft } from './input-drafts';
  import EventForm from './EventForm.svelte';
  import RecurrenceEditor from './RecurrenceEditor.svelte';
  import { describeRule, describeWhen, KIND_LABEL } from './labels';
  import { localTimeZone } from './time';
  import { me } from '$lib/me.svelte';
  import { router } from '$lib/router.svelte';

  interface Props {
    eventId: string | undefined;
    onclose: () => void;
    onchanged: () => void;
    returnFocus?: (() => HTMLElement | null) | undefined;
    occurrence?: string | undefined;
    locked?: boolean;
  }
  const { eventId, onclose, onchanged, returnFocus, occurrence, locked = false }: Props = $props();
  const recovery = getContext<TaskSessionRecovery | undefined>(TASK_SESSION_RECOVERY);
  let activeId = $state(untrack(() => eventId));
  let readUser = $state(untrack(() => me.userId));
  let event = $state<EventView>();
  let error = $state<string>();
  let comparison = $state<EventComparison>();
  let ruleComparing = $state(false);
  const comparing = $derived(comparison?.open === true || ruleComparing);
  const conflictFields = $derived(eventConflictFields());
  let editable = $state(false);
  let permissionBlocked = $state(false);
  let hiddenForAccess = $state(false);
  let accountBlocked = $state(false);
  const hiddenForAccount = $derived(accountBlocked || readUser !== me.userId);
  let readAttempt = $state(0);
  let retryRead = $state(false);
  let confirmDelete = $state(false);
  let epoch = $state(0);
  let ruleEpoch = $state(0);
  let pending = $state(0);
  const busy = $derived(pending > 0);
  let shown = 0;
  let mounted = true;
  onDestroy(() => {
    mounted = false;
    shown += 1;
  });
  let formVersion = 0;
  let formBase = $state.raw<EventView>();
  let formWrap = $state<HTMLFieldSetElement>();
  let heading = $state<HTMLElement | null>(null);
  let selectedSubmit = false;
  let unsaved = $state(false);
  let ruleUnsaved = $state(false);
  let formKept = $state(true);
  let ruleKept = $state(true);
  let formEditor = $state<{
    discardDraft(): void;
    snapshot(): EventRawDraft;
    restoreInput(value: EventRawDraft): void;
    applyInput(input: EventInput, reviewed: EventView): void;
  }>();
  let ruleEditor = $state<{ discardDraft(): void; cancelComparison?(): void }>();
  const leaveGuard = createLeaveGuard({
    dirty: () => !accountSwitching() && (unsaved || ruleUnsaved || comparison !== undefined),
    busy: () => busy,
    identity: () => shown,
  });
  onMount(() =>
    leaveGuard.attach({
      beforeUnload: () =>
        (unsaved || ruleUnsaved || comparison !== undefined) &&
        (!leavingWithDrafts() || !formKept || !ruleKept),
    }),
  );
  const confirmClose = $derived(leaveGuard.open);
  let keepFocus: HTMLElement | null = null;
  let content = $state<HTMLElement | null>(null);
  const zone = localTimeZone();
  let untilError = $state<string>();
  const canEdit = $derived(
    editable &&
      event !== undefined &&
      event.access !== 'read' &&
      !locked &&
      !permissionBlocked &&
      !hiddenForAccess &&
      !hiddenForAccount &&
      !accountSafety.switching &&
      !accountSafety.restoring,
  );

  const context = (): ContextCapture => ({
    userId: me.userId,
    sequence: shown,
    path: router.path,
    navigationGeneration: router.navigationGeneration,
    sessionGeneration: readTaskSessionGeneration(),
    safetyGeneration: accountSafety.generation,
  });
  function owns(captured: ContextCapture): boolean {
    return (
      mounted &&
      readUser === me.userId &&
      !hiddenForAccount &&
      !accountSafety.switching &&
      !accountSafety.restoring &&
      sameContext(captured, context())
    );
  }
  $effect(() => {
    const id = eventId;
    untrack(() => {
      if (id !== activeId) {
        leaveGuard.keep();
        leaveGuard.request(() => {
          activeId = id;
        });
      }
    });
  });
  $effect(() => {
    const id = activeId,
      user = me.userId;
    const attempt = readAttempt;
    shown += 1;
    pending = 0;
    readUser = user;
    event = undefined;
    formBase = undefined;
    comparison = undefined;
    ruleComparing = false;
    error = undefined;
    untilError = undefined;
    permissionBlocked = false;
    hiddenForAccess = false;
    accountBlocked = false;
    editable = false;
    retryRead = false;
    confirmDelete = false;
    unsaved = false;
    ruleUnsaved = false;
    leaveGuard.keep();
    if (id === undefined) return;
    let cancelled = false;
    const captured = untrack(context);
    const active = () =>
      !cancelled &&
      readAttempt === attempt &&
      mounted &&
      sameTaskRequest(captured, context()) &&
      !accountSafety.switching &&
      !accountSafety.restoring &&
      readUser === user;
    void readTaskWithRecovery(
      captured,
      context,
      active,
      recovery,
      () => eventsApi.event(id, user),
      undefined,
      {
        loaded: (value, accepted) => {
          if (!owns(accepted)) return;
          event = value;
          formBase = value;
          formVersion = value.version;
          permissionBlocked = value.archived || value.access === 'read';
          editable = !value.archived && value.access !== 'read' && !locked;
        },
        failed: (cause, accepted) => {
          if (!owns(accepted)) return;
          error =
            cause instanceof ApiError && cause.status === 404
              ? m.events_not_found()
              : errorText(cause, m.events_event_load_failed());
          retryRead = true;
        },
      },
    ).catch((cause: unknown) => {
      if (
        !cancelled &&
        sameTaskRequest(captured, context()) &&
        cause instanceof ApiError &&
        cause.code === 'account_changed'
      ) {
        accountBlocked = true;
        error = m.common_error_account_changed();
        retryRead = true;
      } else if (active() && cause instanceof DiscardedTaskRead) {
        error = m.events_event_load_failed();
        retryRead = true;
      }
    });
    return () => {
      cancelled = true;
    };
  });

  async function sending<T>(request: Promise<T>, captured: ContextCapture): Promise<T> {
    pending += 1;
    try {
      return await request;
    } catch (cause: unknown) {
      if (owns(captured) && untilBeforeStart(cause)) untilError = m.events_until_before_start();
      throw cause;
    } finally {
      if (captured.sequence === shown) pending -= 1;
    }
  }
  function blockDenied(cause: unknown): void {
    if (!(cause instanceof ApiError)) return;
    if (cause.code === 'account_changed') accountBlocked = true;
    if ([403, 404].includes(cause.status)) permissionBlocked = true;
    if (cause.status === 404) hiddenForAccess = true;
  }
  function blockChangedAccount(cause: unknown, captured: ContextCapture): void {
    if (
      cause instanceof ApiError &&
      cause.code === 'account_changed' &&
      mounted &&
      readUser === captured.userId &&
      sameTaskRequest(captured, context())
    ) {
      accountBlocked = true;
      error = m.common_error_account_changed();
    }
  }
  async function failedWrite(cause: unknown, captured: ContextCapture): Promise<void> {
    blockChangedAccount(cause, captured);
    if (!owns(captured)) return;
    blockDenied(cause);
    error = untilBeforeStart(cause)
      ? comparison?.open === true
        ? m.events_until_before_start()
        : undefined
      : errorText(cause, m.events_save_failed());
    if (
      !(cause instanceof ApiError) ||
      cause.status !== 409 ||
      cause.code !== 'invalid_state' ||
      event === undefined
    )
      return;
    try {
      const fresh = await startCapturedRead(
        captured,
        context,
        eventsApi.event(event.id, readUser).catch((failure: unknown) => {
          // The viewer mismatch changes the safety epoch before the captured read discards it.
          blockChangedAccount(failure, captured);
          throw failure;
        }),
      ).take();
      if (!owns(captured)) return;
      permissionBlocked = fresh.archived || fresh.access === 'read';
    } catch (failure: unknown) {
      if (owns(captured)) blockDenied(failure);
    }
  }
  async function focusLater(target: () => HTMLElement | null | undefined): Promise<void> {
    const captured = context();
    await tick();
    requestAnimationFrame(() => {
      if (owns(captured)) target()?.focus();
    });
  }
  function openComparison(current: EventView, input: EventInput, raw: EventRawDraft): void {
    if (formBase === undefined) return;
    const old = comparison;
    comparison = {
      base: $state.snapshot(formBase),
      baseVersion: formVersion,
      mine: $state.snapshot(input),
      server: current,
      mineDraft: raw,
      original: old?.original ?? raw,
      originalVersion: old?.originalVersion ?? formVersion,
      choices: initialFieldChoices(fieldDifferences(conflictFields, formBase, input, current)),
      open: true,
      repeated: old !== undefined,
    };
    confirmDelete = false;
    if (current.access === 'read') permissionBlocked = true;
  }
  async function save(input: EventInput, baseVersion: number, selected: boolean): Promise<boolean> {
    if (
      event === undefined ||
      formBase === undefined ||
      formEditor === undefined ||
      busy ||
      !canEdit ||
      ruleComparing
    )
      return false;
    const captured = context(),
      raw = formEditor.snapshot();
    untilError = undefined;
    error = undefined;
    const reviewed = comparison?.server;
    // A comparison opened in this session never trails the loaded event: a 409 leaves `event`
    // older and rule saves are blocked while it is open. Only a draft restored after another
    // writer moved on does, so compare against the newer read instead of reinstating the
    // stale snapshot or lowering the base version (CR-305-1, D-94).
    if (selected && reviewed !== undefined && event.version > reviewed.version) {
      openComparison($state.snapshot(event), input, raw);
      return false;
    }
    if (
      selected &&
      reviewed !== undefined &&
      EVENT_UNITS.every((key) => sameUnit(key, input, reviewed))
    ) {
      if (!ruleUnsaved && !sameRule(reviewed.recurrence, event.recurrence)) ruleEpoch += 1;
      event = reviewed;
      formBase = event;
      formVersion = event.version;
      onchanged();
      return true;
    }
    let result: Awaited<ReturnType<typeof eventsApi.updateEvent>>;
    try {
      result = await sending(
        eventsApi.updateEvent(event.id, baseVersion, input, readUser),
        captured,
      );
    } catch (cause: unknown) {
      await failedWrite(cause, captured);
      return false;
    }
    if (!owns(captured)) return false;
    if (result.kind === 'conflict') {
      openComparison(result.current, input, raw);
      return false;
    }
    if (!ruleUnsaved && !sameRule(result.event.recurrence, event.recurrence)) ruleEpoch += 1;
    event = result.event;
    formBase = result.event;
    formVersion = result.event.version;
    onchanged();
    return true;
  }
  function submitted(input: EventInput): Promise<boolean> {
    return save(input, formVersion, selectedSubmit);
  }
  async function applyChoices(saveNow: boolean): Promise<void> {
    const ask = comparison;
    if (ask === undefined || !ask.open || busy || !canEdit || formEditor === undefined) return;
    const candidate = selectedFields(
      conflictFields,
      $state.snapshot(ask.mine),
      inputOf($state.snapshot(ask.server)),
      ask.choices,
    );
    formEditor.applyInput(candidate, $state.snapshot(ask.server));
    formBase = ask.server;
    formVersion = ask.server.version;
    if (!saveNow) {
      ask.open = false;
      await focusLater(() => document.getElementById('event-edit-title'));
      return;
    }
    const form = formWrap?.querySelector('form');
    if (!(form instanceof HTMLFormElement)) return;
    selectedSubmit = true;
    try {
      form.requestSubmit();
    } finally {
      selectedSubmit = false;
    }
  }
  function leaveComparison(original = false): void {
    const ask = comparison;
    if (ask === undefined || !ask.open || busy) return;
    const raw = original ? ask.original : ask.mineDraft;
    const baseVersion = original ? ask.originalVersion : ask.baseVersion;
    const base = original ? raw.base : ask.base;
    formEditor?.restoreInput({
      ...$state.snapshot(raw),
      base: base === null ? null : { ...$state.snapshot(base), version: baseVersion },
    });
    formBase = base ?? undefined;
    formVersion = baseVersion;
    ask.open = false;
    void focusLater(() => document.getElementById('event-edit-title'));
  }
  function restoredComparison(value: EventComparison | undefined): void {
    comparison = value;
  }
  function savedForm(): void {
    comparison = undefined;
    epoch += 1;
    notifyDone(m.common_saved());
    void focusLater(() => heading);
  }

  async function saveRule(
    rule: RecurrenceRule | null,
    baseRule?: RecurrenceRule | null,
  ): Promise<void> {
    if (event === undefined || busy || !canEdit || comparison?.open === true)
      throw new Error(m.events_save_failed());
    const captured = context(),
      before = event.version;
    untilError = undefined;
    try {
      const saved = await sending(
        eventsApi.setEventRecurrence(
          event.id,
          rule,
          baseRule === undefined ? event.recurrence : baseRule,
          readUser,
        ),
        captured,
      );
      if (!owns(captured)) throw new DiscardedTaskRead();
      if (formVersion === before && saved.version === before + 1) formVersion = saved.version;
      event = saved;
      onchanged();
    } catch (cause: unknown) {
      if (owns(captured) && cause instanceof RuleConflict) event = cause.current;
      else await failedWrite(cause, captured);
      throw cause;
    }
  }
  async function skipOccurrence(date: string): Promise<void> {
    if (event === undefined || busy || !canEdit) return;
    const captured = context();
    try {
      await sending(eventsApi.excludeOccurrence(event.id, date, readUser), captured);
      if (owns(captured)) {
        onchanged();
        onclose();
      }
    } catch (cause: unknown) {
      await failedWrite(cause, captured);
    }
  }
  async function remove(): Promise<void> {
    if (event === undefined || busy || !canEdit) return;
    const captured = context();
    try {
      await sending(eventsApi.deleteEvent(event.id, readUser), captured);
      if (owns(captured)) {
        onchanged();
        onclose();
      }
    } catch (cause: unknown) {
      await failedWrite(cause, captured);
    }
  }
  function close(): void {
    if (busy || confirmClose) return;
    const active = document.activeElement;
    keepFocus = active instanceof HTMLElement && content?.contains(active) ? active : heading;
    leaveGuard.request(onclose);
  }
</script>

<!-- 닫기를 대화상자가 정하지 않고 close()가 정한다(고친 입력이 있으면 열어 둔 채 묻는다). -->
<Dialog.Root
  bind:open={
    () => activeId !== undefined,
    (o: boolean) => {
      if (!o) close();
    }
  }
>
  <Dialog.Content
    bind:ref={content}
    class="max-h-[90vh] overflow-y-auto sm:max-w-2xl"
    escapeKeydownBehavior={busy || confirmClose ? 'ignore' : 'close'}
    interactOutsideBehavior={busy || confirmClose ? 'ignore' : 'close'}
    showCloseButton={!busy}
    onEscapeKeydown={(e: KeyboardEvent) => {
      if (!comparing) return;
      e.preventDefault();
      if (comparison?.open === true) leaveComparison();
      else ruleEditor?.cancelComparison?.();
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
        >{!hiddenForAccount && !hiddenForAccess
          ? (event?.title ?? m.events_event())
          : m.events_event()}</Dialog.Title
      >
      <Dialog.Description>
        {#if event !== undefined && !hiddenForAccount && !hiddenForAccess}{KIND_LABEL[event.kind]} · {describeWhen(
            event,
            zone,
          )}{/if}
      </Dialog.Description>
    </Dialog.Header>
    {#if error !== undefined}
      <p role="alert" class="text-sm text-destructive">{error}</p>
    {/if}
    {#if retryRead}<Button
        variant="outline"
        onclick={() => {
          readAttempt += 1;
        }}>{m.common_retry()}</Button
      >{/if}
    <div
      hidden={hiddenForAccount || hiddenForAccess}
      inert={hiddenForAccount || hiddenForAccess}
      class="flex min-w-0 flex-col gap-4"
    >
      {#if event !== undefined}
        {@const current = event}
        {#if comparison?.open}
          <FieldConflict
            fields={conflictFields}
            base={inputOf(comparison.base)}
            mine={comparison.mine}
            server={inputOf(comparison.server)}
            bind:choices={comparison.choices}
            {busy}
            blocked={!canEdit}
            repeated={comparison.repeated}
            onsave={() => void applyChoices(true)}
            onedit={() => void applyChoices(false)}
            oncancel={() => {
              leaveComparison();
            }}
            onoriginal={JSON.stringify(comparison.original) !== JSON.stringify(comparison.mineDraft)
              ? () => {
                  leaveComparison(true);
                }
              : undefined}
          />
        {/if}
        {#if editable}
          <!-- 입력 양식을 감싼다(다시 저장이 이 안의 양식을 제출한다). 입력은 그것을 만든 일정에서 시작한다(반복 규칙
             저장으로 `event`가 바뀌어도 손대지 않은 시각은 그 일정의 값으로 보낸다). 요청이 가는 동안은 일정 입력과 반복
             규칙 입력을 함께 잠근다: 저장되면 양식을 저장된 일정으로 다시 그리므로 그사이 친 글이 말없이 사라졌고, 일정
             저장과 반복 규칙 저장이 겹치면 늦게 온 쪽이 더 옛 일정을 다시 놓았다(CodeRabbit). -->
          <fieldset
            class="min-w-0"
            disabled={busy || !canEdit}
            inert={comparing}
            bind:this={formWrap}
          >
            {#key epoch}
              <EventForm
                bind:this={formEditor}
                idPrefix="event-edit"
                draftName={`event:${current.id}`}
                attachGuard={false}
                disabled={busy || !canEdit}
                {comparison}
                onrestorecomparison={restoredComparison}
                onrestorebase={(base: EventView) => {
                  formBase = base;
                  formVersion = base.version;
                }}
                ondraftkept={(value: boolean) => {
                  formKept = value;
                }}
                onsaved={savedForm}
                initial={formBase ?? current}
                submitLabel={m.common_save()}
                onsubmit={submitted}
                onunsaved={(value: boolean) => {
                  unsaved = value;
                }}
              />
            {/key}
          </fieldset>
          <fieldset class="min-w-0" disabled={busy || !canEdit} inert={comparison?.open === true}>
            {#key ruleEpoch}
              <RecurrenceEditor
                bind:this={ruleEditor}
                idPrefix="event-recurrence"
                disabled={busy || !canEdit}
                draftName={`event-recurrence:${current.id}`}
                onunsaved={(value: boolean) => {
                  ruleUnsaved = value;
                }}
                ondraftkept={(value: boolean) => {
                  ruleKept = value;
                }}
                rule={current.recurrence}
                {canEdit}
                conflictRule={(cause: unknown) =>
                  cause instanceof RuleConflict ? { rule: cause.current.recurrence } : undefined}
                oncomparison={(value: boolean) => {
                  ruleComparing = value;
                }}
                zoneNote={m.events_zone_note({ zone: current.timezone })}
                onsave={saveRule}
                {untilError}
              />
            {/key}
          </fieldset>
          {#if occurrence !== undefined && current.recurrence !== null}
            <div class="flex min-w-0 flex-wrap items-center gap-2 text-sm" inert={comparing}>
              <span>{m.events_occurrence_only({ date: formatDay(occurrence) })}</span>
              <Button
                size="sm"
                variant="outline"
                disabled={busy || !canEdit}
                onclick={() => void skipOccurrence(occurrence)}>{m.events_skip_occurrence()}</Button
              >
            </div>
          {/if}
          <div class="flex min-w-0 flex-wrap items-center gap-2 border-t pt-3" inert={comparing}>
            {#if confirmDelete}
              <span id="event-delete-question" class="text-sm">{m.events_delete_confirm()}</span>
              <Button
                size="sm"
                disabled={busy || !canEdit}
                variant="destructive"
                data-confirm-go="event-delete"
                aria-describedby="event-delete-question"
                onclick={() => void remove()}>{m.events_delete()}</Button
              >
              <Button
                size="sm"
                disabled={busy || !canEdit}
                variant="ghost"
                onclick={() => {
                  confirmDelete = false;
                  void focusConfirmStart('event-delete');
                }}>{m.common_cancel()}</Button
              >
            {:else}
              <Button
                size="sm"
                disabled={busy || !canEdit}
                variant="outline"
                data-confirm="event-delete"
                onclick={() => {
                  confirmDelete = true;
                  void focusConfirm('event-delete');
                }}>{m.events_delete_event()}</Button
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
    </div>
    <DiscardChangesConfirm
      open={confirmClose}
      onkeep={leaveGuard.keep}
      ondiscard={() => {
        formEditor?.discardDraft();
        ruleEditor?.discardDraft();
        leaveGuard.discard();
      }}
      returnFocus={() => (keepFocus?.isConnected === true ? keepFocus : heading)}
    />
  </Dialog.Content>
</Dialog.Root>
