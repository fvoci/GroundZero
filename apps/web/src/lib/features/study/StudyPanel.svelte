<script lang="ts">
  // 스터디 계획(D-36): 계획 단위(교육과정·챕터·단계, 쪽·문제·분 분량, 공통 계획일)와 내 진행, 계획 대비 진행·남은
  // 분량, 밀린 단위 재배치(내가 고른 시작일·하루 단위 수), 함께 하는 사람의 진행 집계.
  import type {
    EpisodeStatus,
    StudyParticipantView,
    StudyPlanView,
    StudyUnitKind,
    StudyUnitView,
  } from '$lib/api/client';
  import { api, STUDY_UNIT_KINDS } from '$lib/api/client';
  import { RetryKey } from '$lib/api/retry-key';
  import { errorText } from '$lib/api/errors';
  import { formatDay } from '$lib/display';
  import { localTimeZone, wallTime } from '$lib/features/events/time';
  import { timer } from '$lib/features/time/timer.svelte';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';

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

  const box =
    'h-9 rounded-md border border-input bg-transparent px-3 text-base shadow-xs focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none';

  let plan = $state<StudyPlanView | undefined>(undefined);
  let participants = $state<StudyParticipantView[]>([]);
  let error = $state<string | undefined>(undefined);
  let status = $state('');
  let reloadKey = $state(0);
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
  const draftKey = new RetryKey();
  let replanStart = $state(today);
  let replanPerDay = $state('1');
  let openUnit = $state<string | undefined>(undefined);
  let mine = $state({ done_amount: '', planned_on: '', note: '' });

  const progress = $derived(plan === undefined ? undefined : studyProgress(plan.units, today));

  $effect(() => {
    const id = itemId;
    if (reloadKey < 0) return;
    let cancelled = false;
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
  }

  async function addUnit(e: SubmitEvent): Promise<void> {
    e.preventDefault();
    const amount = draft.amount.trim() === '' ? 0 : Number(draft.amount);
    if (draft.title.trim() === '' || !Number.isInteger(amount) || amount < 0) {
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
    const requestId = draftKey.for({ itemId, unit });
    const ok = await run(
      () => api.addStudyUnits(itemId, [{ request_id: requestId, ...unit }]),
      m.study_unit_added(),
    );
    if (ok) {
      draft = { title: '', chapter: '', unit: draft.unit, amount: '', planned_on: '' };
      draftKey.done();
    }
  }
</script>

{#if error !== undefined}<p role="alert" class="text-sm text-destructive">{error}</p>{/if}
<p aria-live="polite" class="text-sm">{status}</p>

{#if plan !== undefined && progress !== undefined}
  {@const p = progress}
  {@const current = plan}
  <section class="flex flex-col gap-3" aria-labelledby="study-heading">
    <h2 id="study-heading" class="text-lg font-semibold">{m.study_heading()}</h2>
    <div class="grid gap-2 sm:grid-cols-3">
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

    {#if p.overdue.length > 0}
      <form
        class="flex flex-wrap items-end gap-2 rounded-md border p-2 text-sm"
        aria-label={m.study_replan_form()}
        onsubmit={(e: SubmitEvent) => {
          e.preventDefault();
          const per = Number(replanPerDay);
          if (!Number.isInteger(per) || per < 1) {
            error = m.study_per_day_invalid();
            return;
          }
          void run(
            () => api.planStudyUnits(itemId, replan(p.overdue, replanStart, per)),
            m.study_replanned({
              count: p.overdue.length,
              start: replanStart === '' ? '' : formatDay(replanStart),
              perDay: per,
            }),
          );
        }}
      >
        <span>{m.study_replan_intro({ count: p.overdue.length })}</span>
        <div class="flex flex-col gap-1">
          <Label for="study-replan-start">{m.study_start_date()}</Label>
          <Input id="study-replan-start" type="date" bind:value={replanStart} class="w-40" />
        </div>
        <div class="flex flex-col gap-1">
          <Label for="study-replan-per">{m.study_per_day()}</Label>
          <Input
            id="study-replan-per"
            type="number"
            min={1}
            bind:value={replanPerDay}
            class="w-20"
          />
        </div>
        <Button type="submit" size="sm" variant="outline">{m.study_replan()}</Button>
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
            <Button
              size="sm"
              variant="ghost"
              aria-label={m.study_track_aria({ title: u.title })}
              onclick={() =>
                void timer
                  .start('study', { kind: 'study_unit', id: u.id })
                  .then(() => {
                    status = m.study_track_started({ title: u.title });
                  })
                  .catch(() => {
                    error = m.study_timer_busy();
                  })}>{m.study_track()}</Button
            >
            {#if current.can_write}
              <Button
                size="sm"
                variant="ghost"
                onclick={() =>
                  void run(() => api.deleteStudyUnit(u.id), m.study_deleted({ title: u.title }))}
                >{m.study_delete()}</Button
              >
            {/if}
          </div>
          {#if openUnit === u.id}
            <form
              class="grid gap-2 border-t pt-2 sm:grid-cols-3"
              onsubmit={(e: SubmitEvent) => {
                e.preventDefault();
                const doneAmount = mine.done_amount.trim() === '' ? 0 : Number(mine.done_amount);
                if (!Number.isInteger(doneAmount) || doneAmount < 0) {
                  error = m.study_done_amount_invalid();
                  return;
                }
                void run(
                  async () =>
                    api.setUnitProgress(u.id, {
                      status: (await freshMine(u)).status,
                      done_amount: doneAmount,
                      planned_on: mine.planned_on === '' ? null : mine.planned_on,
                      note: mine.note,
                      today,
                    }),
                  m.study_progress_saved({ title: u.title }),
                );
              }}
            >
              <div class="flex flex-col gap-1">
                <Label for={`unit-done-${u.id}`}
                  >{m.study_done_amount({ unit: unitLabel(u.unit) })}</Label
                >
                <Input
                  id={`unit-done-${u.id}`}
                  type="number"
                  min={0}
                  bind:value={mine.done_amount}
                />
              </div>
              <div class="flex flex-col gap-1">
                <Label for={`unit-plan-${u.id}`}>{m.study_my_planned()}</Label>
                <Input id={`unit-plan-${u.id}`} type="date" bind:value={mine.planned_on} />
              </div>
              <div class="flex flex-col gap-1 sm:col-span-3">
                <Label for={`unit-note-${u.id}`}>{m.study_note()}</Label>
                <textarea
                  id={`unit-note-${u.id}`}
                  class="rounded-md border border-input bg-transparent px-3 py-2 text-base"
                  rows={2}
                  bind:value={mine.note}></textarea>
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
      <form class="flex flex-wrap items-end gap-2" onsubmit={(e) => void addUnit(e)} novalidate>
        <div class="flex flex-1 flex-col gap-1">
          <Label for="study-unit-title">{m.study_new_title()}</Label>
          <Input id="study-unit-title" bind:value={draft.title} maxlength={300} />
        </div>
        <div class="flex flex-col gap-1">
          <Label for="study-unit-chapter">{m.study_chapter()}</Label>
          <Input id="study-unit-chapter" bind:value={draft.chapter} maxlength={300} />
        </div>
        <div class="flex w-24 flex-col gap-1">
          <Label for="study-unit-amount">{m.study_amount()}</Label>
          <Input id="study-unit-amount" type="number" min={0} bind:value={draft.amount} />
        </div>
        <div class="flex flex-col gap-1">
          <Label for="study-unit-kind">{m.study_unit()}</Label>
          <select id="study-unit-kind" class={box} bind:value={draft.unit}>
            {#each STUDY_UNIT_KINDS as k (k)}<option value={k}>{unitLabel(k)}</option>{/each}
          </select>
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
