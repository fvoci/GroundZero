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
  import { api, ApiError, newRequestId, STUDY_UNIT_KINDS } from '$lib/api/client';
  import { localTimeZone, wallTime } from '$lib/features/events/time';
  import { timer } from '$lib/features/time/timer.svelte';
  import { Button } from '$lib/ui/button';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';

  import { plannedOn, replan, studyProgress, UNIT_LABEL } from './progress';

  interface Props {
    itemId: string;
  }

  const { itemId }: Props = $props();

  const today = wallTime(new Date().toISOString(), localTimeZone()).slice(0, 10);
  const STATUS_LABEL: Record<EpisodeStatus, string> = {
    not_started: '시작 전',
    in_progress: '진행 중',
    done: '완료',
  };
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
  let draftId = newRequestId();
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
        if (!cancelled)
          error = e instanceof ApiError ? e.message : '스터디 계획을 불러오지 못했습니다.';
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
      error = e instanceof ApiError ? e.message : '처리하지 못했습니다.';
      return false;
    }
  }

  function setStatus(u: StudyUnitView, next: EpisodeStatus): void {
    void run(
      () =>
        api.setUnitProgress(u.id, {
          status: next,
          done_amount: next === 'done' ? u.amount : u.mine.done_amount,
          planned_on: u.mine.planned_on,
          note: u.mine.note,
          today,
        }),
      `${String(u.number)}. ${u.title}: ${STATUS_LABEL[next]}`,
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
      error = '제목과 분량(0 이상의 정수)을 확인해 주세요.';
      return;
    }
    const ok = await run(
      () =>
        api.addStudyUnits(itemId, [
          {
            request_id: draftId,
            title: draft.title.trim(),
            chapter: draft.chapter.trim(),
            unit: draft.unit,
            amount,
            planned_on: draft.planned_on === '' ? null : draft.planned_on,
          },
        ]),
      '단위를 더했습니다.',
    );
    if (ok) {
      draft = { title: '', chapter: '', unit: draft.unit, amount: '', planned_on: '' };
      draftId = newRequestId();
    }
  }
</script>

{#if error !== undefined}<p role="alert" class="text-sm text-destructive">{error}</p>{/if}
<p aria-live="polite" class="text-sm">{status}</p>

{#if plan !== undefined && progress !== undefined}
  {@const p = progress}
  {@const current = plan}
  <section class="flex flex-col gap-3" aria-labelledby="study-heading">
    <h2 id="study-heading" class="text-lg font-semibold">계획과 내 진행</h2>
    <div class="grid gap-2 sm:grid-cols-3">
      <div class="rounded-md border p-2">
        <div class="text-sm text-muted-foreground">진행률</div>
        <div class="text-lg font-semibold">{p.percent}%</div>
        <div class="text-sm">
          완료 {p.done} · 진행 중 {p.inProgress} · 남음 {p.remaining} / {p.total}
        </div>
      </div>
      <div class="rounded-md border p-2">
        <div class="text-sm text-muted-foreground">오늘</div>
        <div class="text-lg font-semibold">예정 {p.todayPlanned} · 완료 {p.todayDone}</div>
        <div class="text-sm">밀린 단위 {p.overdue.length}</div>
      </div>
      <div class="rounded-md border p-2">
        <div class="text-sm text-muted-foreground">분량</div>
        {#each p.amounts as a (a.unit)}
          <div class="text-sm">
            {a.done}/{a.target}{UNIT_LABEL[a.unit]} (남은 {a.target - a.done})
          </div>
        {:else}
          <div class="text-sm">없음</div>
        {/each}
      </div>
    </div>

    {#if p.overdue.length > 0}
      <form
        class="flex flex-wrap items-end gap-2 rounded-md border p-2 text-sm"
        aria-label="밀린 단위 다시 배정"
        onsubmit={(e: SubmitEvent) => {
          e.preventDefault();
          const per = Number(replanPerDay);
          if (!Number.isInteger(per) || per < 1) {
            error = '하루에 할 단위 수는 1 이상의 정수입니다.';
            return;
          }
          void run(
            () => api.planStudyUnits(itemId, replan(p.overdue, replanStart, per)),
            `밀린 ${String(p.overdue.length)}개를 ${replanStart}부터 하루 ${String(per)}개씩 다시 배정했습니다.`,
          );
        }}
      >
        <span>밀린 단위 {p.overdue.length}개를 내 계획으로 다시 배정:</span>
        <div class="flex flex-col gap-1">
          <Label for="study-replan-start">시작일</Label>
          <Input id="study-replan-start" type="date" bind:value={replanStart} class="w-40" />
        </div>
        <div class="flex flex-col gap-1">
          <Label for="study-replan-per">하루에</Label>
          <Input
            id="study-replan-per"
            type="number"
            min={1}
            bind:value={replanPerDay}
            class="w-20"
          />
        </div>
        <Button type="submit" size="sm" variant="outline">다시 배정</Button>
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
            {#if u.amount > 0}<span>{u.amount}{UNIT_LABEL[u.unit]}</span>{/if}
            {#if planned !== null}
              <span
                class={u.mine.status !== 'done' && planned < today
                  ? 'text-destructive'
                  : 'text-muted-foreground'}
                >{planned}{u.mine.planned_on !== null ? '(내 날짜)' : ''}</span
              >
            {/if}
            <span class="ml-auto">{STATUS_LABEL[u.mine.status]}</span>
          </div>
          <div class="flex flex-wrap gap-1">
            {#each ['not_started', 'in_progress', 'done'] as const as s (s)}
              {#if u.mine.status !== s}
                <Button
                  size="sm"
                  variant="ghost"
                  onclick={() => {
                    setStatus(u, s);
                  }}>{STATUS_LABEL[s]}</Button
                >
              {/if}
            {/each}
            <Button
              size="sm"
              variant="ghost"
              aria-expanded={openUnit === u.id}
              onclick={() => {
                openMine(u);
              }}>내 분량·날짜·메모</Button
            >
            <Button
              size="sm"
              variant="ghost"
              aria-label={`${u.title} 공부 시간 측정`}
              onclick={() =>
                void timer
                  .start('study', { kind: 'study_unit', id: u.id })
                  .then(() => {
                    status = `${u.title}: 공부 시간 측정을 시작했습니다.`;
                  })
                  .catch(() => {
                    error = '이미 측정 중인 타이머가 있습니다. 먼저 끝내 주세요.';
                  })}>측정</Button
            >
            {#if current.can_write}
              <Button
                size="sm"
                variant="ghost"
                onclick={() =>
                  void run(() => api.deleteStudyUnit(u.id), `${u.title}을(를) 지웠습니다.`)}
                >지우기</Button
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
                  error = '한 분량은 0 이상의 정수입니다.';
                  return;
                }
                void run(
                  () =>
                    api.setUnitProgress(u.id, {
                      status: u.mine.status,
                      done_amount: doneAmount,
                      planned_on: mine.planned_on === '' ? null : mine.planned_on,
                      note: mine.note,
                      today,
                    }),
                  `${u.title}: 내 진행을 저장했습니다.`,
                );
              }}
            >
              <div class="flex flex-col gap-1">
                <Label for={`unit-done-${u.id}`}>한 분량({UNIT_LABEL[u.unit]})</Label>
                <Input
                  id={`unit-done-${u.id}`}
                  type="number"
                  min={0}
                  bind:value={mine.done_amount}
                />
              </div>
              <div class="flex flex-col gap-1">
                <Label for={`unit-plan-${u.id}`}>내 계획일</Label>
                <Input id={`unit-plan-${u.id}`} type="date" bind:value={mine.planned_on} />
              </div>
              <div class="flex flex-col gap-1 sm:col-span-3">
                <Label for={`unit-note-${u.id}`}>메모(나만 봄)</Label>
                <textarea
                  id={`unit-note-${u.id}`}
                  class="rounded-md border border-input bg-transparent px-3 py-2 text-base"
                  rows={2}
                  bind:value={mine.note}></textarea>
              </div>
              <div><Button type="submit" size="sm">저장</Button></div>
            </form>
          {/if}
        </li>
      {:else}
        <li class="text-sm text-muted-foreground">계획 단위가 없습니다.</li>
      {/each}
    </ol>

    {#if current.can_write}
      <form class="flex flex-wrap items-end gap-2" onsubmit={(e) => void addUnit(e)} novalidate>
        <div class="flex flex-1 flex-col gap-1">
          <Label for="study-unit-title">새 단위 제목</Label>
          <Input id="study-unit-title" bind:value={draft.title} maxlength={300} />
        </div>
        <div class="flex flex-col gap-1">
          <Label for="study-unit-chapter">교육과정·챕터</Label>
          <Input id="study-unit-chapter" bind:value={draft.chapter} maxlength={300} />
        </div>
        <div class="flex w-24 flex-col gap-1">
          <Label for="study-unit-amount">분량</Label>
          <Input id="study-unit-amount" type="number" min={0} bind:value={draft.amount} />
        </div>
        <div class="flex flex-col gap-1">
          <Label for="study-unit-kind">단위</Label>
          <select id="study-unit-kind" class={box} bind:value={draft.unit}>
            {#each STUDY_UNIT_KINDS as k (k)}<option value={k}>{UNIT_LABEL[k]}</option>{/each}
          </select>
        </div>
        <div class="flex flex-col gap-1">
          <Label for="study-unit-planned">계획일</Label>
          <Input id="study-unit-planned" type="date" bind:value={draft.planned_on} />
        </div>
        <Button type="submit">단위 추가</Button>
      </form>
    {/if}

    {#if participants.length > 0}
      <section class="flex flex-col gap-1" aria-labelledby="study-participants-heading">
        <h3 id="study-participants-heading" class="font-medium">함께 하는 사람</h3>
        <ul class="text-sm">
          {#each participants as person (person.user_id)}
            <li>
              {person.display_name}: 완료 {person.done} · 진행 중 {person.in_progress} · 밀림 {person.overdue}
            </li>
          {/each}
        </ul>
      </section>
    {/if}
  </section>
{/if}
