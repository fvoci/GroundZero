<script lang="ts">
  // 강의 모듈의 강좌 화면(D-37): 강좌 정보, 진도 요약, 회차 목록과 내 진도, 회차 추가·CSV 일괄 등록, 함께
  // 수강하는 사람의 진도 집계. 완료는 직접 표시한다. 회차마다 강의 시간 측정을 시작할 수 있다(D-32).
  import { onMount, tick, untrack } from 'svelte';

  import type { CourseView, EpisodeStatus, EpisodeView, ParticipantView } from '$lib/api/client';
  import { api, ApiError, newRequestId } from '$lib/api/client';
  import { createFormDraft } from '$lib/features/common/form-draft.svelte';
  import FormDraftOffer from '$lib/features/common/FormDraftOffer.svelte';
  import DiscardChangesConfirm from '$lib/features/common/DiscardChangesConfirm.svelte';
  import { accountSwitching, notifyDone } from '$lib/features/feedback/feedback.svelte';
  import { me } from '$lib/me.svelte';
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
  let timerBusy = $state(false);
  async function startEpisode(id: string): Promise<void> {
    if (timerBusy) return;
    timerBusy = true;
    error = undefined;
    try {
      await timer.start('lecture', { kind: 'episode', id });
    } catch (e: unknown) {
      error = errorText(e, m.time_start_failed());
    } finally {
      timerBusy = false;
    }
  }
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
  // 숫자 원문은 화면에서 유지하고 제출할 때만 숫자로 바꾼다.
  let shared = $state<{
    number: string;
    title: string;
    chapter: string;
    duration: string;
    url: string;
    materials: string;
    planned_on: string;
  }>({
    number: '',
    title: '',
    chapter: '',
    duration: '',
    url: '',
    materials: '',
    planned_on: '',
  });
  let reloadKey = $state(0);
  let seq = 0;
  let owner = $state(me.userId);
  let loadedItem = '';
  let detailsBase = $state('');
  let sharedBase = $state('');
  let mineBase = $state('');
  let baseVersion = $state('');
  let newId = $state(newRequestId());
  let newPayload = $state('');
  let csvIds = $state<string[]>([]);
  let csvSentText = $state('');
  let adding = $state(false);
  let savingMine = $state(false);

  // All values stay strings until submit, including invalid numbers and durations.
  function textFields<T extends Record<string, string>>(raw: unknown, shape: T): T | undefined {
    const valid = (value: unknown): value is T =>
      typeof value === 'object' &&
      value !== null &&
      Object.keys(shape).every(
        (key) => key in value && typeof Reflect.get(value, key) === 'string',
      );
    return valid(raw) ? raw : undefined;
  }
  function resetDetails(): void {
    const d = course?.details;
    details = {
      subject: d?.subject ?? '',
      instructor: d?.instructor ?? '',
      platform: d?.platform ?? '',
      url: d?.url ?? '',
      textbook: d?.textbook ?? '',
      starts_on: d?.starts_on ?? '',
      expires_on: d?.expires_on ?? '',
      target_on: d?.target_on ?? '',
    };
    detailsBase = JSON.stringify(details);
  }
  function resetShared(): void {
    const e = editingEpisode;
    shared = {
      number: e === undefined ? '' : String(e.number),
      title: e?.title ?? '',
      chapter: e?.chapter ?? '',
      duration: e?.duration_seconds ? clockText(e.duration_seconds) : '',
      url: e?.url ?? '',
      materials: e?.materials ?? '',
      planned_on: e?.planned_on ?? '',
    };
    sharedBase = JSON.stringify(shared);
    baseVersion = String(e?.version ?? 0);
  }
  // 저장하는 기준값에서도 긴 작성 입력만 꺼낸다(D-27 개정).
  function baselineText(base: string, key: string): string {
    try {
      const value: unknown = JSON.parse(base);
      if (typeof value !== 'object' || value === null) return '';
      const text: unknown = Reflect.get(value, key);
      return typeof text === 'string' ? text : '';
    } catch {
      return '';
    }
  }
  function resetMine(): void {
    const e = course?.episodes.find((entry) => entry.id === openEpisode);
    mine = {
      planned_on: e?.mine.planned_on ?? '',
      position:
        e?.mine.last_position_seconds == null ? '' : clockText(e.mine.last_position_seconds),
      note: e?.mine.note ?? '',
    };
    mineBase = JSON.stringify(mine);
    positionError = undefined;
  }
  const detailsDraft = createFormDraft({
    preserve: false,
    name: () => `course:${itemId}:details`,
    snapshot: () => ({ ...details, base: detailsBase }),
    parse: (raw) => textFields(raw, { ...details, base: '' }),
    restore: ({ base, ...input }) => {
      details = input;
      detailsBase = base;
      editing = true;
    },
    reset: resetDetails,
    dirty: () => editing && JSON.stringify(details) !== detailsBase,
    busy: () => savingDetails,
  });
  const episodeDraft = createFormDraft({
    name: () =>
      editingEpisode === undefined
        ? undefined
        : `course:${itemId}:episode:${editingEpisode.id}:edit`,
    snapshot: () => ({ ...shared, base: sharedBase, version: baseVersion }),
    storedSnapshot: () => ({
      number: '',
      title: '',
      chapter: '',
      duration: '',
      url: '',
      planned_on: '',
      materials: shared.materials,
      base: JSON.stringify({ materials: baselineText(sharedBase, 'materials') }),
      version: baseVersion,
    }),
    parse: (raw) => textFields(raw, { ...shared, base: '', version: '' }),
    restore: ({ base, version, materials }) => {
      shared = { ...shared, materials };
      sharedBase = JSON.stringify({ ...shared, materials: baselineText(base, 'materials') });
      baseVersion = version;
    },
    reset: resetShared,
    dirty: () =>
      editingEpisode !== undefined && shared.materials !== baselineText(sharedBase, 'materials'),
    screenOnly: () =>
      editingEpisode !== undefined &&
      ['number', 'title', 'chapter', 'duration', 'url', 'planned_on'].some(
        (key) => Reflect.get(shared, key) !== baselineText(sharedBase, key),
      ),
    busy: () => savingEpisode,
  });
  const mineDraft = createFormDraft({
    name: () =>
      openEpisode === undefined ? undefined : `course:${itemId}:episode:${openEpisode}:mine`,
    snapshot: () => ({ ...mine, base: mineBase }),
    storedSnapshot: () => ({
      planned_on: '',
      position: '',
      note: mine.note,
      base: JSON.stringify({ note: baselineText(mineBase, 'note') }),
    }),
    parse: (raw) => textFields(raw, { ...mine, base: '' }),
    restore: ({ base, note }) => {
      mine = { ...mine, note };
      mineBase = JSON.stringify({ ...mine, note: baselineText(base, 'note') });
    },
    reset: resetMine,
    // D-113 SR1-11: 읽기 전용도 개인 진도는 남긴다. 보관 상태에서는 읽기만 한다.
    dirty: () =>
      course !== undefined &&
      course.access !== 'archived' &&
      openEpisode !== undefined &&
      mine.note !== baselineText(mineBase, 'note'),
    screenOnly: () =>
      course !== undefined &&
      course.access !== 'archived' &&
      openEpisode !== undefined &&
      ['planned_on', 'position'].some(
        (key) => Reflect.get(mine, key) !== baselineText(mineBase, key),
      ),
    busy: () => savingMine,
  });
  const newDraft = createFormDraft({
    preserve: false,
    name: () => `course:${itemId}:new`,
    snapshot: () => ({ ...newEpisode, requestId: newId, requestPayload: newPayload }),
    parse: (raw) => textFields(raw, { ...newEpisode, requestId: '', requestPayload: '' }),
    restore: ({ requestId, requestPayload, ...input }) => {
      newEpisode = input;
      newId = requestId;
      newPayload = requestPayload;
    },
    reset: () => {
      newEpisode = { title: '', chapter: '', duration: '', planned_on: '' };
      newId = newRequestId();
      newPayload = '';
    },
    dirty: () => Object.values(newEpisode).some((value) => value !== ''),
    busy: () => adding,
  });
  const csvDraft = createFormDraft({
    name: () => `course:${itemId}:import`,
    snapshot: () => ({ text: csvText, sentText: csvSentText, ids: [...csvIds] }),
    parse: (raw) => {
      if (
        typeof raw !== 'object' ||
        raw === null ||
        !('text' in raw) ||
        typeof raw.text !== 'string' ||
        !('sentText' in raw) ||
        typeof raw.sentText !== 'string' ||
        !('ids' in raw) ||
        !Array.isArray(raw.ids) ||
        !raw.ids.every((id): id is string => typeof id === 'string')
      )
        return undefined;
      return { text: raw.text, sentText: raw.sentText, ids: raw.ids };
    },
    restore: (input) => {
      csvText = input.text;
      csvSentText = input.sentText;
      csvIds = input.ids;
    },
    reset: () => {
      csvText = '';
      csvSentText = '';
      csvIds = [];
    },
    dirty: () => csvText !== '',
    busy: () => adding,
  });
  const drafts = [detailsDraft, episodeDraft, mineDraft, newDraft, csvDraft];
  onMount(() => {
    const detach = drafts.map((draft) => draft.attach());
    return () => {
      detach.forEach((remove) => {
        remove();
      });
    };
  });

  const progress = $derived(
    course === undefined
      ? undefined
      : courseProgress(course.episodes, today, course.details.target_on),
  );

  $effect(() => {
    const id = itemId;
    const userId = me.userId;
    const switching = accountSwitching();
    if (reloadKey < 0) return;
    untrack(() => {
      if (owner !== userId || loadedItem !== id) {
        owner = userId;
        loadedItem = id;
        course = undefined;
        participants = [];
        editing = false;
        editingEpisode = undefined;
        openEpisode = undefined;
        savingDetails = false;
        savingEpisode = false;
        savingMine = false;
        adding = false;
      }
    });
    const mine = ++seq;
    if (switching) return;
    Promise.all([api.course(id), api.courseParticipants(id, today)])
      .then(([c, p]) => {
        if (mine !== seq) return;
        setCourse(c);
        participants = p;
        error = undefined;
      })
      .catch((e: unknown) => {
        if (mine === seq) error = errorText(e, m.courses_load_failed());
      });
  });

  /**
   * 다시 읽은 강좌를 넣는다. 그사이 쓸 수 없게 됐으면(역할 강등·보관) 연 내 진도 폼을 같은 갱신에서 닫는다. 폼 이름이
   * 먼저 바뀌어야 form-draft가 쓸 수 없어 깨끗해진 폼으로 이 탭의 남긴 입력을 지우지 않는다(다시 열면 남긴 입력으로
   * 고르게 하고, 쓸 수 있을 때 되살린다).
   */
  function setCourse(next: CourseView): void {
    if (course !== undefined && course.access !== 'archived' && next.access === 'archived')
      openEpisode = undefined;
    course = next;
  }

  /** 처리하고 다시 읽는다. 성공하면 true(실패하면 오류를 보이고 false). */
  async function run(
    action: () => Promise<unknown>,
    done: string,
    stillOwner: () => boolean = () => true,
  ): Promise<boolean> {
    error = undefined;
    try {
      await action();
      if (!stillOwner()) return false;
      notifyDone(done);
      reloadKey += 1;
      return true;
    } catch (e: unknown) {
      if (stillOwner()) error = errorText(e, m.courses_action_failed());
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
    detailsBase = JSON.stringify(details);
  }

  const orNull = (v: string): string | null => (v === '' ? null : v);

  /** 서버의 지금 내 진도. 다른 탭·기기에서 바꾼 메모·위치·상태를 옛 화면 값으로 덮지 않게 바꾸기 직전에 읽는다. */
  async function freshMine(e: EpisodeView): Promise<EpisodeView['mine']> {
    const now = await api.course(itemId);
    return now.episodes.find((x) => x.id === e.id)?.mine ?? e.mine;
  }

  function setStatus(e: EpisodeView, next: EpisodeStatus): void {
    if (course === undefined || course.access === 'archived') return;
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
    mineDraft.leave.request(() => {
      selectEpisode(e);
    });
  }
  function selectEpisode(e: EpisodeView): void {
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
    mineBase = JSON.stringify(mine);
  }

  function editEpisode(e: EpisodeView): void {
    episodeDraft.leave.request(() => {
      selectEditEpisode(e);
    });
  }
  function selectEditEpisode(e: EpisodeView): void {
    if (editingEpisode?.id === e.id) {
      editingEpisode = undefined;
      return;
    }
    editingEpisode = e;
    shared = {
      number: String(e.number),
      title: e.title,
      chapter: e.chapter,
      duration: e.duration_seconds > 0 ? clockText(e.duration_seconds) : '',
      url: e.url,
      materials: e.materials,
      planned_on: e.planned_on ?? '',
    };
    sharedBase = JSON.stringify(shared);
    baseVersion = String(e.version);
  }

  /**
   * 회차 공통 정보 저장. 읽은 판을 함께 보내 서버가 그 판일 때만 바꾼다(확인과 바꾸기가 한 문장). 고치기를 연 뒤 다른
   * 사람이 먼저 고쳤으면 409: 입력은 그대로 두고 기준만 지금 판으로 바꿔 알린다. 다시 저장하면(명시적으로) 바꾼다.
   */
  let savingEpisode = $state(false);

  async function saveEpisode(base: EpisodeView): Promise<void> {
    const number = Number(shared.number);
    const seconds = parseDuration(shared.duration);
    const url = shared.url.trim();
    if (
      shared.number.trim() === '' ||
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
    if (savingEpisode || course?.access !== 'write') return;
    const attempt = episodeDraft.capture();
    if (attempt === undefined) return;
    savingEpisode = true;
    error = undefined;
    try {
      await api.updateEpisode(base.id, Number(baseVersion), {
        number,
        title: shared.title.trim(),
        chapter: shared.chapter.trim(),
        duration_seconds: seconds,
        url,
        materials: shared.materials,
        planned_on: orNull(shared.planned_on),
      });
      if (!episodeDraft.isOwner(attempt)) return;
      // Closing this target makes isOwner false, so release its busy state first.
      savingEpisode = false;
      if (episodeDraft.complete(attempt)) editingEpisode = undefined;
      notifyDone(m.courses_episode_saved({ number }));
      reloadKey += 1;
    } catch (e: unknown) {
      if (!episodeDraft.isOwner(attempt)) return;
      if (
        e instanceof ApiError &&
        e.status === 409 &&
        (await episodeMovedOn(base, () => episodeDraft.isOwner(attempt)))
      )
        return;
      error = errorText(e, m.courses_action_failed());
    } finally {
      if (episodeDraft.isOwner(attempt)) savingEpisode = false;
    }
  }

  /**
   * 409를 받은 뒤 지금 회차를 다시 읽는다. 다른 사람이 먼저 고쳐 판이 바뀌었으면(보관으로 막힌 409와 구분) 기준을 지금
   * 판으로 바꾸고 알린다(입력은 그대로). 회차가 지워졌으면 고치기를 닫고 알린다. 처리했으면 true.
   */
  async function episodeMovedOn(base: EpisodeView, stillOwner: () => boolean): Promise<boolean> {
    let now: CourseView;
    try {
      now = await api.course(itemId);
    } catch {
      return false;
    }
    if (!stillOwner()) return true;
    setCourse(now);
    const latest = now.episodes.find((x) => x.id === base.id);
    if (latest === undefined) {
      error = m.common_error_not_found();
      return true;
    }
    if (latest.version === base.version) return false;
    editingEpisode = latest;
    baseVersion = String(latest.version);
    error = m.courses_episode_changed();
    return true;
  }

  const csv = $derived(csvText.trim() === '' ? undefined : parseEpisodesCsv(csvText));

  // 요청 id는 입력을 시작할 때 정하고 성공할 때까지 지킨다. 응답을 잃고 다시 눌러도 서버가 같은 회차로 본다.
  // 한 회차 추가: 같은 입력의 재시도는 같은 요청 id, 고친 입력은 새 id.

  /** 회차를 더한다. 성공하면 true. */
  async function addEpisodes(
    rows: EpisodeRow[],
    ids: readonly string[],
    stillOwner: () => boolean,
  ): Promise<boolean> {
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
      if (stillOwner()) error = errorText(e, m.courses_action_failed());
      return false;
    }
    if (!stillOwner()) return false;
    let after: CourseView;
    try {
      after = await api.course(itemId);
    } catch (e: unknown) {
      if (stillOwner()) error = errorText(e, m.courses_load_failed());
      return false;
    }
    if (!stillOwner()) return false;
    setCourse(after);
    reloadKey += 1;
    const now = courseProgress(after.episodes, today, after.details.target_on).percentByCount;
    // 목차가 늘면 진행률이 바뀐다는 것을 알린다(D-37).
    notifyDone(m.courses_added({ count: added, before, after: now }));
    return true;
  }

  async function saveMine(e: EpisodeView): Promise<void> {
    if (savingMine || course === undefined || course.access === 'archived') return;
    const raw = mine.position.trim();
    const position = raw === '' ? null : parseDuration(raw);
    if (raw !== '' && (position === null || position > MAX_SECONDS)) {
      positionError = m.courses_position_invalid();
      return;
    }
    const attempt = mineDraft.capture();
    if (attempt === undefined) return;
    const sent = { ...mine };
    error = undefined;
    positionError = undefined;
    savingMine = true;
    try {
      const fresh = await freshMine(e);
      if (!mineDraft.isOwner(attempt)) return;
      await api.setEpisodeProgress(e.id, {
        status: fresh.status,
        planned_on: orNull(sent.planned_on),
        last_position_seconds: position,
        note: sent.note,
        today,
      });
      if (!mineDraft.isOwner(attempt)) return;
      // Keep the successful values visible; only this submitted snapshot becomes clean.
      if (mineDraft.isCurrent(attempt)) {
        mineDraft.complete(attempt);
        mine = sent;
        mineBase = JSON.stringify(sent);
        mineDraft.persist();
      }
      notifyDone(m.courses_mine_saved({ number: e.number }));
      reloadKey += 1;
    } catch (e: unknown) {
      if (mineDraft.isOwner(attempt)) error = errorText(e, m.courses_action_failed());
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

{#if error !== undefined}
  <p role="alert" class="text-sm text-destructive">{error}</p>
{/if}

{#if course !== undefined && progress !== undefined && owner === me.userId && !accountSwitching()}
  {@const c = course}
  {@const p = progress}
  <section class="flex flex-col gap-2" aria-labelledby="course-info-heading">
    <div class="flex items-center gap-2">
      <h2 id="course-info-heading" class="text-lg font-semibold">{m.courses_info()}</h2>
      {#if c.access === 'write' && !editing}
        <Button
          size="sm"
          variant="outline"
          onclick={() => {
            startEdit(c);
          }}>{m.courses_edit()}</Button
        >
      {/if}
    </div>
    <!-- 이 파일의 폼은 모두 저장하는 동안 입력과 되살리기를 잠근다(D-97). 그사이 바뀐 입력은 성공한 저장을 버리게 하고,
         만들기는 다시 내면 새 요청 id로 하나 더 만들며, 판이 있는 고치기는 기준 판을 못 옮겨 자기 저장과 409로 부딪친다. -->
    {#each detailsDraft.offers as offer (offer.key)}
      <fieldset class="contents" disabled={savingDetails}>
        <FormDraftOffer
          restorable={c.access === 'write' && !accountSwitching()}
          onrestore={() => {
            if (c.access === 'write' && !accountSwitching()) detailsDraft.restore(offer);
          }}
          ondiscard={() => {
            detailsDraft.discard(offer);
          }}
        />
      </fieldset>
    {/each}
    {#if editing}
      <fieldset class="min-w-0" disabled={savingDetails}>
        <form
          class="grid gap-2 @xl:grid-cols-2"
          onsubmit={(e: SubmitEvent) => {
            e.preventDefault();
            if (details.url.trim() !== '' && !isWebUrl(details.url.trim())) {
              error = m.courses_url_invalid();
              return;
            }
            // 저장에 성공한 뒤에만 닫는다. 실패하면(보관됨·권한·연결) 입력을 그대로 두고 알린다.
            if (savingDetails) return;
            if (c.access !== 'write') return;
            const attempt = detailsDraft.capture();
            if (attempt === undefined) return;
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
              () => detailsDraft.isOwner(attempt),
            ).then((ok) => {
              if (!detailsDraft.isOwner(attempt)) return;
              savingDetails = false;
              if (
                ok &&
                session === editSession &&
                detailKeys.every((key) => details[key] === sent[key]) &&
                detailsDraft.complete(attempt)
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
          <div class="flex gap-2 @xl:col-span-2">
            <Button type="submit" disabled={savingDetails}>{m.common_save()}</Button>
            <Button
              variant="ghost"
              onclick={() => {
                detailsDraft.leave.request(() => {
                  editing = false;
                });
              }}>{m.common_cancel()}</Button
            >
          </div>
        </form>
      </fieldset>
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
    <div class="grid gap-2 text-sm @xl:grid-cols-3">
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
            <!-- D-113 SR1-11: 읽기 전용도 본인 진도를 남긴다. -->
            {#if c.access !== 'archived'}
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
            {/if}
            <Button
              size="sm"
              variant="ghost"
              aria-label={m.courses_track_aria({ number: e.number })}
              disabled={timerBusy ||
                (timer.entry?.target?.kind === 'episode' && timer.entry.target.id === e.id)}
              onclick={() => void startEpisode(e.id)}
              >{timer.entry?.target?.kind === 'episode' && timer.entry.target.id === e.id
                ? m.time_recording()
                : m.courses_track()}</Button
            >
            <!-- 내 메모·위치는 쓸 수 없어도 볼 수 있다(남기기만 막는다, D3B-F-2). -->
            <Button
              size="sm"
              variant="ghost"
              aria-expanded={openEpisode === e.id}
              aria-label={m.courses_notes_aria({ number: e.number })}
              onclick={() => {
                open(e);
              }}>{m.courses_notes()}</Button
            >
            {#if c.access === 'write'}
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
            {#each episodeDraft.offers as offer (offer.key)}
              <fieldset class="contents" disabled={savingEpisode}>
                <FormDraftOffer
                  onrestore={() => {
                    episodeDraft.restore(offer);
                  }}
                  ondiscard={() => {
                    episodeDraft.discard(offer);
                  }}
                />
              </fieldset>
            {/each}
            <fieldset class="min-w-0" disabled={savingEpisode}>
              <form
                class="grid gap-2 border-t pt-2 @xl:grid-cols-4"
                aria-label={m.courses_episode_form({ number: e.number })}
                onsubmit={(ev: SubmitEvent) => {
                  ev.preventDefault();
                  void saveEpisode(base);
                }}
              >
                <div class="flex flex-col gap-1">
                  <Label for={`ep-number-${e.id}`}>{m.courses_number()}</Label>
                  <Input id={`ep-number-${e.id}`} inputmode="numeric" bind:value={shared.number} />
                </div>
                <div class="flex flex-col gap-1 @xl:col-span-3">
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
                <div class="flex flex-col gap-1 @xl:col-span-2">
                  <Label for={`ep-planned-${e.id}`}>{m.courses_planned()}</Label>
                  <Input id={`ep-planned-${e.id}`} type="date" bind:value={shared.planned_on} />
                </div>
                <div class="flex flex-col gap-1 @xl:col-span-4">
                  <Label for={`ep-url-${e.id}`}>{m.courses_episode_url()}</Label>
                  <Input
                    id={`ep-url-${e.id}`}
                    type="url"
                    bind:value={shared.url}
                    maxlength={2000}
                  />
                </div>
                <div class="flex flex-col gap-1 @xl:col-span-4">
                  <Label for={`ep-materials-${e.id}`}>{m.courses_materials()}</Label>
                  <Textarea
                    id={`ep-materials-${e.id}`}
                    class="min-h-20"
                    bind:value={shared.materials}
                  />
                </div>
                <div class="flex gap-2 @xl:col-span-4">
                  <Button type="submit" size="sm" disabled={savingEpisode}>{m.common_save()}</Button
                  >
                  <Button
                    size="sm"
                    variant="ghost"
                    onclick={() => {
                      episodeDraft.leave.request(() => {
                        editingEpisode = undefined;
                      });
                    }}>{m.common_cancel()}</Button
                  >
                </div>
              </form>
            </fieldset>
          {/if}
          {#if openEpisode === e.id}
            <!-- 쓸 수 없으면 저장 없이 읽기만 한다. 남긴 입력은 지우지 않고 쓸 수 있을 때 고르게 한다. -->
            {#if c.access !== 'archived'}
              {#each mineDraft.offers as offer (offer.key)}
                <fieldset class="contents" disabled={savingMine}>
                  <FormDraftOffer
                    onrestore={() => {
                      mineDraft.restore(offer);
                    }}
                    ondiscard={() => {
                      mineDraft.discard(offer);
                    }}
                  />
                </fieldset>
              {/each}
            {/if}
            <fieldset class="min-w-0" disabled={savingMine}>
              <form
                class="grid gap-2 border-t pt-2 @xl:grid-cols-3"
                onsubmit={(ev: SubmitEvent) => {
                  ev.preventDefault();
                  void saveMine(e);
                }}
              >
                <div class="flex flex-col gap-1">
                  <Label for={`mine-planned-${e.id}`}>{m.courses_my_planned()}</Label>
                  {#if c.access !== 'archived'}
                    <Input id={`mine-planned-${e.id}`} type="date" bind:value={mine.planned_on} />
                  {:else}
                    <!-- 날짜 칸은 readonly여도 달력이 열리는 브라우저가 있어 읽을 글로 보인다. -->
                    <Input id={`mine-planned-${e.id}`} readonly value={dayText(mine.planned_on)} />
                  {/if}
                </div>
                <div class="flex flex-col gap-1">
                  <Label for={`mine-position-${e.id}`}>{m.courses_last_position()}</Label>
                  <Input
                    id={`mine-position-${e.id}`}
                    bind:value={mine.position}
                    readonly={c.access === 'archived'}
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
                <div class="flex flex-col gap-1 @xl:col-span-3">
                  <Label for={`mine-note-${e.id}`}>{m.courses_my_note()}</Label>
                  <Textarea
                    id={`mine-note-${e.id}`}
                    class="min-h-20"
                    readonly={c.access === 'archived'}
                    bind:value={mine.note}
                  />
                </div>
                {#if c.access !== 'archived'}
                  <div><Button type="submit" size="sm">{m.common_save()}</Button></div>
                {/if}
              </form>
            </fieldset>
          {/if}
        </li>
      {/each}
    </ol>

    {#if c.access === 'write'}
      <!-- 회차 추가와 CSV 가져오기는 같은 `adding`을 쓴다. 한쪽을 보내는 동안 두 폼을 함께 잠근다. -->
      {#each newDraft.offers as offer (offer.key)}
        <fieldset class="contents" disabled={adding}>
          <FormDraftOffer
            onrestore={() => {
              newDraft.restore(offer);
            }}
            ondiscard={() => {
              newDraft.discard(offer);
            }}
          />
        </fieldset>
      {/each}
      <fieldset class="min-w-0" disabled={adding}>
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
            if (adding) return;
            const payload = JSON.stringify(row);
            if (payload !== newPayload) {
              newPayload = payload;
              newId = newRequestId();
            }
            const attempt = newDraft.capture();
            if (attempt === undefined) return;
            const requestId = newId;
            adding = true;
            void addEpisodes([row], [requestId], () => newDraft.isOwner(attempt))
              .then((ok) => {
                if (ok) newDraft.complete(attempt);
              })
              .finally(() => {
                if (newDraft.isOwner(attempt)) adding = false;
              });
          }}
        >
          <!-- 최소 폭 12rem은 200% 글꼴의 375px에서 화면보다 넓어지므로 컨테이너 폭을 넘지 않게 한다(D-110). -->
          <div class="flex min-w-[min(12rem,100%)] flex-1 flex-col gap-1">
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
      </fieldset>
      <details class="text-sm">
        <summary class="cursor-pointer">{m.courses_csv_summary()}</summary>
        <fieldset class="flex min-w-0 flex-col gap-2 pt-2" disabled={adding}>
          {#each csvDraft.offers as offer (offer.key)}
            <FormDraftOffer
              onrestore={() => {
                csvDraft.restore(offer);
              }}
              ondiscard={() => {
                csvDraft.discard(offer);
              }}
            />
          {/each}
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
                if (adding) return;
                if (csvSentText !== csvText) {
                  csvSentText = csvText;
                  csvIds = rows.map(() => newRequestId());
                }
                if (rows.length !== csvIds.length) return;
                const attempt = csvDraft.capture();
                if (attempt === undefined) return;
                adding = true;
                void addEpisodes(rows, csvIds, () => csvDraft.isOwner(attempt))
                  .then((ok) => {
                    if (ok) csvDraft.complete(attempt);
                  })
                  .finally(() => {
                    if (csvDraft.isOwner(attempt)) adding = false;
                  });
              }}>{m.courses_csv_add()}</Button
            >
          </div>
        </fieldset>
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
