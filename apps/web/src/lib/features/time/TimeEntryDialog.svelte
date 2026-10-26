<script lang="ts">
  // 시간 기록 고치기(본인만): 시작·끝(보는 사람 시간대)·멈춘 시간·종류·메모·연결 대상, 수정 이력, 지우기.
  // 기준 버전으로 충돌을 막고, 충돌하면 서버 내용을 보여 준 뒤 다시 고치게 한다.
  import { onMount, untrack } from 'svelte';
  import type { TimeEntryView, TimeKind, TimeTargetKind } from '$lib/api/client';
  import { api, parseTimeEntry, TIME_KINDS } from '$lib/api/client';
  import { createFormDraft } from '$lib/features/common/form-draft.svelte';
  import DiscardChangesConfirm from '$lib/features/common/DiscardChangesConfirm.svelte';
  import { me } from '$lib/me.svelte';
  import { errorText } from '$lib/api/errors';
  import { focusConfirm, focusConfirmStart } from '$lib/features/common/inline-confirm';
  import type { EditedWall } from '$lib/features/events/repeated';
  import { describedBy, readEditedWall, repeatedOptions } from '$lib/features/events/repeated';
  import RepeatedTimeChoice from '$lib/features/events/RepeatedTimeChoice.svelte';
  import { wallTime } from '$lib/features/events/time';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';
  import * as Dialog from '$lib/ui/dialog';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';
  import * as Select from '$lib/ui/select';

  import { durationText, TIME_KIND_LABEL } from './format';
  import TargetPicker from './TargetPicker.svelte';

  interface Props {
    entry: TimeEntryView | undefined;
    zone: string;
    onclose: () => void;
    onchanged: () => void;
  }

  const { entry, zone, onclose, onchanged }: Props = $props();

  interface Form {
    start: string;
    end: string;
    pausedMinutes: string;
    kind: TimeKind;
    memo: string;
    target: { kind: TimeTargetKind; id: string; label: string } | null;
  }

  let form = $state<Form | undefined>(undefined);
  // 고치지 않은 칸은 원래 값(초 단위)을 그대로 보낸다. 입력란은 분 단위라 다시 계산하면 길이가 바뀐다.
  let initial = $state({ start: '', end: '', pausedMinutes: '' });
  let editZone = $state(untrack(() => zone));
  let base = $state<TimeEntryView | undefined>(undefined);
  let conflict = $state<TimeEntryView | undefined>();
  let error = $state<string | undefined>(undefined);
  /** 그 시간대에 없는 시각을 넣은 칸(칸 옆에 알린다). */
  let missing = $state<'start' | 'end' | undefined>(undefined);
  // 고친 칸이 두 번 있는 시각(가을 여름시간 끝, D-93)일 때 고른 UTC 시각. 그 칸을 고치면 지운다.
  let startChoice = $state<string | undefined>(undefined);
  let endChoice = $state<string | undefined>(undefined);
  /** 두 번 있는 시각인데 고르지 않고 저장하려 한 칸(칸 옆에 알린다, D-82). */
  const unchosen = $state({ start: false, end: false });
  // 고르지 않고 저장하려 한 횟수(다시 막힐 때마다 칸 옆 알림을 새로 그린다).
  let validationAttempt = $state(0);
  let busy = $state(false);
  let confirmDelete = $state(false);
  let history = $state<
    {
      started_at: string;
      ended_at: string | null;
      kind: TimeKind;
      memo: string;
      changed_at: string;
    }[]
  >([]);

  function fill(e: TimeEntryView): void {
    base = e;
    editZone = zone;
    // 지역 값으로 만든 뒤 넣는다(효과 안에서 방금 넣은 $state를 다시 읽으면 효과가 자기 자신을 다시 부른다).
    const next: Form = {
      start: wallTime(e.started_at, zone),
      end: e.ended_at === null ? '' : wallTime(e.ended_at, zone),
      pausedMinutes: String(Math.round(e.paused_seconds / 60)),
      kind: e.kind,
      memo: e.memo,
      target:
        e.target === null
          ? null
          : {
              kind: e.target.kind,
              id: e.target.id,
              label: e.target.label ?? m.time_target_hidden(),
            },
    };
    initial = { start: next.start, end: next.end, pausedMinutes: next.pausedMinutes };
    form = next;
    // 다시 채운 칸(다른 기록, 충돌 뒤 서버 내용)에는 앞서 고른 것이 맞지 않는다.
    startChoice = undefined;
    endChoice = undefined;
    unchosen.start = false;
    unchosen.end = false;
  }

  interface EditDraft {
    form: Form;
    base: TimeEntryView;
    initial: typeof initial;
    zone: string;
    startChoice: string | undefined;
    endChoice: string | undefined;
  }
  function isDirty(): boolean {
    return (
      form !== undefined &&
      base !== undefined &&
      (form.start !== initial.start ||
        form.end !== initial.end ||
        form.pausedMinutes !== initial.pausedMinutes ||
        form.kind !== base.kind ||
        form.memo !== base.memo ||
        form.target?.id !== base.target?.id ||
        form.target?.kind !== base.target?.kind ||
        startChoice !== undefined ||
        endChoice !== undefined ||
        editZone !== zone)
    );
  }
  const draft = createFormDraft<EditDraft | undefined>({
    name: () => (entry === undefined ? undefined : `time-entry:${entry.id}`),
    snapshot: () =>
      form === undefined || base === undefined
        ? undefined
        : {
            form: { ...form, target: form.target === null ? null : { ...form.target } },
            base: { ...base, target: base.target === null ? null : { ...base.target } },
            initial: { ...initial },
            zone: editZone,
            startChoice,
            endChoice,
          },
    parse: (value) => {
      if (
        typeof value !== 'object' ||
        value === null ||
        !('form' in value) ||
        !('base' in value) ||
        !('initial' in value) ||
        !('zone' in value) ||
        typeof value.zone !== 'string'
      )
        return undefined;
      const f = value.form;
      const i = value.initial;
      if (
        typeof f !== 'object' ||
        f === null ||
        typeof i !== 'object' ||
        i === null ||
        !('start' in f) ||
        typeof f.start !== 'string' ||
        !('end' in f) ||
        typeof f.end !== 'string' ||
        !('pausedMinutes' in f) ||
        typeof f.pausedMinutes !== 'string' ||
        !('memo' in f) ||
        typeof f.memo !== 'string' ||
        !('kind' in f) ||
        !('target' in f) ||
        !('start' in i) ||
        typeof i.start !== 'string' ||
        !('end' in i) ||
        typeof i.end !== 'string' ||
        !('pausedMinutes' in i) ||
        typeof i.pausedMinutes !== 'string'
      )
        return undefined;
      const kind = TIME_KINDS.find((k) => k === f.kind);
      if (kind === undefined) return undefined;
      let target: Form['target'] = null;
      if (f.target !== null) {
        const t = f.target;
        if (
          typeof t !== 'object' ||
          !('kind' in t) ||
          !('id' in t) ||
          typeof t.id !== 'string' ||
          !('label' in t) ||
          typeof t.label !== 'string'
        )
          return undefined;
        const targetKind = (['task', 'document', 'item', 'episode', 'study_unit'] as const).find(
          (k) => k === t.kind,
        );
        if (targetKind === undefined) return undefined;
        target = { kind: targetKind, id: t.id, label: t.label };
      }
      const sc = 'startChoice' in value ? value.startChoice : undefined;
      const ec = 'endChoice' in value ? value.endChoice : undefined;
      if (
        (sc !== undefined && typeof sc !== 'string') ||
        (ec !== undefined && typeof ec !== 'string')
      )
        return undefined;
      try {
        const restoredBase = parseTimeEntry(value.base);
        if (restoredBase.id !== entry?.id) return undefined;
        return {
          form: {
            start: f.start,
            end: f.end,
            pausedMinutes: f.pausedMinutes,
            memo: f.memo,
            kind,
            target,
          },
          base: restoredBase,
          initial: { start: i.start, end: i.end, pausedMinutes: i.pausedMinutes },
          zone: value.zone,
          startChoice: sc,
          endChoice: ec,
        };
      } catch {
        return undefined;
      }
    },
    restore: (value) => {
      if (value === undefined) return;
      form = value.form;
      base = value.base;
      initial = value.initial;
      editZone = value.zone;
      startChoice = value.startChoice;
      endChoice = value.endChoice;
      conflict = undefined;
      error = undefined;
    },
    reset: () => {
      form = undefined;
      base = undefined;
      conflict = undefined;
      busy = false;
      history = [];
      error = undefined;
    },
    dirty: isDirty,
    busy: () => busy,
  });
  onMount(draft.attach);

  function applyCurrent(current: TimeEntryView): void {
    // Capture the selected version before discarding resets the reactive conflict.
    draft.leave.request(() => {
      fill(current);
    });
  }

  // 고친 칸이 두 번 있는 시각이면 고를 두 항목(고치지 않은 칸은 저장된 시점을 그대로 보내므로 묻지 않는다).
  const startOptions = $derived(
    form !== undefined && form.start !== initial.start ? repeatedOptions(form.start, editZone) : [],
  );
  const endOptions = $derived(
    form !== undefined && form.end !== '' && form.end !== initial.end
      ? repeatedOptions(form.end, editZone)
      : [],
  );
  const startInvalid = $derived(unchosen.start && startOptions.length === 2);
  const endInvalid = $derived(unchosen.end && endOptions.length === 2);

  let loadedEntry: TimeEntryView | undefined;
  let loadedOwner: string | undefined;
  $effect(() => {
    const e = entry;
    const userId = me.userId;
    if (e !== undefined && loadedEntry?.id === e.id && loadedOwner === userId && untrack(isDirty))
      return;
    // 다이얼로그는 늘 마운트돼 있다(닫으면 entry만 비운다). 앞 기록의 오류가 다음에 연 기록에 남지 않게 지운다.
    error = undefined;
    missing = undefined;
    confirmDelete = false;
    history = [];
    conflict = undefined;
    if (e === undefined) {
      loadedEntry = undefined;
      return;
    }
    // The parent still holds the previous account's entry until it reloads its list.
    if (loadedEntry === e && loadedOwner !== userId) return;
    loadedEntry = e;
    loadedOwner = userId;
    untrack(() => {
      fill(e);
    });
    let cancelled = false;
    void api
      .timeEntryRevisions(e.id)
      .then((h) => {
        if (!cancelled && me.userId === userId) history = h;
      })
      .catch(() => {
        if (!cancelled && me.userId === userId) history = [];
      });
    return () => {
      cancelled = true;
    };
  });

  async function save(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (base === undefined || form === undefined || busy || !draft.active || draft.blocked) return;
    // 앞선 저장의 오류는 먼저 지운다(없는 시각으로 일찍 돌아가도 이미 고친 오류가 남지 않게).
    error = undefined;
    missing = undefined;
    unchosen.start = false;
    unchosen.end = false;
    const paused = Number(form.pausedMinutes.trim() === '' ? '0' : form.pausedMinutes);
    if (!Number.isInteger(paused) || paused < 0) {
      error = m.time_paused_invalid();
      return;
    }
    // 고친 칸만 읽는다(D-93). 고치지 않은 칸은 저장된 시점(초 포함)을 그대로 보낸다. 고친 시각이 그 시간대에 없으면
    // (여름시간 시작일의 건너뛴 시각, D-81) 조용히 옮겨 저장하지 않고, 두 번 있으면(여름시간 끝) 어느 쪽인지 고를 때까지
    // 보내지 않는다. 둘 다 그 칸 옆에 알린다(D-82). 진행 중인 타이머는 끝이 비어 있어 시작만 본다.
    const start: EditedWall =
      form.start === initial.start
        ? { kind: 'instant', instant: base.started_at }
        : readEditedWall(form.start, editZone, startChoice);
    const end: EditedWall | null =
      form.end === ''
        ? null
        : base.ended_at !== null && form.end === initial.end
          ? { kind: 'instant', instant: base.ended_at }
          : readEditedWall(form.end, editZone, endChoice);
    if (start.kind === 'missing') {
      missing = 'start';
      return;
    }
    if (end !== null && end.kind === 'missing') {
      missing = 'end';
      return;
    }
    if (start.kind === 'unchosen' || (end !== null && end.kind === 'unchosen')) {
      unchosen.start = start.kind === 'unchosen';
      unchosen.end = end !== null && end.kind === 'unchosen';
      validationAttempt += 1;
      return;
    }
    const attempt = draft.capture();
    if (attempt === undefined) return;
    busy = true;
    try {
      const result = await api.updateTimeEntry(base.id, base.version, {
        started_at: start.instant,
        ended_at: end === null ? null : end.instant,
        // 진행 중인 타이머와 고치지 않은 멈춘 시간은 원래 초 단위 값 그대로.
        paused_seconds:
          base.ended_at === null || form.pausedMinutes === initial.pausedMinutes
            ? base.paused_seconds
            : paused * 60,
        kind: form.kind,
        memo: form.memo,
        target: form.target === null ? null : { kind: form.target.kind, id: form.target.id },
      });
      if (!draft.isCurrent(attempt)) return;
      if (result.kind === 'conflict') {
        conflict = result.current;
        error = m.time_entry_conflict();
        return;
      }
      draft.complete(attempt);
      fill(result.entry);
      onchanged();
      onclose();
    } catch (e: unknown) {
      if (draft.isCurrent(attempt)) error = errorText(e, m.time_save_failed());
    } finally {
      if (draft.isOwner(attempt)) busy = false;
    }
  }

  async function remove(): Promise<void> {
    if (base === undefined || busy) return;
    const attempt = draft.capture();
    if (attempt === undefined) return;
    busy = true;
    try {
      await api.deleteTimeEntry(base.id);
      if (!draft.isCurrent(attempt)) return;
      draft.complete(attempt);
      onchanged();
      onclose();
    } catch (e: unknown) {
      if (draft.isCurrent(attempt)) error = errorText(e, m.time_delete_failed());
    } finally {
      if (draft.isOwner(attempt)) busy = false;
    }
  }
</script>

<Dialog.Root
  bind:open={
    () => entry !== undefined,
    (o: boolean) => {
      if (!o) draft.leave.request(onclose);
    }
  }
>
  <Dialog.Content
    class="max-h-[90vh] overflow-y-auto sm:max-w-xl"
    escapeKeydownBehavior={busy || draft.leave.open ? 'ignore' : 'close'}
    interactOutsideBehavior={busy || draft.leave.open ? 'ignore' : 'close'}
  >
    <Dialog.Header>
      <Dialog.Title>{m.time_edit_title()}</Dialog.Title>
      <Dialog.Description>
        {#if base !== undefined}{m.time_edit_description({
            duration: durationText(base.seconds),
            zone: editZone,
          })}{/if}
      </Dialog.Description>
    </Dialog.Header>
    {#each draft.offers as offer (offer.key)}
      <div class="flex flex-wrap items-center gap-2" data-testid="time-entry-draft-offer">
        <p>{m.common_draft_available()}</p>
        <Button
          size="sm"
          onclick={() => {
            draft.restore(offer);
          }}>{m.common_draft_restore()}</Button
        >
        <Button
          size="sm"
          variant="outline"
          onclick={() => {
            draft.discard(offer);
          }}>{m.common_draft_discard()}</Button
        >
      </div>
    {/each}
    {#if form !== undefined && base !== undefined}
      {@const f = form}
      {@const running = base.ended_at === null}
      <form class="grid gap-3 sm:grid-cols-2" onsubmit={(e) => void save(e)}>
        <div class="flex flex-col gap-1">
          <Label for="time-edit-start">{m.time_start()}</Label>
          <Input
            id="time-edit-start"
            type="datetime-local"
            bind:value={f.start}
            required
            oninput={() => {
              missing = undefined;
              startChoice = undefined;
              unchosen.start = false;
            }}
            aria-invalid={missing === 'start' || startInvalid}
            aria-describedby={describedBy(
              missing === 'start' && 'time-edit-start-error',
              startOptions.length === 2 && 'time-edit-start-repeated-legend',
              startInvalid && 'time-edit-start-repeated-error',
            )}
          />
          {#if missing === 'start'}
            <p id="time-edit-start-error" class="text-xs text-destructive" role="alert">
              {m.events_missing_time({ zone: editZone })}
            </p>
          {/if}
          {#if startOptions.length === 2}
            <RepeatedTimeChoice
              id="time-edit-start"
              field={m.time_start()}
              zone={editZone}
              options={startOptions}
              chosen={startChoice}
              invalid={startInvalid}
              attempt={validationAttempt}
              onchoose={(instant: string) => {
                startChoice = instant;
                unchosen.start = false;
              }}
            />
          {/if}
        </div>
        <div class="flex flex-col gap-1">
          <Label for="time-edit-end">{m.time_end()}</Label>
          <Input
            id="time-edit-end"
            type="datetime-local"
            bind:value={f.end}
            disabled={running}
            required={!running}
            oninput={() => {
              missing = undefined;
              endChoice = undefined;
              unchosen.end = false;
            }}
            aria-invalid={missing === 'end' || endInvalid}
            aria-describedby={describedBy(
              missing === 'end' && 'time-edit-end-error',
              endOptions.length === 2 && 'time-edit-end-repeated-legend',
              endInvalid && 'time-edit-end-repeated-error',
            )}
          />
          {#if missing === 'end'}
            <p id="time-edit-end-error" class="text-xs text-destructive" role="alert">
              {m.events_missing_time({ zone: editZone })}
            </p>
          {/if}
          {#if endOptions.length === 2}
            <RepeatedTimeChoice
              id="time-edit-end"
              field={m.time_end()}
              zone={editZone}
              options={endOptions}
              chosen={endChoice}
              invalid={endInvalid}
              attempt={validationAttempt}
              onchoose={(instant: string) => {
                endChoice = instant;
                unchosen.end = false;
              }}
            />
          {/if}
        </div>
        <div class="flex flex-col gap-1">
          <Label for="time-edit-paused">{m.time_paused_minutes()}</Label>
          <Input
            id="time-edit-paused"
            inputmode="numeric"
            bind:value={f.pausedMinutes}
            disabled={running}
          />
        </div>
        <div class="flex flex-col gap-1">
          <span class="text-sm font-medium" id="time-edit-kind-label">{m.time_kind()}</span>
          <Select.Root
            type="single"
            value={f.kind}
            onValueChange={(v: string) => {
              const k = TIME_KINDS.find((x) => x === v);
              if (k !== undefined) f.kind = k;
            }}
          >
            <Select.Trigger aria-labelledby="time-edit-kind-label"
              >{TIME_KIND_LABEL[f.kind]}</Select.Trigger
            >
            <Select.Content>
              {#each TIME_KINDS as k (k)}
                <Select.Item value={k} label={TIME_KIND_LABEL[k]}>{TIME_KIND_LABEL[k]}</Select.Item>
              {/each}
            </Select.Content>
          </Select.Root>
        </div>
        <div class="flex flex-col gap-1 sm:col-span-2">
          <Label for="time-edit-memo">{m.time_memo()}</Label>
          <Input id="time-edit-memo" bind:value={f.memo} maxlength={2000} />
        </div>
        <div class="flex flex-col gap-2 sm:col-span-2">
          <span class="text-sm font-medium">{m.time_target()}</span>
          {#if f.target !== null}
            <div class="flex items-center gap-2 text-sm">
              <span>{f.target.label}</span>
              <Button size="sm" variant="ghost" onclick={() => (f.target = null)}
                >{m.time_unlink()}</Button
              >
            </div>
          {:else}
            <span class="text-sm text-muted-foreground">{m.time_none()}</span>
          {/if}
          <TargetPicker
            idPrefix="time-edit"
            onpick={(t: { kind: TimeTargetKind; id: string; label: string }) => {
              f.target = t;
            }}
          />
        </div>
        {#if error !== undefined}
          <p role="alert" class="text-sm text-destructive sm:col-span-2">{error}</p>
        {/if}
        {#if conflict !== undefined}
          {@const current = conflict}
          <div class="flex flex-col gap-2 rounded border p-2 sm:col-span-2">
            <p>
              {wallTime(current.started_at, editZone)} – {current.ended_at === null
                ? m.time_in_progress()
                : wallTime(current.ended_at, editZone)}
            </p>
            <p>
              {m.time_paused_minutes()}: {current.paused_seconds / 60} · {TIME_KIND_LABEL[
                current.kind
              ]}
            </p>
            <p class="whitespace-pre-wrap break-words">{current.memo}</p>
            <p>{m.time_target()}: {current.target?.label ?? m.time_none()}</p>
            <Button
              variant="outline"
              onclick={() => {
                applyCurrent(current);
              }}>{m.time_use_current()}</Button
            >
          </div>
        {/if}
        <div class="flex flex-wrap items-center gap-2 sm:col-span-2">
          <Button type="submit" disabled={busy || !draft.active || draft.blocked}
            >{m.common_save()}</Button
          >
          {#if confirmDelete}
            <span id="time-delete-question" class="text-sm">{m.time_delete_confirm()}</span>
            <Button
              size="sm"
              variant="destructive"
              data-confirm-go="time-delete"
              aria-describedby="time-delete-question"
              onclick={() => void remove()}>{m.time_delete()}</Button
            >
            <Button
              size="sm"
              variant="ghost"
              onclick={() => {
                confirmDelete = false;
                void focusConfirmStart('time-delete');
              }}>{m.common_cancel()}</Button
            >
          {:else}
            <Button
              size="sm"
              variant="outline"
              data-confirm="time-delete"
              onclick={() => {
                confirmDelete = true;
                void focusConfirm('time-delete');
              }}>{m.time_delete_entry()}</Button
            >
          {/if}
        </div>
      </form>
      {#if history.length > 0}
        <section class="flex flex-col gap-1 border-t pt-2" aria-labelledby="time-history-heading">
          <h3 id="time-history-heading" class="text-sm font-semibold">
            {m.time_history_heading()}
          </h3>
          <ul class="flex flex-col gap-1 text-sm text-muted-foreground">
            {#each history as h (h.changed_at)}
              <li>
                {m.time_history_entry({
                  changed: wallTime(h.changed_at, editZone).replace('T', ' '),
                  start: wallTime(h.started_at, editZone).replace('T', ' '),
                  end:
                    h.ended_at === null
                      ? m.time_in_progress()
                      : wallTime(h.ended_at, editZone).slice(11),
                  kind: TIME_KIND_LABEL[h.kind],
                })}{h.memo === '' ? '' : ` · ${h.memo}`}
              </li>
            {/each}
          </ul>
        </section>
      {/if}
    {/if}
  </Dialog.Content>
</Dialog.Root>

<DiscardChangesConfirm
  open={draft.leave.open}
  onkeep={draft.leave.keep}
  ondiscard={draft.discardAndLeave}
/>
