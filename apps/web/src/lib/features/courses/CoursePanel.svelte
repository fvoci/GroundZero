<script lang="ts">
  // 강의 모듈의 강좌 화면(D-37): 강좌 정보, 진도 요약, 회차 목록과 내 진도, 회차 추가·CSV 일괄 등록, 함께
  // 수강하는 사람의 진도 집계. 완료는 직접 표시한다. 회차마다 강의 시간 측정을 시작할 수 있다(D-32).
  import type { CourseView, EpisodeStatus, EpisodeView, ParticipantView } from '$lib/api/client';
  import { api, ApiError, newRequestId } from '$lib/api/client';
  import { localTimeZone, wallTime } from '$lib/features/events/time';
  import { clockText, durationText } from '$lib/features/time/format';
  import { timer } from '$lib/features/time/timer.svelte';
  import { Button } from '$lib/ui/button';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';
  import { isWebUrl } from '$lib/url';

  import type { EpisodeRow } from './csv';
  import { parseDuration, parseEpisodesCsv } from './csv';
  import { courseProgress, daysUntil, plannedOn } from './progress';

  interface Props {
    itemId: string;
  }

  const { itemId }: Props = $props();

  const zone = localTimeZone();
  const today = wallTime(new Date().toISOString(), zone).slice(0, 10);
  const STATUS_LABEL: Record<EpisodeStatus, string> = {
    not_started: '미수강',
    in_progress: '수강 중',
    done: '완료',
  };

  let course = $state<CourseView | undefined>(undefined);
  let participants = $state<ParticipantView[]>([]);
  let error = $state<string | undefined>(undefined);
  let status = $state('');
  let editing = $state(false);
  let details = $state({
    subject: '',
    instructor: '',
    platform: '',
    url: '',
    textbook: '',
    starts_on: '',
    expires_on: '',
    target_on: '',
  });
  let openEpisode = $state<string | undefined>(undefined);
  let mine = $state({ planned_on: '', position: '', note: '' });
  let newEpisode = $state({ title: '', chapter: '', duration: '', planned_on: '' });
  let csvText = $state('');
  let reloadKey = $state(0);
  let seq = 0;

  const progress = $derived(
    course === undefined
      ? undefined
      : courseProgress(course.episodes, today, course.details.target_on),
  );

  $effect(() => {
    const id = itemId;
    if (reloadKey < 0) return;
    const mine = ++seq;
    Promise.all([api.course(id), api.courseParticipants(id, today)])
      .then(([c, p]) => {
        if (mine !== seq) return;
        course = c;
        participants = p;
        error = undefined;
      })
      .catch((e: unknown) => {
        if (mine === seq) error = e instanceof ApiError ? e.message : '강좌를 불러오지 못했습니다.';
      });
  });

  async function run(action: () => Promise<unknown>, done: string): Promise<void> {
    error = undefined;
    try {
      await action();
      status = done;
      reloadKey += 1;
    } catch (e: unknown) {
      error = e instanceof ApiError ? e.message : '처리하지 못했습니다.';
    }
  }

  function startEdit(c: CourseView): void {
    const d = c.details;
    details = {
      subject: d.subject,
      instructor: d.instructor,
      platform: d.platform,
      url: d.url,
      textbook: d.textbook,
      starts_on: d.starts_on ?? '',
      expires_on: d.expires_on ?? '',
      target_on: d.target_on ?? '',
    };
    editing = true;
  }

  const orNull = (v: string): string | null => (v === '' ? null : v);

  function setStatus(e: EpisodeView, next: EpisodeStatus): void {
    void run(
      () =>
        api.setEpisodeProgress(e.id, {
          status: next,
          planned_on: e.mine.planned_on,
          last_position_seconds: e.mine.last_position_seconds,
          note: e.mine.note,
          today,
        }),
      `${String(e.number)}회 ${STATUS_LABEL[next]}`,
    );
  }

  function open(e: EpisodeView): void {
    if (openEpisode === e.id) {
      openEpisode = undefined;
      return;
    }
    openEpisode = e.id;
    mine = {
      planned_on: e.mine.planned_on ?? '',
      position:
        e.mine.last_position_seconds === null ? '' : clockText(e.mine.last_position_seconds),
      note: e.mine.note,
    };
  }

  const csv = $derived(csvText.trim() === '' ? undefined : parseEpisodesCsv(csvText));

  // 요청 id는 입력을 시작할 때 정하고 성공할 때까지 지킨다. 응답을 잃고 다시 눌러도 서버가 같은 회차로 본다.
  let draftId = newRequestId();
  const csvIds = $derived(csv === undefined ? [] : csv.rows.map(() => newRequestId()));

  /** 회차를 더한다. 성공하면 true. */
  async function addEpisodes(rows: EpisodeRow[], ids: readonly string[]): Promise<boolean> {
    if (course === undefined || rows.length === 0) return false;
    const before = progress?.percentByCount ?? 0;
    error = undefined;
    let added: number;
    try {
      added = await api.addEpisodes(
        itemId,
        rows.map((r, i) => ({
          request_id: ids[i] ?? newRequestId(),
          url: '',
          materials: '',
          ...r,
        })),
      );
    } catch (e: unknown) {
      error = e instanceof ApiError ? e.message : '처리하지 못했습니다.';
      return false;
    }
    const after = await api.course(itemId);
    course = after;
    reloadKey += 1;
    const now = courseProgress(after.episodes, today, after.details.target_on).percentByCount;
    // 목차가 늘면 진행률이 바뀐다는 것을 알린다(D-37).
    status = `회차 ${String(added)}개를 더했습니다. 진행률 ${String(before)}% → ${String(now)}%`;
    return true;
  }
</script>

{#if error !== undefined}
  <p role="alert" class="text-sm text-destructive">{error}</p>
{/if}
<p aria-live="polite" class="text-sm">{status}</p>

{#if course !== undefined && progress !== undefined}
  {@const c = course}
  {@const p = progress}
  <section class="flex flex-col gap-2" aria-labelledby="course-info-heading">
    <div class="flex items-center gap-2">
      <h2 id="course-info-heading" class="text-lg font-semibold">강좌 정보</h2>
      {#if c.can_write && !editing}
        <Button
          size="sm"
          variant="outline"
          onclick={() => {
            startEdit(c);
          }}>고치기</Button
        >
      {/if}
    </div>
    {#if editing}
      <form
        class="grid gap-2 sm:grid-cols-2"
        onsubmit={(e: SubmitEvent) => {
          e.preventDefault();
          if (details.url.trim() !== '' && !isWebUrl(details.url.trim())) {
            error = '링크는 http:// 또는 https://로 시작해야 합니다.';
            return;
          }
          editing = false;
          void run(
            () =>
              api.setCourseDetails(itemId, {
                subject: details.subject,
                instructor: details.instructor,
                platform: details.platform,
                url: details.url,
                textbook: details.textbook,
                starts_on: orNull(details.starts_on),
                expires_on: orNull(details.expires_on),
                target_on: orNull(details.target_on),
              }),
            '강좌 정보를 저장했습니다.',
          );
        }}
      >
        {#each [['subject', '과목'], ['instructor', '강사'], ['platform', '플랫폼'], ['url', '링크'], ['textbook', '교재']] as const as [key, label] (key)}
          <div class="flex flex-col gap-1">
            <Label for={`course-${key}`}>{label}</Label>
            <Input id={`course-${key}`} bind:value={details[key]} maxlength={400} />
          </div>
        {/each}
        {#each [['starts_on', '수강 시작일'], ['expires_on', '수강권 만료일'], ['target_on', '목표 완료일']] as const as [key, label] (key)}
          <div class="flex flex-col gap-1">
            <Label for={`course-${key}`}>{label}</Label>
            <Input id={`course-${key}`} type="date" bind:value={details[key]} />
          </div>
        {/each}
        <div class="flex gap-2 sm:col-span-2">
          <Button type="submit">저장</Button>
          <Button variant="ghost" onclick={() => (editing = false)}>취소</Button>
        </div>
      </form>
    {:else}
      <dl class="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-sm">
        {#each [['과목', c.details.subject], ['강사', c.details.instructor], ['플랫폼', c.details.platform], ['교재', c.details.textbook], ['수강 시작일', c.details.starts_on ?? ''], ['수강권 만료일', c.details.expires_on ?? ''], ['목표 완료일', c.details.target_on ?? '']] as [label, value] (label)}
          {#if value !== ''}
            <dt class="text-muted-foreground">{label}</dt>
            <dd>{value}</dd>
          {/if}
        {/each}
        {#if isWebUrl(c.details.url)}
          <dt class="text-muted-foreground">링크</dt>
          <dd>
            <a class="underline" href={c.details.url} target="_blank" rel="noopener noreferrer"
              >{c.details.url}</a
            >
          </dd>
        {/if}
      </dl>
    {/if}
    {#if c.details.expires_on !== null && daysUntil(today, c.details.expires_on) <= 14}
      <p class="text-sm text-destructive" role="status">
        {daysUntil(today, c.details.expires_on) < 0
          ? '수강권이 만료되었습니다.'
          : `수강권이 ${String(daysUntil(today, c.details.expires_on))}일 뒤 만료됩니다.`}
      </p>
    {/if}
  </section>

  <section class="flex flex-col gap-2" aria-labelledby="course-progress-heading">
    <h2 id="course-progress-heading" class="text-lg font-semibold">내 진도</h2>
    <div class="grid gap-2 text-sm sm:grid-cols-3">
      <div class="rounded-md border p-2">
        <div class="text-muted-foreground">진행률</div>
        <div class="text-lg font-semibold">
          회차 {p.percentByCount}% · 영상 {p.percentByDuration}%
        </div>
        <div>완료 {p.done} · 수강 중 {p.inProgress} · 남음 {p.remaining} / {p.total}</div>
      </div>
      <div class="rounded-md border p-2">
        <div class="text-muted-foreground">오늘</div>
        <div class="text-lg font-semibold">예정 {p.todayPlanned} · 완료 {p.todayDone}</div>
        <div>밀린 회차 {p.overdue.length}</div>
      </div>
      <div class="rounded-md border p-2">
        <div class="text-muted-foreground">남은 분량</div>
        <div class="text-lg font-semibold">{durationText(p.remainingSeconds)}</div>
        {#if p.perDayEpisodes !== null && p.perDaySeconds !== null}
          <div>목표일까지 하루 {p.perDayEpisodes}회차 · {durationText(p.perDaySeconds)}</div>
        {/if}
      </div>
    </div>
    {#if p.overdue.length > 0}
      <p class="text-sm">
        밀린 회차: {p.overdue.map((e) => `${String(e.number)}회`).join(', ')} — 아래에서 내 계획일을 다시
        정할 수 있습니다.
      </p>
    {/if}
  </section>

  <section class="flex flex-col gap-2" aria-labelledby="episodes-heading">
    <h2 id="episodes-heading" class="text-lg font-semibold">회차</h2>
    {#if c.episodes.length === 0}
      <p class="text-sm text-muted-foreground">회차가 없습니다.</p>
    {/if}
    <ol class="flex flex-col gap-1 text-sm" data-testid="episodes">
      {#each c.episodes as e (e.id)}
        {@const planned = plannedOn(e)}
        <li class="flex flex-col gap-1 rounded-md border p-2">
          <div class="flex flex-wrap items-center gap-2">
            <span class="w-10 shrink-0 tabular-nums text-muted-foreground">{e.number}회</span>
            {#if e.chapter !== ''}<span class="text-xs text-muted-foreground">{e.chapter}</span
              >{/if}
            {#if isWebUrl(e.url)}
              <a
                class="min-w-0 flex-1 truncate underline"
                href={e.url}
                target="_blank"
                rel="noopener noreferrer">{e.title}</a
              >
            {:else}
              <span class="min-w-0 flex-1 truncate">{e.title}</span>
            {/if}
            <span class="text-xs text-muted-foreground tabular-nums"
              >{e.duration_seconds > 0 ? clockText(e.duration_seconds) : ''}</span
            >
            {#if planned !== null}
              <span
                class={[
                  'text-xs',
                  e.mine.status !== 'done' && planned < today
                    ? 'text-destructive'
                    : 'text-muted-foreground',
                ]}>{planned}</span
              >
            {/if}
            <span class="text-xs"
              >{STATUS_LABEL[e.mine.status]}{e.mine.watch_count > 1
                ? ` · ${String(e.mine.watch_count)}번 봄`
                : ''}</span
            >
          </div>
          <div class="flex flex-wrap gap-1">
            {#if e.mine.status !== 'done'}
              <Button
                size="sm"
                variant="outline"
                aria-label={`${String(e.number)}회 완료`}
                onclick={() => {
                  setStatus(e, 'done');
                }}>완료</Button
              >
            {:else}
              <Button
                size="sm"
                variant="ghost"
                aria-label={`${String(e.number)}회 다시 보기`}
                onclick={() => {
                  setStatus(e, 'in_progress');
                }}>다시 보기</Button
              >
            {/if}
            {#if e.mine.status === 'not_started'}
              <Button
                size="sm"
                variant="ghost"
                aria-label={`${String(e.number)}회 수강 중`}
                onclick={() => {
                  setStatus(e, 'in_progress');
                }}>수강 중</Button
              >
            {/if}
            <Button
              size="sm"
              variant="ghost"
              aria-label={`${String(e.number)}회 강의 시간 측정`}
              onclick={() =>
                void timer
                  .start('lecture', { kind: 'episode', id: e.id })
                  .then(() => {
                    status = `${String(e.number)}회 강의 시간 측정을 시작했습니다.`;
                  })
                  .catch(() => {
                    error = '이미 측정 중인 타이머가 있습니다. 먼저 끝내 주세요.';
                  })}>측정</Button
            >
            <Button
              size="sm"
              variant="ghost"
              aria-expanded={openEpisode === e.id}
              aria-label={`${String(e.number)}회 내 메모·날짜`}
              onclick={() => {
                open(e);
              }}>메모·날짜</Button
            >
            {#if c.can_write}
              <Button
                size="sm"
                variant="ghost"
                aria-label={`${String(e.number)}회 지우기`}
                onclick={() =>
                  void run(() => api.deleteEpisode(e.id), `${String(e.number)}회를 지웠습니다.`)}
                >지우기</Button
              >
            {/if}
          </div>
          {#if openEpisode === e.id}
            <form
              class="grid gap-2 border-t pt-2 sm:grid-cols-3"
              onsubmit={(ev: SubmitEvent) => {
                ev.preventDefault();
                const position = parseDuration(mine.position.includes(':') ? mine.position : '');
                void run(
                  () =>
                    api.setEpisodeProgress(e.id, {
                      status: e.mine.status,
                      planned_on: orNull(mine.planned_on),
                      last_position_seconds: mine.position === '' ? null : position,
                      note: mine.note,
                      today,
                    }),
                  `${String(e.number)}회 내 메모·날짜를 저장했습니다.`,
                );
              }}
            >
              <div class="flex flex-col gap-1">
                <Label for={`mine-planned-${e.id}`}>내 계획일</Label>
                <Input id={`mine-planned-${e.id}`} type="date" bind:value={mine.planned_on} />
              </div>
              <div class="flex flex-col gap-1">
                <Label for={`mine-position-${e.id}`}>마지막 시청 위치(분:초)</Label>
                <Input
                  id={`mine-position-${e.id}`}
                  bind:value={mine.position}
                  placeholder="12:30"
                />
              </div>
              <div class="flex flex-col gap-1 sm:col-span-3">
                <Label for={`mine-note-${e.id}`}>내 메모(나만 봄)</Label>
                <textarea
                  id={`mine-note-${e.id}`}
                  class="min-h-20 rounded-md border bg-background p-2 text-sm"
                  bind:value={mine.note}></textarea>
              </div>
              <div><Button type="submit" size="sm">저장</Button></div>
            </form>
          {/if}
        </li>
      {/each}
    </ol>

    {#if c.can_write}
      <form
        class="flex flex-wrap items-end gap-2"
        onsubmit={(ev: SubmitEvent) => {
          ev.preventDefault();
          const seconds = parseDuration(newEpisode.duration);
          if (newEpisode.title.trim() === '' || seconds === null) {
            error = '제목과 길이(분 또는 분:초)를 확인해 주세요.';
            return;
          }
          const row = {
            title: newEpisode.title.trim(),
            chapter: newEpisode.chapter.trim(),
            duration_seconds: seconds,
            planned_on: orNull(newEpisode.planned_on),
          };
          void addEpisodes([row], [draftId]).then((ok) => {
            if (!ok) return;
            newEpisode = { title: '', chapter: '', duration: '', planned_on: '' };
            draftId = newRequestId();
          });
        }}
      >
        <div class="flex flex-1 flex-col gap-1">
          <Label for="episode-title">새 회차 제목</Label>
          <Input id="episode-title" bind:value={newEpisode.title} maxlength={300} />
        </div>
        <div class="flex flex-col gap-1">
          <Label for="episode-chapter">챕터</Label>
          <Input id="episode-chapter" bind:value={newEpisode.chapter} />
        </div>
        <div class="flex w-24 flex-col gap-1">
          <Label for="episode-duration">길이</Label>
          <Input id="episode-duration" bind:value={newEpisode.duration} placeholder="12:30" />
        </div>
        <div class="flex flex-col gap-1">
          <Label for="episode-planned">계획일</Label>
          <Input id="episode-planned" type="date" bind:value={newEpisode.planned_on} />
        </div>
        <Button type="submit">회차 추가</Button>
      </form>
      <details class="text-sm">
        <summary class="cursor-pointer">CSV로 여러 회차 등록</summary>
        <div class="flex flex-col gap-2 pt-2">
          <Label for="episodes-csv"
            >한 줄에 한 회차: 제목, 챕터, 길이(분 또는 분:초), 계획일(YYYY-MM-DD)</Label
          >
          <textarea
            id="episodes-csv"
            class="min-h-32 rounded-md border bg-background p-2 font-mono text-sm"
            bind:value={csvText}></textarea>
          {#if csv !== undefined}
            <p aria-live="polite">
              {csv.rows.length}개 회차를 읽었습니다.
              {#if csv.errors.length > 0}
                <span class="text-destructive">
                  고칠 줄: {csv.errors
                    .map((er) => `${String(er.line)}번째(${er.reason})`)
                    .join(', ')}
                </span>
              {/if}
            </p>
          {/if}
          <div>
            <Button
              variant="outline"
              disabled={csv === undefined || csv.rows.length === 0 || csv.errors.length > 0}
              onclick={() => {
                const rows = csv?.rows ?? [];
                void addEpisodes(rows, csvIds).then((ok) => {
                  if (ok) csvText = '';
                });
              }}>등록</Button
            >
          </div>
        </div>
      </details>
    {/if}
  </section>

  {#if participants.length > 0}
    <section class="flex flex-col gap-2" aria-labelledby="participants-heading">
      <h2 id="participants-heading" class="text-lg font-semibold">함께 수강</h2>
      <p class="text-sm text-muted-foreground">
        완료·수강 중·밀린 회차 수만 함께 봅니다. 개인 메모와 시간 기록은 보이지 않습니다.
      </p>
      <ul class="flex flex-col gap-1 text-sm">
        {#each participants as person (person.user_id)}
          <li>
            {person.display_name}: 완료 {person.done} / {c.episodes.length} · 수강 중 {person.in_progress}
            {#if person.overdue > 0}· <span class="text-destructive">밀림 {person.overdue}</span
              >{/if}
            · {durationText(person.done_seconds)}
          </li>
        {/each}
      </ul>
    </section>
  {/if}
{/if}
