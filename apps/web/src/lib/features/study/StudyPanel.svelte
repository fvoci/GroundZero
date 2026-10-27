<script lang="ts">
  // 스터디 계획(D-36): 계획 단위(교육과정·챕터·단계, 쪽·문제·분 분량, 공통 계획일)와 내 진행, 계획 대비 진행·남은
  // 분량, 밀린 단위 재배치(내가 고른 시작일·하루 단위 수), 함께 하는 사람의 진행 집계.
  import { onMount, tick, untrack } from 'svelte';

  import type {
    EpisodeStatus,
    StudyParticipantView,
    StudyPlanView,
    StudyUnitKind,
    StudyUnitView,
  } from '$lib/api/client';
  import { api, ApiError, newRequestId, STUDY_UNIT_KINDS } from '$lib/api/client';
  import { createFormDraft } from '$lib/features/common/form-draft.svelte';
  import FormDraftOffer from '$lib/features/common/FormDraftOffer.svelte';
  import DiscardChangesConfirm from '$lib/features/common/DiscardChangesConfirm.svelte';
  import { accountSwitching } from '$lib/features/feedback/feedback.svelte';
  import { me } from '$lib/me.svelte';
  import { errorText } from '$lib/api/errors';
  import { formatDay } from '$lib/display';
  import { localTimeZone, wallTime } from '$lib/features/events/time';
  import { timer } from '$lib/features/time/timer.svelte';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';
  import { NativeSelect } from '$lib/ui/native-select';
  import { Textarea } from '$lib/ui/textarea';

  import { plannedOn, replan, studyProgress, unitAmount, unitLabel } from './progress';

  interface Props {
    itemId: string;
  }

  const { itemId }: Props = $props();

  const today = wallTime(new Date().toISOString(), localTimeZone()).slice(0, 10);

  function statusLabel(status: EpisodeStatus): string {
    switch (status) {
      case 'not_started':
        return m.study_status_not_started();
      case 'in_progress':
        return m.study_status_in_progress();
      case 'done':
        return m.study_status_done();
    }
  }

  let plan = $state<StudyPlanView | undefined>(undefined);
  let participants = $state<StudyParticipantView[]>([]);
  let error = $state<string | undefined>(undefined);
  let status = $state('');
  let timerBusy = $state(false);
  async function startUnit(id: string): Promise<void> {
    if (timerBusy) return;
    timerBusy = true;
    error = undefined;
    try {
      await timer.start('study', { kind: 'study_unit', id });
    } catch (e: unknown) {
      error = errorText(e, m.time_start_failed());
    } finally {
      timerBusy = false;
    }
  }
  let reloadKey = $state(0);
  // 숫자도 원문을 보존하고 제출할 때 검증한다.
  interface UnitDraft {
    title: string;
    chapter: string;
    unit: StudyUnitKind;
    amount: string;
    planned_on: string;
  }

  let draft = $state<UnitDraft>({
    title: '',
    chapter: '',
    unit: 'pages',
    amount: '',
    planned_on: '',
  });
  // 같은 입력의 재시도는 같은 요청 id, 고친 입력은 새 id.
  let requestId = $state(newRequestId());
  let requestPayload = $state('');
  let replanStart = $state(today);
  // 하루 단위 수도 원문을 보존하고 제출할 때 검증한다.
  let replanPerDay = $state('1');
  let replanning = $state(false);
  let openUnit = $state<string | undefined>(undefined);
  let mine = $state<{ done_amount: string; planned_on: string; note: string }>({
    done_amount: '',
    planned_on: '',
    note: '',
  });
  /** 공통 정보를 고치는 단위와, 고치기를 열 때(또는 마지막으로 확인할 때) 읽은 그 단위. */
  let editingUnit = $state<StudyUnitView | undefined>(undefined);
  /**
   * 지우기를 한 번 더 묻는 단위. 단위를 지우면 서버가 모든 참여자의 그 단위 진행과 개인 메모(본인만 읽는다)까지
   * 지우고 되돌릴 수 없어, 한 번에 지우지 않고 그 영향을 알린 뒤 지운다.
   */
  let confirmingDelete = $state<string | undefined>(undefined);
  let confirmDeleteButton = $state<HTMLButtonElement | null>(null);

  async function askDelete(id: string): Promise<void> {
    confirmingDelete = id;
    await tick();
    confirmDeleteButton?.focus();
  }

  /** 지우기를 그만둔다. 묻는 칸이 초점과 함께 사라지므로 그 단위의 지우기 단추로 초점을 돌려준다. */
  async function cancelDelete(id: string): Promise<void> {
    confirmingDelete = undefined;
    await tick();
    document.getElementById(`unit-delete-${id}`)?.focus();
  }
  let shared = $state<UnitDraft & { number: string }>({
    number: '',
    title: '',
    chapter: '',
    unit: 'pages',
    amount: '',
    planned_on: '',
  });

  /** 빈 칸은 0. 0 이상의 정수가 아니면 undefined. */
  function wholeAmount(value: string): number | undefined {
    const amount = value.trim() === '' ? 0 : Number(value);
    return Number.isInteger(amount) && amount >= 0 ? amount : undefined;
  }

  const progress = $derived(plan === undefined ? undefined : studyProgress(plan.units, today));
  let owner = $state(me.userId);
  let loadedItem = '';
  let sharedBase = $state('');
  let mineBase = $state('');
  let baseVersion = $state('');
  let adding = $state(false);
  let savingMine = $state(false);
  function textFields<T extends Record<string, string>>(raw: unknown, shape: T): T | undefined {
    const valid = (value: unknown): value is T =>
      typeof value === 'object' &&
      value !== null &&
      Object.keys(shape).every(
        (key) => key in value && typeof Reflect.get(value, key) === 'string',
      ) &&
      (!('unit' in shape) ||
        ('unit' in value && STUDY_UNIT_KINDS.some((kind) => kind === value.unit)));
    return valid(raw) ? raw : undefined;
  }
  function resetShared(): void {
    const u = editingUnit;
    shared = {
      number: u === undefined ? '' : String(u.number),
      title: u?.title ?? '',
      chapter: u?.chapter ?? '',
      unit: u?.unit ?? 'pages',
      amount: u === undefined ? '' : String(u.amount),
      planned_on: u?.planned_on ?? '',
    };
    sharedBase = JSON.stringify(shared);
    baseVersion = String(u?.version ?? 0);
  }
  function resetMine(): void {
    const u = plan?.units.find((entry) => entry.id === openUnit);
    mine = {
      done_amount: u === undefined ? '' : String(u.mine.done_amount),
      planned_on: u?.mine.planned_on ?? '',
      note: u?.mine.note ?? '',
    };
    mineBase = JSON.stringify(mine);
  }
  const newDraft = createFormDraft({
    name: () => `study:${itemId}:new`,
    snapshot: () => ({ ...draft, requestId, requestPayload }),
    parse: (raw) => textFields(raw, { ...draft, requestId: '', requestPayload: '' }),
    restore: (input) => {
      const { requestId: id, requestPayload: payload, ...fields } = input;
      draft = fields;
      requestId = id;
      requestPayload = payload;
    },
    reset: () => {
      draft = { title: '', chapter: '', unit: 'pages', amount: '', planned_on: '' };
      requestId = newRequestId();
      requestPayload = '';
    },
    dirty: () =>
      draft.title !== '' ||
      draft.chapter !== '' ||
      draft.amount !== '' ||
      draft.planned_on !== '' ||
      draft.unit !== 'pages',
    busy: () => adding,
  });
  const unitDraft = createFormDraft({
    name: () =>
      editingUnit === undefined ? undefined : `study:${itemId}:unit:${editingUnit.id}:edit`,
    snapshot: () => ({ ...shared, base: sharedBase, version: baseVersion }),
    parse: (raw) => textFields(raw, { ...shared, base: '', version: '' }),
    restore: ({ base, version, ...input }) => {
      shared = input;
      sharedBase = base;
      baseVersion = version;
    },
    reset: resetShared,
    dirty: () => editingUnit !== undefined && JSON.stringify(shared) !== sharedBase,
    busy: () => savingUnit,
  });
  const mineDraft = createFormDraft({
    name: () => (openUnit === undefined ? undefined : `study:${itemId}:unit:${openUnit}:mine`),
    snapshot: () => ({ ...mine, base: mineBase }),
    parse: (raw) => textFields(raw, { ...mine, base: '' }),
    restore: ({ base, ...input }) => {
      mine = input;
      mineBase = base;
    },
    reset: resetMine,
    dirty: () => openUnit !== undefined && JSON.stringify(mine) !== mineBase,
    busy: () => savingMine,
  });
  // 재배치 시작일·하루 단위 수는 조회가 아니라 작업 설정이라 다른 학습 폼처럼 남긴다(⑱).
  const replanDraft = createFormDraft({
    name: () => `study:${itemId}:replan`,
    snapshot: () => ({ start: replanStart, perDay: replanPerDay }),
    parse: (raw) => textFields(raw, { start: '', perDay: '' }),
    restore: (input) => {
      replanStart = input.start;
      replanPerDay = input.perDay;
    },
    reset: () => {
      replanStart = today;
      replanPerDay = '1';
    },
    dirty: () => replanStart !== today || replanPerDay !== '1',
    busy: () => replanning,
  });
  const drafts = [newDraft, unitDraft, mineDraft, replanDraft];

  async function reschedule(overdue: readonly StudyUnitView[]): Promise<void> {
    const per = Number(replanPerDay.trim());
    if (replanPerDay.trim() === '' || !Number.isInteger(per) || per < 1) {
      error = m.study_per_day_invalid();
      return;
    }
    if (replanning || !plan?.can_write) return;
    const attempt = replanDraft.capture();
    if (attempt === undefined) return;
    const start = replanStart;
    replanning = true;
    try {
      const done = await run(
        () => api.planStudyUnits(itemId, replan(overdue, start, per)),
        m.study_replanned({
          count: overdue.length,
          start: start === '' ? '' : formatDay(start),
          perDay: per,
        }),
      );
      if (done) replanDraft.complete(attempt);
    } finally {
      if (replanDraft.isOwner(attempt)) replanning = false;
    }
  }
  onMount(() => {
    const detach = drafts.map((draft) => draft.attach());
    return () => {
      detach.forEach((remove) => {
        remove();
      });
    };
  });

  $effect(() => {
    const id = itemId;
    const userId = me.userId;
    const switching = accountSwitching();
    if (reloadKey < 0) return;
    untrack(() => {
      if (owner !== userId || loadedItem !== id) {
        owner = userId;
        loadedItem = id;
        plan = undefined;
        participants = [];
        editingUnit = undefined;
        openUnit = undefined;
        savingUnit = false;
        savingMine = false;
        adding = false;
      }
    });
    let cancelled = false;
    if (switching) return;
    Promise.all([api.studyPlan(id), api.studyParticipants(id, today)])
      .then(([p, list]) => {
        if (cancelled) return;
        plan = p;
        participants = list;
      })
      .catch((e: unknown) => {
        if (!cancelled) error = errorText(e, m.study_load_failed());
      });
    return () => {
      cancelled = true;
    };
  });

  async function run(action: () => Promise<unknown>, done: string): Promise<boolean> {
    error = undefined;
    try {
      await action();
      status = done;
      reloadKey += 1;
      return true;
    } catch (e: unknown) {
      error = errorText(e, m.study_action_failed());
      return false;
    }
  }

  /** 서버의 지금 내 진도. 다른 탭·기기에서 바꾼 메모·분량·상태를 옛 화면 값으로 덮지 않게 바꾸기 직전에 읽는다. */
  async function freshMine(u: StudyUnitView): Promise<StudyUnitView['mine']> {
    const now = await api.studyPlan(itemId);
    return now.units.find((x) => x.id === u.id)?.mine ?? u.mine;
  }

  function setStatus(u: StudyUnitView, next: EpisodeStatus): void {
    if (!plan?.can_write) return;
    void run(
      async () => {
        const fresh = await freshMine(u);
        await api.setUnitProgress(u.id, {
          status: next,
          done_amount: next === 'done' ? u.amount : fresh.done_amount,
          planned_on: fresh.planned_on,
          note: fresh.note,
          today,
        });
      },
      m.study_unit_status({ number: u.number, title: u.title, status: statusLabel(next) }),
    );
  }

  function openMine(u: StudyUnitView): void {
    mineDraft.leave.request(() => {
      selectMine(u);
    });
  }
  function selectMine(u: StudyUnitView): void {
    if (openUnit === u.id) {
      openUnit = undefined;
      return;
    }
    openUnit = u.id;
    mine = {
      done_amount: String(u.mine.done_amount),
      planned_on: u.mine.planned_on ?? '',
      note: u.mine.note,
    };
    mineBase = JSON.stringify(mine);
  }

  function editUnit(u: StudyUnitView): void {
    unitDraft.leave.request(() => {
      selectEditUnit(u);
    });
  }
  function selectEditUnit(u: StudyUnitView): void {
    if (editingUnit?.id === u.id) {
      editingUnit = undefined;
      return;
    }
    editingUnit = u;
    shared = {
      number: String(u.number),
      title: u.title,
      chapter: u.chapter,
      unit: u.unit,
      amount: String(u.amount),
      planned_on: u.planned_on ?? '',
    };
    sharedBase = JSON.stringify(shared);
    baseVersion = String(u.version);
  }

  /**
   * 단위 공통 정보 저장. 읽은 판을 함께 보내 서버가 그 판일 때만 바꾼다(확인과 바꾸기가 한 문장). 고치기를 연 뒤 다른
   * 사람이 먼저 고쳤으면 409: 입력은 그대로 두고 기준만 지금 판으로 바꿔 알린다. 다시 저장하면(명시적으로) 바꾼다.
   */
  let savingUnit = $state(false);

  async function saveUnit(base: StudyUnitView): Promise<void> {
    const amount = wholeAmount(shared.amount);
    const number = Number(shared.number);
    const title = shared.title.trim();
    if (
      shared.number.trim() === '' ||
      !Number.isInteger(number) ||
      number < 1 ||
      title === '' ||
      amount === undefined
    ) {
      error = m.study_unit_edit_invalid();
      return;
    }
    // 저장 중에 다시 누르면 같은 판으로 두 번 보내 두 번째가 내 첫 저장 때문에 409를 받는다(잘못된 "먼저 고쳤다" 안내).
    if (savingUnit || !plan?.can_write) return;
    const attempt = unitDraft.capture();
    if (attempt === undefined) return;
    savingUnit = true;
    error = undefined;
    try {
      await api.updateStudyUnit(base.id, Number(baseVersion), {
        number,
        title,
        chapter: shared.chapter.trim(),
        unit: shared.unit,
        amount,
        planned_on: shared.planned_on === '' ? null : shared.planned_on,
      });
      if (!unitDraft.isOwner(attempt)) return;
      // Closing this target makes isOwner false, so release its busy state first.
      savingUnit = false;
      if (unitDraft.complete(attempt)) editingUnit = undefined;
      status = m.study_unit_saved({ title });
      reloadKey += 1;
    } catch (e: unknown) {
      if (!unitDraft.isOwner(attempt)) return;
      if (
        e instanceof ApiError &&
        e.status === 409 &&
        (await unitMovedOn(base, () => unitDraft.isOwner(attempt)))
      )
        return;
      error = errorText(e, m.study_action_failed());
    } finally {
      if (unitDraft.isOwner(attempt)) savingUnit = false;
    }
  }

  /**
   * 409를 받은 뒤 지금 계획을 다시 읽는다. 다른 사람이 먼저 고쳐 판이 바뀌었으면(보관으로 막힌 409와 구분) 기준을 지금
   * 판으로 바꾸고 알린다(입력은 그대로). 단위가 지워졌으면 고치기를 닫고 알린다. 처리했으면 true.
   */
  async function unitMovedOn(base: StudyUnitView, stillOwner: () => boolean): Promise<boolean> {
    let now: StudyPlanView;
    try {
      now = await api.studyPlan(itemId);
    } catch {
      return false;
    }
    if (!stillOwner()) return true;
    plan = now;
    const latest = now.units.find((x) => x.id === base.id);
    if (latest === undefined) {
      error = m.common_error_not_found();
      return true;
    }
    if (latest.version === base.version) return false;
    editingUnit = latest;
    baseVersion = String(latest.version);
    error = m.study_unit_changed();
    return true;
  }

  async function addUnit(e: SubmitEvent): Promise<void> {
    e.preventDefault();
    if (adding || !plan?.can_write) return;
    const amount = wholeAmount(draft.amount);
    if (draft.title.trim() === '' || amount === undefined) {
      error = m.study_unit_invalid();
      return;
    }
    const unit = {
      title: draft.title.trim(),
      chapter: draft.chapter.trim(),
      unit: draft.unit,
      amount,
      planned_on: draft.planned_on === '' ? null : draft.planned_on,
    };
    const payload = JSON.stringify(unit);
    if (payload !== requestPayload) {
      requestPayload = payload;
      requestId = newRequestId();
    }
    const attempt = newDraft.capture();
    if (attempt === undefined) return;
    error = undefined;
    adding = true;
    try {
      await api.addStudyUnits(itemId, [{ request_id: requestId, ...unit }]);
      if (!newDraft.isOwner(attempt)) return;
      newDraft.complete(attempt);
      status = m.study_unit_added();
      reloadKey += 1;
    } catch (e: unknown) {
      if (newDraft.isOwner(attempt)) error = errorText(e, m.study_action_failed());
    } finally {
      if (newDraft.isOwner(attempt)) adding = false;
    }
  }

  async function saveMine(u: StudyUnitView): Promise<void> {
    if (savingMine || !plan?.can_write) return;
    const doneAmount = wholeAmount(mine.done_amount);
    if (doneAmount === undefined) {
      error = m.study_done_amount_invalid();
      return;
    }
    const attempt = mineDraft.capture();
    if (attempt === undefined) return;
    const sent = { ...mine };
    error = undefined;
    savingMine = true;
    try {
      const fresh = await freshMine(u);
      if (!mineDraft.isOwner(attempt)) return;
      await api.setUnitProgress(u.id, {
        status: fresh.status,
        done_amount: doneAmount,
        planned_on: sent.planned_on === '' ? null : sent.planned_on,
        note: sent.note,
        today,
      });
      if (!mineDraft.isOwner(attempt)) return;
      if (mineDraft.isCurrent(attempt)) {
        mineDraft.complete(attempt);
        mine = sent;
        mineBase = JSON.stringify(sent);
        mineDraft.persist();
      }
      status = m.study_progress_saved({ title: u.title });
      reloadKey += 1;
    } catch (e: unknown) {
      if (mineDraft.isOwner(attempt)) error = errorText(e, m.study_action_failed());
    } finally {
      if (mineDraft.isOwner(attempt)) savingMine = false;
    }
  }
</script>

{#each drafts as draft (draft)}
  <DiscardChangesConfirm
    open={draft.active && draft.leave.open}
    onkeep={draft.leave.keep}
    ondiscard={draft.discardAndLeave}
  />
{/each}

{#if error !== undefined}<p role="alert" class="text-sm text-destructive">{error}</p>{/if}
<p aria-live="polite" class="text-sm">{status}</p>

{#if plan !== undefined && progress !== undefined && owner === me.userId && !accountSwitching()}
  {@const p = progress}
  {@const current = plan}
  <section class="flex flex-col gap-3" aria-labelledby="study-heading">
    <h2 id="study-heading" class="text-lg font-semibold">{m.study_heading()}</h2>
    <div class="grid gap-2 @xl:grid-cols-3">
      <div class="rounded-md border p-2">
        <div class="text-sm text-muted-foreground">{m.study_progress()}</div>
        <div class="text-lg font-semibold">{p.percent}%</div>
        <div class="text-sm">
          {m.study_counts({
            done: p.done,
            inProgress: p.inProgress,
            remaining: p.remaining,
            total: p.total,
          })}
        </div>
      </div>
      <div class="rounded-md border p-2">
        <div class="text-sm text-muted-foreground">{m.study_today()}</div>
        <div class="text-lg font-semibold">
          {m.study_today_counts({ planned: p.todayPlanned, done: p.todayDone })}
        </div>
        <div class="text-sm">{m.study_overdue_count({ count: p.overdue.length })}</div>
      </div>
      <div class="rounded-md border p-2">
        <div class="text-sm text-muted-foreground">{m.study_amount()}</div>
        {#each p.amounts as a (a.unit)}
          <div class="text-sm">
            {m.study_amount_progress({
              done: a.done,
              amount: unitAmount(a.unit, a.target),
              left: a.target - a.done,
            })}
          </div>
        {:else}
          <div class="text-sm">{m.study_none()}</div>
        {/each}
      </div>
    </div>

    {#if p.overdue.length > 0 && current.can_write}
      {#each replanDraft.offers as offer (offer.key)}
        <FormDraftOffer
          onrestore={() => {
            replanDraft.restore(offer);
          }}
          ondiscard={() => {
            replanDraft.discard(offer);
          }}
        />
      {/each}
      <form
        class="flex flex-wrap items-end gap-2 rounded-md border p-2 text-sm"
        aria-label={m.study_replan_form()}
        novalidate
        onsubmit={(e: SubmitEvent) => {
          e.preventDefault();
          void reschedule(p.overdue);
        }}
      >
        <span>{m.study_replan_intro({ count: p.overdue.length })}</span>
        <div class="flex flex-col gap-1">
          <Label for="study-replan-start">{m.study_start_date()}</Label>
          <Input id="study-replan-start" type="date" bind:value={replanStart} class="w-40" />
        </div>
        <div class="flex flex-col gap-1">
          <Label for="study-replan-per">{m.study_per_day()}</Label>
          <Input id="study-replan-per" inputmode="numeric" bind:value={replanPerDay} class="w-20" />
        </div>
        <Button type="submit" size="sm" variant="outline" disabled={replanning}
          >{m.study_replan()}</Button
        >
      </form>
    {/if}

    <ol class="flex flex-col gap-1" data-testid="study-units">
      {#each current.units as u (u.id)}
        {@const planned = plannedOn(u)}
        <li class="flex flex-col gap-1 rounded-md border p-2 text-sm">
          <div class="flex flex-wrap items-baseline gap-2">
            <span class="text-muted-foreground">{u.number}.</span>
            <span class="font-medium">{u.title}</span>
            {#if u.chapter !== ''}<span class="text-muted-foreground">{u.chapter}</span>{/if}
            {#if u.amount > 0}<span>{unitAmount(u.unit, u.amount)}</span>{/if}
            {#if planned !== null}
              <span
                class={u.mine.status !== 'done' && planned < today
                  ? 'text-destructive'
                  : 'text-muted-foreground'}
                >{u.mine.planned_on !== null
                  ? m.study_planned_mine({ date: formatDay(planned) })
                  : formatDay(planned)}</span
              >
            {/if}
            <span class="ml-auto">{statusLabel(u.mine.status)}</span>
          </div>
          <div class="flex flex-wrap gap-1">
            <!-- 진도는 쓸 수 있는 사람만 남긴다. 읽기 전용은 본인 진도도 남기지 못한다(D-20 ⑧, 서버도 거절). -->
            {#if current.can_write}
              {#each ['not_started', 'in_progress', 'done'] as const as s (s)}
                {#if u.mine.status !== s}
                  <Button
                    size="sm"
                    variant="ghost"
                    onclick={() => {
                      setStatus(u, s);
                    }}>{statusLabel(s)}</Button
                  >
                {/if}
              {/each}
              <Button
                size="sm"
                variant="ghost"
                aria-expanded={openUnit === u.id}
                onclick={() => {
                  openMine(u);
                }}>{m.study_mine()}</Button
              >
            {/if}
            <Button
              size="sm"
              variant="ghost"
              aria-label={m.study_track_aria({ title: u.title })}
              disabled={timerBusy ||
                (timer.entry?.target?.kind === 'study_unit' && timer.entry.target.id === u.id)}
              onclick={() => void startUnit(u.id)}
              >{timer.entry?.target?.kind === 'study_unit' && timer.entry.target.id === u.id
                ? m.time_recording()
                : m.study_track()}</Button
            >
            {#if current.can_write}
              <Button
                size="sm"
                variant="ghost"
                aria-expanded={editingUnit?.id === u.id}
                aria-label={m.study_edit_aria({ title: u.title })}
                onclick={() => {
                  editUnit(u);
                }}>{m.study_edit()}</Button
              >
              {#if confirmingDelete !== u.id}
                <Button
                  id={`unit-delete-${u.id}`}
                  size="sm"
                  variant="ghost"
                  aria-label={m.study_delete_aria({ title: u.title })}
                  onclick={() => void askDelete(u.id)}>{m.study_delete()}</Button
                >
              {:else}
                <span class="flex flex-wrap items-center gap-1 text-sm">
                  <span id={`unit-delete-warning-${u.id}`}>{m.study_delete_confirm()}</span>
                  <!-- 초점을 받는 확인 단추가 어느 단위를 지우는지 말하게 한다(회차 지우기와 같다). -->
                  <Button
                    size="sm"
                    variant="destructive"
                    bind:ref={confirmDeleteButton}
                    aria-label={m.study_delete_aria({ title: u.title })}
                    aria-describedby={`unit-delete-warning-${u.id}`}
                    onclick={() => {
                      confirmingDelete = undefined;
                      void run(
                        () => api.deleteStudyUnit(u.id),
                        m.study_deleted({ title: u.title }),
                      );
                    }}>{m.study_delete()}</Button
                  >
                  <Button size="sm" variant="ghost" onclick={() => void cancelDelete(u.id)}
                    >{m.common_cancel()}</Button
                  >
                </span>
              {/if}
            {/if}
          </div>
          {#if editingUnit?.id === u.id}
            {@const base = editingUnit}
            {#each unitDraft.offers as offer (offer.key)}
              <FormDraftOffer
                onrestore={() => {
                  unitDraft.restore(offer);
                }}
                ondiscard={() => {
                  unitDraft.discard(offer);
                }}
              />
            {/each}
            <form
              class="grid gap-2 border-t pt-2 @xl:grid-cols-4"
              aria-label={m.study_unit_form({ title: u.title })}
              novalidate
              onsubmit={(e: SubmitEvent) => {
                e.preventDefault();
                void saveUnit(base);
              }}
            >
              <div class="flex flex-col gap-1">
                <Label for={`unit-number-${u.id}`}>{m.study_number()}</Label>
                <Input id={`unit-number-${u.id}`} inputmode="numeric" bind:value={shared.number} />
              </div>
              <div class="flex flex-col gap-1 @xl:col-span-3">
                <Label for={`unit-title-${u.id}`}>{m.study_new_title()}</Label>
                <Input id={`unit-title-${u.id}`} bind:value={shared.title} maxlength={300} />
              </div>
              <div class="flex flex-col gap-1">
                <Label for={`unit-chapter-${u.id}`}>{m.study_chapter()}</Label>
                <Input id={`unit-chapter-${u.id}`} bind:value={shared.chapter} maxlength={300} />
              </div>
              <div class="flex flex-col gap-1">
                <Label for={`unit-amount-${u.id}`}>{m.study_amount()}</Label>
                <Input id={`unit-amount-${u.id}`} inputmode="numeric" bind:value={shared.amount} />
              </div>
              <div class="flex flex-col gap-1">
                <Label for={`unit-kind-${u.id}`}>{m.study_unit()}</Label>
                <NativeSelect id={`unit-kind-${u.id}`} class="w-full" bind:value={shared.unit}>
                  {#each STUDY_UNIT_KINDS as k (k)}<option value={k}>{unitLabel(k)}</option>{/each}
                </NativeSelect>
              </div>
              <div class="flex flex-col gap-1">
                <Label for={`unit-shared-planned-${u.id}`}>{m.study_planned()}</Label>
                <Input
                  id={`unit-shared-planned-${u.id}`}
                  type="date"
                  bind:value={shared.planned_on}
                />
              </div>
              <div class="flex gap-2 @xl:col-span-4">
                <Button type="submit" size="sm" disabled={savingUnit}>{m.common_save()}</Button>
                <Button
                  size="sm"
                  variant="ghost"
                  onclick={() => {
                    unitDraft.leave.request(() => {
                      editingUnit = undefined;
                    });
                  }}>{m.common_cancel()}</Button
                >
              </div>
            </form>
          {/if}
          {#if openUnit === u.id && current.can_write}
            {#each mineDraft.offers as offer (offer.key)}
              <FormDraftOffer
                onrestore={() => {
                  mineDraft.restore(offer);
                }}
                ondiscard={() => {
                  mineDraft.discard(offer);
                }}
              />
            {/each}
            <form
              class="grid gap-2 border-t pt-2 @xl:grid-cols-3"
              onsubmit={(e: SubmitEvent) => {
                e.preventDefault();
                void saveMine(u);
              }}
            >
              <div class="flex flex-col gap-1">
                <Label for={`unit-done-${u.id}`}
                  >{m.study_done_amount({ unit: unitLabel(u.unit) })}</Label
                >
                <Input id={`unit-done-${u.id}`} inputmode="numeric" bind:value={mine.done_amount} />
              </div>
              <div class="flex flex-col gap-1">
                <Label for={`unit-plan-${u.id}`}>{m.study_my_planned()}</Label>
                <Input id={`unit-plan-${u.id}`} type="date" bind:value={mine.planned_on} />
              </div>
              <div class="flex flex-col gap-1 @xl:col-span-3">
                <Label for={`unit-note-${u.id}`}>{m.study_note()}</Label>
                <Textarea id={`unit-note-${u.id}`} rows={2} bind:value={mine.note} />
              </div>
              <div><Button type="submit" size="sm">{m.common_save()}</Button></div>
            </form>
          {/if}
        </li>
      {:else}
        <li class="text-sm text-muted-foreground">{m.study_empty()}</li>
      {/each}
    </ol>

    {#if current.can_write}
      {#each newDraft.offers as offer (offer.key)}
        <FormDraftOffer
          onrestore={() => {
            newDraft.restore(offer);
          }}
          ondiscard={() => {
            newDraft.discard(offer);
          }}
        />
      {/each}
      <form class="flex flex-wrap items-end gap-2" onsubmit={(e) => void addUnit(e)} novalidate>
        <div class="flex min-w-[min(12rem,100%)] flex-1 flex-col gap-1">
          <Label for="study-unit-title">{m.study_new_title()}</Label>
          <Input id="study-unit-title" bind:value={draft.title} maxlength={300} />
        </div>
        <div class="flex flex-col gap-1">
          <Label for="study-unit-chapter">{m.study_chapter()}</Label>
          <Input id="study-unit-chapter" bind:value={draft.chapter} maxlength={300} />
        </div>
        <div class="flex w-24 flex-col gap-1">
          <Label for="study-unit-amount">{m.study_amount()}</Label>
          <Input id="study-unit-amount" inputmode="numeric" bind:value={draft.amount} />
        </div>
        <div class="flex flex-col gap-1">
          <Label for="study-unit-kind">{m.study_unit()}</Label>
          <NativeSelect id="study-unit-kind" class="w-full" bind:value={draft.unit}>
            {#each STUDY_UNIT_KINDS as k (k)}<option value={k}>{unitLabel(k)}</option>{/each}
          </NativeSelect>
        </div>
        <div class="flex flex-col gap-1">
          <Label for="study-unit-planned">{m.study_planned()}</Label>
          <Input id="study-unit-planned" type="date" bind:value={draft.planned_on} />
        </div>
        <Button type="submit">{m.study_add_unit()}</Button>
      </form>
    {/if}

    {#if participants.length > 0}
      <section class="flex flex-col gap-1" aria-labelledby="study-participants-heading">
        <h3 id="study-participants-heading" class="font-medium">{m.study_participants()}</h3>
        <ul class="text-sm">
          {#each participants as person (person.user_id)}
            <li>
              {m.study_participant({
                name: person.display_name,
                done: person.done,
                inProgress: person.in_progress,
                overdue: person.overdue,
              })}
            </li>
          {/each}
        </ul>
      </section>
    {/if}
  </section>
{/if}
