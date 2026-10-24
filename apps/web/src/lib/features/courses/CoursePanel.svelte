<script lang="ts">
  // 강의 모듈의 강좌 화면(D-37): 강좌 정보, 진도 요약, 회차 목록과 내 진도, 회차 추가·CSV 일괄 등록, 함께
  // 수강하는 사람의 진도 집계. 완료는 직접 표시한다. 회차마다 강의 시간 측정을 시작할 수 있다(D-32).
  import { tick } from 'svelte';

  import type { CourseView, EpisodeStatus, EpisodeView, ParticipantView } from '$lib/api/client';
  import { api, ApiError, newRequestId } from '$lib/api/client';
  import { RetryKey } from '$lib/api/retry-key';
  import { errorText } from '$lib/api/errors';
  import { formatDay } from '$lib/display';
  import { localTimeZone, wallTime } from '$lib/features/events/time';
  import { clockText, durationText } from '$lib/features/time/format';
  import { timer } from '$lib/features/time/timer.svelte';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';
  import { Textarea } from '$lib/ui/textarea';
  import { isWebUrl } from '$lib/url';

  import type { EpisodeRow } from './csv';
  import { MAX_SECONDS, parseDuration, parseEpisodesCsv } from './csv';
  import { courseProgress, daysUntil, plannedOn } from './progress';

  interface Props {
    itemId: string;
  }

  const { itemId }: Props = $props();

  const zone = localTimeZone();
  const today = wallTime(new Date().toISOString(), zone).slice(0, 10);

  function statusLabel(status: EpisodeStatus): string {
    switch (status) {
      case 'not_started':
        return m.courses_status_not_started();
      case 'in_progress':
        return m.courses_status_in_progress();
      case 'done':
        return m.courses_status_done();
    }
  }

  /** 날짜만 있는 값을 보일 글자로. 없으면 빈 글자. */
  const dayText = (ymd: string | null): string =>
    ymd === null || ymd === '' ? '' : formatDay(ymd);

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
  /** 강좌 정보 입력 칸. 저장하는 사이 더 고친 칸이 있는지 볼 때 쓴다. */
  const detailKeys = [
    'subject',
    'instructor',
    'platform',
    'url',
    'textbook',
    'starts_on',
    'expires_on',
    'target_on',
  ] as const;
  let openEpisode = $state<string | undefined>(undefined);
  let mine = $state({ planned_on: '', position: '', note: '' });
  /** 시청 위치 입력 오류(칸 옆에 보인다, D-82). */
  let positionError = $state<string | undefined>(undefined);
  let newEpisode = $state({ title: '', chapter: '', duration: '', planned_on: '' });
  let csvText = $state('');
  /** 공통 정보(번호·제목·링크·자료 등)를 고치는 회차와, 고치기를 열 때(또는 마지막으로 확인할 때) 읽은 그 회차. */
  let editingEpisode = $state<EpisodeView | undefined>(undefined);
  /**
   * 지우기를 한 번 더 묻는 회차. 회차를 지우면 서버가 모든 참여자의 그 회차 진도와 개인 메모(본인만 읽는다)까지
   * 지우고 되돌릴 수 없어, 한 번에 지우지 않고 그 영향을 알린 뒤 지운다.
   */
  let confirmingDelete = $state<string | undefined>(undefined);
  let confirmDeleteButton = $state<HTMLButtonElement | null>(null);

  async function askDelete(id: string): Promise<void> {
    confirmingDelete = id;
    await tick();
    confirmDeleteButton?.focus();
  }

  /** 지우기를 그만둔다. 묻는 칸이 초점과 함께 사라지므로 그 회차의 지우기 단추로 초점을 돌려준다. */
  async function cancelDelete(id: string): Promise<void> {
    confirmingDelete = undefined;
    await tick();
    document.getElementById(`episode-delete-${id}`)?.focus();
  }
  // 번호 칸(type="number")은 숫자를 주고, 비우면 null을 준다.
  let shared = $state<{
    number: number | null;
    title: string;
    chapter: string;
    duration: string;
    url: string;
    materials: string;
    planned_on: string;
  }>({
    number: null,
    title: '',
    chapter: '',
    duration: '',
    url: '',
    materials: '',
    planned_on: '',
  });
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
        if (mine === seq) error = errorText(e, m.courses_load_failed());
      });
  });

  /** 처리하고 다시 읽는다. 성공하면 true(실패하면 오류를 보이고 false). */
  async function run(action: () => Promise<unknown>, done: string): Promise<boolean> {
    error = undefined;
    try {
      await action();
      status = done;
      reloadKey += 1;
      return true;
    } catch (e: unknown) {
      error = errorText(e, m.courses_action_failed());
      return false;
    }
  }

  let savingDetails = $state(false);
  // 강좌 정보 고치기를 연 차례. 저장을 기다리는 사이 취소하고 다시 열면, 앞 저장의 응답이 새로 연 폼을 닫지 않게 한다.
  let editSession = 0;

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
    editSession += 1;
    editing = true;
  }

  const orNull = (v: string): string | null => (v === '' ? null : v);

  /** 서버의 지금 내 진도. 다른 탭·기기에서 바꾼 메모·위치·상태를 옛 화면 값으로 덮지 않게 바꾸기 직전에 읽는다. */
  async function freshMine(e: EpisodeView): Promise<EpisodeView['mine']> {
    const now = await api.course(itemId);
    return now.episodes.find((x) => x.id === e.id)?.mine ?? e.mine;
  }

  function setStatus(e: EpisodeView, next: EpisodeStatus): void {
    void run(
      async () => {
        const fresh = await freshMine(e);
        await api.setEpisodeProgress(e.id, {
          status: next,
          planned_on: fresh.planned_on,
          last_position_seconds: fresh.last_position_seconds,
          note: fresh.note,
          today,
        });
      },
      m.courses_episode_status({ number: e.number, status: statusLabel(next) }),
    );
  }

  function open(e: EpisodeView): void {
    if (openEpisode === e.id) {
      openEpisode = undefined;
      return;
    }
    openEpisode = e.id;
    positionError = undefined;
    mine = {
      planned_on: e.mine.planned_on ?? '',
      position:
        e.mine.last_position_seconds === null ? '' : clockText(e.mine.last_position_seconds),
      note: e.mine.note,
    };
  }

  function editEpisode(e: EpisodeView): void {
    if (editingEpisode?.id === e.id) {
      editingEpisode = undefined;
      return;
    }
    editingEpisode = e;
    shared = {
      number: e.number,
      title: e.title,
      chapter: e.chapter,
      duration: e.duration_seconds > 0 ? clockText(e.duration_seconds) : '',
      url: e.url,
      materials: e.materials,
      planned_on: e.planned_on ?? '',
    };
  }

  /**
   * 회차 공통 정보 저장. 읽은 판을 함께 보내 서버가 그 판일 때만 바꾼다(확인과 바꾸기가 한 문장). 고치기를 연 뒤 다른
   * 사람이 먼저 고쳤으면 409: 입력은 그대로 두고 기준만 지금 판으로 바꿔 알린다. 다시 저장하면(명시적으로) 바꾼다.
   */
  let savingEpisode = $state(false);

  async function saveEpisode(base: EpisodeView): Promise<void> {
    const number = shared.number;
    const seconds = parseDuration(shared.duration);
    const url = shared.url.trim();
    if (
      number === null ||
      !Number.isInteger(number) ||
      number < 1 ||
      shared.title.trim() === '' ||
      seconds === null
    ) {
      error = m.courses_episode_invalid();
      return;
    }
    if (url !== '' && !isWebUrl(url)) {
      error = m.courses_url_invalid();
      return;
    }
    // 저장 중에 다시 누르면(Enter 두 번 등) 같은 판으로 두 번 보내, 내 첫 저장 때문에 두 번째가 409를 받고 "다른 사람이
    // 먼저 고쳤다"고 잘못 알린다. 끝날 때까지 다시 보내지 않는다.
    if (savingEpisode) return;
    savingEpisode = true;
    error = undefined;
    try {
      await api.updateEpisode(base.id, base.version, {
        number,
        title: shared.title.trim(),
        chapter: shared.chapter.trim(),
        duration_seconds: seconds,
        url,
        materials: shared.materials,
        planned_on: orNull(shared.planned_on),
      });
      editingEpisode = undefined;
      status = m.courses_episode_saved({ number });
      reloadKey += 1;
    } catch (e: unknown) {
      if (e instanceof ApiError && e.status === 409 && (await episodeMovedOn(base))) return;
      error = errorText(e, m.courses_action_failed());
    } finally {
      savingEpisode = false;
    }
  }

  /**
   * 409를 받은 뒤 지금 회차를 다시 읽는다. 다른 사람이 먼저 고쳐 판이 바뀌었으면(보관으로 막힌 409와 구분) 기준을 지금
   * 판으로 바꾸고 알린다(입력은 그대로). 회차가 지워졌으면 고치기를 닫고 알린다. 처리했으면 true.
   */
  async function episodeMovedOn(base: EpisodeView): Promise<boolean> {
    let now: CourseView;
    try {
      now = await api.course(itemId);
    } catch {
      return false;
    }
    course = now;
    const latest = now.episodes.find((x) => x.id === base.id);
    if (latest === undefined) {
      editingEpisode = undefined;
      error = m.common_error_not_found();
      return true;
    }
    if (latest.version === base.version) return false;
    editingEpisode = latest;
    error = m.courses_episode_changed();
    return true;
  }

  const csv = $derived(csvText.trim() === '' ? undefined : parseEpisodesCsv(csvText));

  // 요청 id는 입력을 시작할 때 정하고 성공할 때까지 지킨다. 응답을 잃고 다시 눌러도 서버가 같은 회차로 본다.
  // 한 회차 추가: 같은 입력의 재시도는 같은 요청 id, 고친 입력은 새 id.
  const draftKey = new RetryKey();
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
      error = errorText(e, m.courses_action_failed());
      return false;
    }
    const after = await api.course(itemId);
    course = after;
    reloadKey += 1;
    const now = courseProgress(after.episodes, today, after.details.target_on).percentByCount;
    // 목차가 늘면 진행률이 바뀐다는 것을 알린다(D-37).
    status = m.courses_added({ count: added, before, after: now });
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
      <h2 id="course-info-heading" class="text-lg font-semibold">{m.courses_info()}</h2>
      {#if c.can_write && !editing}
        <Button
          size="sm"
          variant="outline"
          onclick={() => {
            startEdit(c);
          }}>{m.courses_edit()}</Button
        >
      {/if}
    </div>
    {#if editing}
      <form
        class="grid gap-2 sm:grid-cols-2"
        onsubmit={(e: SubmitEvent) => {
          e.preventDefault();
          if (details.url.trim() !== '' && !isWebUrl(details.url.trim())) {
            error = m.courses_url_invalid();
            return;
          }
          // 저장에 성공한 뒤에만 닫는다. 실패하면(보관됨·권한·연결) 입력을 그대로 두고 알린다.
          if (savingDetails) return;
          savingDetails = true;
          // 보낸 입력. 응답을 기다리는 사이 더 고친 칸이 있으면 닫지 않는다(보내지 않은 입력이 사라지지 않게).
          const sent = { ...details };
          const session = editSession;
          void run(
            () =>
              api.setCourseDetails(itemId, {
                subject: sent.subject,
                instructor: sent.instructor,
                platform: sent.platform,
                url: sent.url,
                textbook: sent.textbook,
                starts_on: orNull(sent.starts_on),
                expires_on: orNull(sent.expires_on),
                target_on: orNull(sent.target_on),
              }),
            m.courses_details_saved(),
          ).then((ok) => {
            savingDetails = false;
            if (
              ok &&
              session === editSession &&
              detailKeys.every((key) => details[key] === sent[key])
            )
              editing = false;
          });
        }}
      >
        {#each [['subject', m.courses_subject()], ['instructor', m.courses_instructor()], ['platform', m.courses_platform()], ['url', m.courses_url()], ['textbook', m.courses_textbook()]] as const as [key, label] (key)}
          <div class="flex flex-col gap-1">
            <Label for={`course-${key}`}>{label}</Label>
            <Input id={`course-${key}`} bind:value={details[key]} maxlength={400} />
          </div>
        {/each}
        {#each [['starts_on', m.courses_starts_on()], ['expires_on', m.courses_expires_on()], ['target_on', m.courses_target_on()]] as const as [key, label] (key)}
          <div class="flex flex-col gap-1">
            <Label for={`course-${key}`}>{label}</Label>
            <Input id={`course-${key}`} type="date" bind:value={details[key]} />
          </div>
        {/each}
        <div class="flex gap-2 sm:col-span-2">
          <Button type="submit" disabled={savingDetails}>{m.common_save()}</Button>
          <Button variant="ghost" onclick={() => (editing = false)}>{m.common_cancel()}</Button>
        </div>
      </form>
    {:else}
      <dl class="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-sm">
        {#each [[m.courses_subject(), c.details.subject], [m.courses_instructor(), c.details.instructor], [m.courses_platform(), c.details.platform], [m.courses_textbook(), c.details.textbook], [m.courses_starts_on(), dayText(c.details.starts_on)], [m.courses_expires_on(), dayText(c.details.expires_on)], [m.courses_target_on(), dayText(c.details.target_on)]] as [label, value] (label)}
          {#if value !== ''}
            <dt class="text-muted-foreground">{label}</dt>
            <dd>{value}</dd>
          {/if}
        {/each}
        {#if isWebUrl(c.details.url)}
          <dt class="text-muted-foreground">{m.courses_url()}</dt>
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
          ? m.courses_expired()
          : m.courses_expires_in({ count: daysUntil(today, c.details.expires_on) })}
      </p>
    {/if}
  </section>

  <section class="flex flex-col gap-2" aria-labelledby="course-progress-heading">
    <h2 id="course-progress-heading" class="text-lg font-semibold">{m.courses_my_progress()}</h2>
    <div class="grid gap-2 text-sm sm:grid-cols-3">
      <div class="rounded-md border p-2">
        <div class="text-muted-foreground">{m.courses_progress()}</div>
        <div class="text-lg font-semibold">
          {m.courses_percent({ byCount: p.percentByCount, byDuration: p.percentByDuration })}
        </div>
        <div>
          {m.courses_counts({
            done: p.done,
            inProgress: p.inProgress,
            remaining: p.remaining,
            total: p.total,
          })}
        </div>
      </div>
      <div class="rounded-md border p-2">
        <div class="text-muted-foreground">{m.courses_today()}</div>
        <div class="text-lg font-semibold">
          {m.courses_today_counts({ planned: p.todayPlanned, done: p.todayDone })}
        </div>
        <div>{m.courses_overdue_count({ count: p.overdue.length })}</div>
      </div>
      <div class="rounded-md border p-2">
        <div class="text-muted-foreground">{m.courses_remaining()}</div>
        <div class="text-lg font-semibold">{durationText(p.remainingSeconds)}</div>
        {#if p.perDayEpisodes !== null && p.perDaySeconds !== null}
          <div>
            {m.courses_per_day({
              count: p.perDayEpisodes,
              duration: durationText(p.perDaySeconds),
            })}
          </div>
        {/if}
      </div>
    </div>
    {#if p.overdue.length > 0}
      <p class="text-sm">
        {m.courses_overdue_list({
          list: p.overdue.map((e) => m.courses_episode_number({ number: e.number })).join(', '),
        })}
      </p>
    {/if}
  </section>

  <section class="flex flex-col gap-2" aria-labelledby="episodes-heading">
    <h2 id="episodes-heading" class="text-lg font-semibold">{m.courses_episodes()}</h2>
    {#if c.episodes.length === 0}
      <p class="text-sm text-muted-foreground">{m.courses_episodes_empty()}</p>
    {/if}
    <ol class="flex flex-col gap-1 text-sm" data-testid="episodes">
      {#each c.episodes as e (e.id)}
        {@const planned = plannedOn(e)}
        <li class="flex flex-col gap-1 rounded-md border p-2">
          <div class="flex flex-wrap items-center gap-2">
            <span class="w-10 shrink-0 tabular-nums text-muted-foreground"
              >{m.courses_episode_number({ number: e.number })}</span
            >
            {#if e.chapter !== ''}<span class="text-xs text-muted-foreground">{e.chapter}</span
              >{/if}
            {#if isWebUrl(e.url)}
              <!-- 제목은 자르지 않고 줄을 바꾼다. 좁으면 시간·날짜·상태가 다음 줄로 내려간다. -->
              <a
                class="min-w-40 flex-1 underline"
                href={e.url}
                target="_blank"
                rel="noopener noreferrer">{e.title}</a
              >
            {:else}
              <span class="min-w-40 flex-1">{e.title}</span>
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
                ]}>{dayText(planned)}</span
              >
            {/if}
            <span class="text-xs"
              >{statusLabel(e.mine.status)}{e.mine.watch_count > 1
                ? ` · ${m.courses_watch_count({ count: e.mine.watch_count })}`
                : ''}</span
            >
          </div>
          <div class="flex flex-wrap gap-1">
            {#if e.mine.status !== 'done'}
              <Button
                size="sm"
                variant="outline"
                aria-label={m.courses_mark_done_aria({ number: e.number })}
                onclick={() => {
                  setStatus(e, 'done');
                }}>{m.courses_mark_done()}</Button
              >
            {:else}
              <Button
                size="sm"
                variant="ghost"
                aria-label={m.courses_rewatch_aria({ number: e.number })}
                onclick={() => {
                  setStatus(e, 'in_progress');
                }}>{m.courses_rewatch()}</Button
              >
            {/if}
            {#if e.mine.status === 'not_started'}
              <Button
                size="sm"
                variant="ghost"
                aria-label={m.courses_watching_aria({ number: e.number })}
                onclick={() => {
                  setStatus(e, 'in_progress');
                }}>{m.courses_watching()}</Button
              >
            {/if}
            <Button
              size="sm"
              variant="ghost"
              aria-label={m.courses_track_aria({ number: e.number })}
              onclick={() =>
                void timer
                  .start('lecture', { kind: 'episode', id: e.id })
                  .then(() => {
                    status = m.courses_track_started({ number: e.number });
                  })
                  .catch(() => {
                    error = m.courses_timer_busy();
                  })}>{m.courses_track()}</Button
            >
            <Button
              size="sm"
              variant="ghost"
              aria-expanded={openEpisode === e.id}
              aria-label={m.courses_notes_aria({ number: e.number })}
              onclick={() => {
                open(e);
              }}>{m.courses_notes()}</Button
            >
            {#if c.can_write}
              <Button
                size="sm"
                variant="ghost"
                aria-expanded={editingEpisode?.id === e.id}
                aria-label={m.courses_edit_episode_aria({ number: e.number })}
                onclick={() => {
                  editEpisode(e);
                }}>{m.courses_edit()}</Button
              >
              {#if confirmingDelete !== e.id}
                <Button
                  id={`episode-delete-${e.id}`}
                  size="sm"
                  variant="ghost"
                  aria-label={m.courses_delete_aria({ number: e.number })}
                  onclick={() => void askDelete(e.id)}>{m.courses_delete()}</Button
                >
              {:else}
                <span class="flex flex-wrap items-center gap-1 text-sm">
                  <span id={`episode-delete-warning-${e.id}`}
                    >{m.courses_delete_confirm({ number: e.number })}</span
                  >
                  <Button
                    size="sm"
                    variant="destructive"
                    bind:ref={confirmDeleteButton}
                    aria-label={m.courses_delete_aria({ number: e.number })}
                    aria-describedby={`episode-delete-warning-${e.id}`}
                    onclick={() => {
                      confirmingDelete = undefined;
                      void run(
                        () => api.deleteEpisode(e.id),
                        m.courses_deleted({ number: e.number }),
                      );
                    }}>{m.courses_delete()}</Button
                  >
                  <Button size="sm" variant="ghost" onclick={() => void cancelDelete(e.id)}
                    >{m.common_cancel()}</Button
                  >
                </span>
              {/if}
            {/if}
          </div>
          {#if e.materials !== ''}
            <p class="text-xs whitespace-pre-wrap text-muted-foreground">
              <span class="font-medium">{m.courses_materials()}</span>
              {e.materials}
            </p>
          {/if}
          {#if editingEpisode?.id === e.id}
            {@const base = editingEpisode}
            <form
              class="grid gap-2 border-t pt-2 sm:grid-cols-4"
              aria-label={m.courses_episode_form({ number: e.number })}
              onsubmit={(ev: SubmitEvent) => {
                ev.preventDefault();
                void saveEpisode(base);
              }}
            >
              <div class="flex flex-col gap-1">
                <Label for={`ep-number-${e.id}`}>{m.courses_number()}</Label>
                <Input id={`ep-number-${e.id}`} type="number" min={1} bind:value={shared.number} />
              </div>
              <div class="flex flex-col gap-1 sm:col-span-3">
                <Label for={`ep-title-${e.id}`}>{m.courses_new_title()}</Label>
                <Input id={`ep-title-${e.id}`} bind:value={shared.title} maxlength={300} />
              </div>
              <div class="flex flex-col gap-1">
                <Label for={`ep-chapter-${e.id}`}>{m.courses_chapter()}</Label>
                <Input id={`ep-chapter-${e.id}`} bind:value={shared.chapter} />
              </div>
              <div class="flex flex-col gap-1">
                <Label for={`ep-duration-${e.id}`}>{m.courses_duration()}</Label>
                <Input
                  id={`ep-duration-${e.id}`}
                  bind:value={shared.duration}
                  placeholder="12:30"
                />
              </div>
              <div class="flex flex-col gap-1 sm:col-span-2">
                <Label for={`ep-planned-${e.id}`}>{m.courses_planned()}</Label>
                <Input id={`ep-planned-${e.id}`} type="date" bind:value={shared.planned_on} />
              </div>
              <div class="flex flex-col gap-1 sm:col-span-4">
                <Label for={`ep-url-${e.id}`}>{m.courses_episode_url()}</Label>
                <Input id={`ep-url-${e.id}`} type="url" bind:value={shared.url} maxlength={2000} />
              </div>
              <div class="flex flex-col gap-1 sm:col-span-4">
                <Label for={`ep-materials-${e.id}`}>{m.courses_materials()}</Label>
                <Textarea
                  id={`ep-materials-${e.id}`}
                  class="min-h-20"
                  bind:value={shared.materials}
                />
              </div>
              <div class="flex gap-2 sm:col-span-4">
                <Button type="submit" size="sm" disabled={savingEpisode}>{m.common_save()}</Button>
                <Button
                  size="sm"
                  variant="ghost"
                  onclick={() => {
                    editingEpisode = undefined;
                  }}>{m.common_cancel()}</Button
                >
              </div>
            </form>
          {/if}
          {#if openEpisode === e.id}
            <form
              class="grid gap-2 border-t pt-2 sm:grid-cols-3"
              onsubmit={(ev: SubmitEvent) => {
                ev.preventDefault();
                // 빈칸은 위치 지우기(null), 그 밖은 분:초·시:분:초·분 수(CSV와 같은 규칙). 읽을 수 없는 입력은 보내지
                // 않고 칸 옆에 알리며 저장된 값은 그대로 둔다(전에는 'abc'가 0초, '12:xx'가 지우기로 저장됐다).
                const raw = mine.position.trim();
                const position = raw === '' ? null : parseDuration(raw);
                // 서버와 같은 한도(100시간, i32)를 넘는 값도 보내지 않는다.
                if (raw !== '' && (position === null || position > MAX_SECONDS)) {
                  positionError = m.courses_position_invalid();
                  return;
                }
                positionError = undefined;
                void run(
                  async () =>
                    api.setEpisodeProgress(e.id, {
                      status: (await freshMine(e)).status,
                      planned_on: orNull(mine.planned_on),
                      last_position_seconds: position,
                      note: mine.note,
                      today,
                    }),
                  m.courses_mine_saved({ number: e.number }),
                );
              }}
            >
              <div class="flex flex-col gap-1">
                <Label for={`mine-planned-${e.id}`}>{m.courses_my_planned()}</Label>
                <Input id={`mine-planned-${e.id}`} type="date" bind:value={mine.planned_on} />
              </div>
              <div class="flex flex-col gap-1">
                <Label for={`mine-position-${e.id}`}>{m.courses_last_position()}</Label>
                <Input
                  id={`mine-position-${e.id}`}
                  bind:value={mine.position}
                  placeholder="12:30"
                  oninput={() => (positionError = undefined)}
                  aria-invalid={positionError !== undefined}
                  aria-describedby={positionError === undefined
                    ? undefined
                    : `mine-position-error-${e.id}`}
                />
                {#if positionError !== undefined}
                  <p
                    id={`mine-position-error-${e.id}`}
                    class="text-xs text-destructive"
                    role="alert"
                  >
                    {positionError}
                  </p>
                {/if}
              </div>
              <div class="flex flex-col gap-1 sm:col-span-3">
                <Label for={`mine-note-${e.id}`}>{m.courses_my_note()}</Label>
                <Textarea id={`mine-note-${e.id}`} class="min-h-20" bind:value={mine.note} />
              </div>
              <div><Button type="submit" size="sm">{m.common_save()}</Button></div>
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
            error = m.courses_new_invalid();
            return;
          }
          const row = {
            title: newEpisode.title.trim(),
            chapter: newEpisode.chapter.trim(),
            duration_seconds: seconds,
            planned_on: orNull(newEpisode.planned_on),
          };
          const requestId = draftKey.for({ itemId, row });
          void addEpisodes([row], [requestId]).then((ok) => {
            if (!ok) return;
            newEpisode = { title: '', chapter: '', duration: '', planned_on: '' };
            draftKey.done(requestId);
          });
        }}
      >
        <div class="flex min-w-48 flex-1 flex-col gap-1">
          <Label for="episode-title">{m.courses_new_title()}</Label>
          <Input id="episode-title" bind:value={newEpisode.title} maxlength={300} />
        </div>
        <div class="flex flex-col gap-1">
          <Label for="episode-chapter">{m.courses_chapter()}</Label>
          <Input id="episode-chapter" bind:value={newEpisode.chapter} />
        </div>
        <div class="flex w-24 flex-col gap-1">
          <Label for="episode-duration">{m.courses_duration()}</Label>
          <Input id="episode-duration" bind:value={newEpisode.duration} placeholder="12:30" />
        </div>
        <div class="flex flex-col gap-1">
          <Label for="episode-planned">{m.courses_planned()}</Label>
          <Input id="episode-planned" type="date" bind:value={newEpisode.planned_on} />
        </div>
        <Button type="submit">{m.courses_add_episode()}</Button>
      </form>
      <details class="text-sm">
        <summary class="cursor-pointer">{m.courses_csv_summary()}</summary>
        <div class="flex flex-col gap-2 pt-2">
          <Label for="episodes-csv">{m.courses_csv_help()}</Label>
          <Textarea
            id="episodes-csv"
            class="max-h-96 min-h-32 overflow-y-auto font-code"
            bind:value={csvText}
          />
          {#if csv !== undefined}
            <p aria-live="polite">
              {m.courses_csv_read({ count: csv.rows.length })}
              {#if csv.errors.length > 0}
                <span class="text-destructive">
                  {m.courses_csv_fix({
                    lines: csv.errors
                      .map((er) => m.courses_csv_line({ line: er.line, reason: er.reason }))
                      .join(', '),
                  })}
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
              }}>{m.courses_csv_add()}</Button
            >
          </div>
        </div>
      </details>
    {/if}
  </section>

  {#if participants.length > 0}
    <section class="flex flex-col gap-2" aria-labelledby="participants-heading">
      <h2 id="participants-heading" class="text-lg font-semibold">{m.courses_participants()}</h2>
      <p class="text-sm text-muted-foreground">{m.courses_participants_hint()}</p>
      <ul class="flex flex-col gap-1 text-sm">
        {#each participants as person (person.user_id)}
          <li>
            {m.courses_participant({
              name: person.display_name,
              done: person.done,
              total: c.episodes.length,
              inProgress: person.in_progress,
            })}
            {#if person.overdue > 0}· <span class="text-destructive"
                >{m.courses_participant_overdue({ count: person.overdue })}</span
              >{/if}
            · {durationText(person.done_seconds)}
          </li>
        {/each}
      </ul>
    </section>
  {/if}
{/if}
