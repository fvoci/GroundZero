<script lang="ts">
  // 항목의 업무 (D-31): 칸반·목록·간트·달력, 검색·필터·정렬과 저장한 보기, 업무 만들기, 상세 대화상자.
  // 칸반 카드는 버튼과 키보드(Alt+화살표)로 열·순서를 옮긴다. 상세는 주소(/t/{업무})로도 열린다.
  import type { Workspace } from '@fvoci/api-client';
  import { tick, untrack } from 'svelte';

  import type {
    ItemView,
    SavedViewView,
    ShiftProposalView,
    TaskDetailView,
    TaskInput,
    TaskStatus,
    TaskView,
    PlanBlockedView,
    UndatedTaskView,
  } from '$lib/api/client';
  import { api, ApiError, TASK_STATUSES } from '$lib/api/client';
  import { RetryKey } from '$lib/api/retry-key';
  import { errorText } from '$lib/api/errors';
  import { me } from '$lib/me.svelte';
  import { m } from '$lib/paraglide/messages.js';
  import CreateForm from '$lib/features/workspaces/CreateForm.svelte';
  import TaskCsvImport from './TaskCsvImport.svelte';
  import { itemPath, itemTaskPath, router } from '$lib/router.svelte';
  import { Button } from '$lib/ui/button';
  import * as DropdownMenu from '$lib/ui/dropdown-menu';
  import * as Dialog from '$lib/ui/dialog';
  import { durationText } from '$lib/features/time/format';
  import CalendarTaskDialog from '$lib/features/events/CalendarTaskDialog.svelte';
  import DiscardChangesConfirm from '$lib/features/common/DiscardChangesConfirm.svelte';
  import { notifyDone } from '$lib/features/feedback/feedback.svelte';
  import { formatDay } from '$lib/display';

  import type { BoardAction, BoardMove, CardBox, DropSlot } from './board';
  import {
    applyMove,
    dropBefore,
    laneEstimate,
    lanes,
    listOrder,
    moveErrorMessage,
    planBoardMove,
    planDrop,
  } from './board';
  import type { MoveOrigin } from './move-sync';
  import {
    backTo,
    classifyMoveError,
    originOf,
    overlayPending,
    reconcileMove,
    replan,
  } from './move-sync';
  import type { TaskFilter, TaskSort, TaskViewMode } from './filters';
  import {
    applyFilter,
    assigneesOf,
    EMPTY_FILTER,
    isFiltered,
    isRecord,
    readConfig,
    sortTasks,
    VIEWS,
  } from './filters';
  import { PRIORITY_LABEL, PRIORITY_TONE, STATUS_LABEL, undatedText } from './labels';
  import GanttChart from './GanttChart.svelte';
  import { formOf } from './form';
  import ShiftProposals from './ShiftProposals.svelte';
  import TaskCalendar from './TaskCalendar.svelte';
  import TaskDetail from './TaskDetail.svelte';

  interface Props {
    item: ItemView;
    workspaces: Workspace[];
    /** 주소에서 연 업무. */
    openTaskId: string | undefined;
    /** 옮기기가 권한·보관 때문에 거절되면 항목을 다시 읽게 한다(쓰기 단추를 끄기 위해). */
    onstale?: () => void;
    /** 같은 화면의 다른 곳(항목 달력)에서 업무를 만들 때마다 늘어난다(목록을 다시 읽는다). */
    tasksVersion?: number;
  }

  const { item, workspaces, openTaskId, onstale, tasksVersion = 0 }: Props = $props();
  $effect(() => {
    if (tasksVersion === 0) return;
    untrack(() => {
      void reload();
    });
  });

  let tasks = $state<TaskView[]>([]);
  let view = $state<TaskViewMode>('board');
  let error = $state<string | undefined>(undefined);
  let status = $state('');
  // 옮기기 실패 안내. 항목을 다시 읽어 목록을 새로 받아도(error를 지움) 남는다. 다음 옮기기가 성공하면 지운다.
  let moveNotice = $state<string | undefined>(undefined);

  // 검색·필터·정렬과 저장한 보기(D-31).
  let filter = $state<TaskFilter>({ ...EMPTY_FILTER });
  let sort = $state<TaskSort>('position');
  let views = $state<SavedViewView[]>([]);
  let selectedView = $state('');
  // 보기를 지우는 중(두 번 눌러 같은 보기를 다시 지우려다 404 실패를 보이지 않게).
  let deletingView = $state(false);
  let viewName = $state('');
  // 같은 이름·설정의 재시도는 같은 요청 id, 고치면 새 id.
  const viewKey = new RetryKey();
  // 보기·필터·정렬은 이 탭에서 항목마다 기억한다. 다른 화면에 갔다 돌아와도 그대로다(이 브라우저 탭에만 남는 개인
  // 편의라 저장한 보기와 다르다). 저장소를 쓸 수 없으면 기억하지 않는다.
  const contextKey = (itemId: string): string => `fvoci:board:${itemId}`;
  function restoreContext(itemId: string): void {
    let raw: unknown = null;
    try {
      raw = JSON.parse(window.sessionStorage.getItem(contextKey(itemId)) ?? 'null');
    } catch {
      // 저장소를 쓸 수 없거나 값이 깨졌으면 기본값으로.
    }
    if (!isRecord(raw)) {
      // 기억한 것이 없는 항목은 기본값으로 연다(앞 항목의 보기·필터가 남아 이 항목 이름으로 저장되지 않게).
      view = 'board';
      filter = { ...EMPTY_FILTER };
      sort = 'position';
      return;
    }
    const config = readConfig(raw);
    view = config.view;
    filter = config.filter;
    sort = config.sort;
  }
  let contextFor = untrack(() => item.id);
  restoreContext(contextFor);
  $effect(() => {
    const id = item.id;
    const snapshot = JSON.stringify({ view, filter, sort });
    if (contextFor !== id) {
      // 다른 항목으로 바뀌었다: 그 항목의 맥락을 불러온다(지금 것을 그 항목 이름으로 남기지 않는다).
      contextFor = id;
      untrack(() => {
        restoreContext(id);
      });
      return;
    }
    try {
      window.sessionStorage.setItem(contextKey(id), snapshot);
    } catch {
      // 저장소를 쓸 수 없으면 기억하지 않는다.
    }
  });
  const shown = $derived(sortTasks(applyFilter(tasks, filter, me.userId), sort));
  const people = $derived(assigneesOf(tasks));
  const ordered = $derived(sort === 'position');

  async function loadViews(itemId: string): Promise<void> {
    views = await api.savedViews(itemId).catch(() => []);
  }

  function applyView(id: string): void {
    selectedView = id;
    const v = views.find((x) => x.id === id);
    if (v === undefined) return;
    const config = readConfig(v.config);
    view = config.view;
    filter = config.filter;
    sort = config.sort;
    viewName = v.name;
  }

  async function saveView(): Promise<void> {
    const name = viewName.trim();
    if (name === '') {
      error = m.tasks_view_name_required();
      return;
    }
    const itemId = item.id;
    const config = { view, filter: { ...filter }, sort };
    // 목록에 같은 이름의 보기가 있으면 읽은 판으로 그 보기를 고치고, 없으면 새로 만든다(D-103). 재시도 키에 대상과
    // 판을 넣어, 목록을 다시 읽은 뒤의 저장은 새 요청이 된다.
    const target = views.find((v) => v.name === name);
    try {
      const requestId = viewKey.for({
        item: itemId,
        name,
        config,
        target: target === undefined ? null : { id: target.id, version: target.version },
      });
      const result =
        target === undefined
          ? await api.createView(itemId, requestId, name, config)
          : await api.updateView(target.id, requestId, target.version, config);
      viewKey.done(requestId);
      await loadViews(itemId);
      if (result.kind !== 'saved') {
        // 다른 탭이 이 이름으로 먼저 만들거나 저장했다. 서버는 아무것도 바꾸지 않았다. 지금 화면의 설정과 이름은 그대로
        // 두고 최신 목록만 읽는다(최신 설정은 목록에서 고르면 보이고, 다시 저장하면 최신 판을 기준으로 이 설정이 된다).
        selectedView = result.current.id;
        status = '';
        error = m.tasks_view_conflict({ name });
        return;
      }
      selectedView = result.view.id;
      status = m.tasks_view_saved({ name });
      error = undefined;
    } catch (e: unknown) {
      error = errorText(e, m.tasks_view_save_failed());
    }
  }

  async function deleteView(): Promise<void> {
    if (selectedView === '' || deletingView) return;
    deletingView = true;
    try {
      await api.deleteView(selectedView);
      selectedView = '';
      await loadViews(item.id);
      status = m.tasks_view_deleted();
    } catch (e: unknown) {
      error = errorText(e, m.tasks_view_delete_failed());
    } finally {
      deletingView = false;
    }
  }

  // 글자는 읽을 때 지금 언어로 만든다.
  const VIEW_LABEL: Record<TaskViewMode, string> = {
    get board() {
      return m.tasks_view_board();
    },
    get list() {
      return m.tasks_view_list();
    },
    get gantt() {
      return m.tasks_view_gantt();
    },
    get calendar() {
      return m.tasks_view_calendar();
    },
  };
  const SORT_LABEL: Record<TaskSort, string> = {
    get position() {
      return m.tasks_sort_position();
    },
    get due() {
      return m.tasks_sort_due();
    },
    get priority() {
      return m.tasks_sort_priority();
    },
    get title() {
      return m.tasks_sort_title();
    },
    get updated() {
      return m.tasks_sort_updated();
    },
  };

  const fieldBox =
    'h-8 rounded-md border border-input bg-transparent px-2 text-sm focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none';

  // 이 항목의 목록을 한 번이라도 받았는지(실패도 받은 것으로 본다: 오류를 보인다). 받기 전에는 빈 열·"업무가 없습니다"
  // 대신 불러오는 중이라고 보인다. 같은 항목을 다시 읽을 때(옮긴 뒤 등)는 지금 목록을 그대로 둔다.
  let loadedFor = $state<string | undefined>(undefined);
  const loading = $derived(loadedFor !== item.id);

  // 저장을 기다리는 이동(카드마다 하나, move-sync.ts). 화면은 먼저 옮기고 서버 응답으로 확정한다. 응답이 늦으면 지연을,
  // 결과를 모르면 목록을 다시 읽어 대조한 결과를 보인다. 이동 API에는 요청 id·판 조건이 없어 자동으로 다시 보내지 않는다.
  type SavePhase = 'saving' | 'slow' | 'checking' | 'unknown' | 'not_saved';
  interface PendingMove {
    seq: number;
    plan: BoardMove;
    origin: MoveOrigin;
    phase: SavePhase;
    title: string;
    how: BoardAction | 'drag' | 'menu';
    /** 이동 요청이 아직 끝나지 않았는지(끝나기 전에는 "반영 안 됨"으로 단정하지 않는다). */
    inFlight: boolean;
    /** 확정되면 되돌리기를 내놓는지(되돌리기 자체의 이동은 내놓지 않는다). */
    undoable: boolean;
  }
  const SLOW_MS = 3000;
  const CHECK_MS = 10000;
  let pending = $state<Record<string, PendingMove>>({});
  /**
   * 서버가 확정한 목록(마지막으로 읽은 목록에 그 뒤 성공 응답을 받은 이동을 반영한 것). 화면 `tasks`는 이 위에 저장을
   * 기다리는 이동을 얹은 것이다. 이동 전 자리(원점)는 여기서 잡는다: 화면 배열에는 다른 카드의 아직 확정되지 않은 이동이
   * 섞여 있어, 그 카드가 실패하거나 응답이 거꾸로 오면 되돌리기가 엉뚱한 자리로 간다.
   */
  let confirmed: TaskView[] = [];
  let moveSeq = 0;
  // 다시 읽기마다 번호를 붙여 늦게 도착한 옛 목록이 더 새 화면을 덮지 않게 한다.
  let reloadSeq = 0;
  let alive = true;
  // 카드별 타이머(화면 상태가 아니므로 반응형으로 두지 않는다).
  const timers: Record<string, ReturnType<typeof setTimeout>[] | undefined> = {};
  /** 카드별 대조 차례(겹친 대조 가운데 마지막 것만 따른다). */
  const checks: Record<string, number | undefined> = {};

  function clearTimers(id: string): void {
    for (const t of timers[id] ?? []) clearTimeout(t);
    timers[id] = undefined;
  }
  function current(id: string, seq: number): boolean {
    return alive && pending[id]?.seq === seq;
  }
  function patch(id: string, seq: number, change: Partial<PendingMove>): void {
    const p = pending[id];
    if (p?.seq === seq) pending = { ...pending, [id]: { ...p, ...change } };
  }
  function finish(id: string): void {
    clearTimers(id);
    pending = Object.fromEntries(Object.entries(pending).filter(([key]) => key !== id));
  }
  /**
   * 저장 결과를 기다리는 이동만 서버 목록 위에 다시 얹는다. "저장 안 됨"과 "확인 못 함"은 얹지 않는다(서버에 있는
   * 자리를 보인다). `except`는 지금 대조하는 카드.
   */
  const overlaid = (server: TaskView[], except?: string): TaskView[] =>
    overlayPending(
      server,
      Object.entries(pending)
        .filter(([id, p]) => id !== except && p.phase !== 'not_saved' && p.phase !== 'unknown')
        .map(([id, p]) => ({ id, plan: p.plan })),
    );
  /** 다른 일로 목록을 새로 읽었을 때 "확인 못 함"인 이동을 그 목록과 대조해 확정한다. */
  function settleUnknown(server: TaskView[]): void {
    for (const [id, p] of Object.entries(pending)) {
      if (p.phase !== 'unknown') continue;
      const verdict = reconcileMove(server, id, p.origin, p.plan);
      if (verdict === 'not_applied') {
        patch(id, p.seq, { phase: 'not_saved' });
        moveNotice = m.tasks_move_not_saved({ title: p.title });
      } else {
        finish(id);
        if (verdict !== 'applied') {
          moveNotice =
            verdict === 'gone'
              ? m.tasks_move_error_gone()
              : m.tasks_move_changed({ title: p.title });
        }
      }
    }
  }
  /** 다른 카드의 "저장 안 됨" 안내가 남아 있으면 지우지 않는다(안내는 하나뿐이다). */
  function clearMoveNotice(): void {
    if (!Object.values(pending).some((p) => p.phase === 'not_saved')) moveNotice = undefined;
  }
  const busy = (id: string): boolean => {
    const phase = pending[id]?.phase;
    return phase !== undefined && phase !== 'not_saved';
  };

  /**
   * 방금 확정된 이동의 되돌리기(마지막 하나만). 결과를 모르는 이동에는 내놓지 않는다. 다시 읽은 목록에서 그 카드가 이
   * 이동의 결과(열·바로 뒤 업무·판)와 다르면(다른 사람이 옮기거나 고침) 거둔다. 시간이 지나도 저절로 없애지 않는다.
   */
  interface UndoOffer {
    id: string;
    title: string;
    /** 이동 전 서버 자리. */
    from: MoveOrigin;
    to: BoardMove;
    /** 이 이동 뒤의 이동 판(D-89). */
    moveVersion: number;
  }
  let undoOffer = $state<UndoOffer | undefined>(undefined);
  function offerUndo(
    id: string,
    title: string,
    from: MoveOrigin,
    to: BoardMove,
    moveVersion: number,
  ): void {
    undoOffer = { id, title, from, to, moveVersion };
  }
  /** 이 목록에서도 되돌리기가 맞는지(그 카드가 이 이동의 결과 그대로인지). */
  const undoStillFits = (server: readonly TaskView[], u: UndoOffer): boolean =>
    server.find((t) => t.id === u.id)?.move_version === u.moveVersion &&
    reconcileMove(server, u.id, u.from, u.to) === 'applied';

  async function undoMove(): Promise<void> {
    const u = undoOffer;
    if (u === undefined || busy(u.id)) return;
    undoOffer = undefined;
    status = m.tasks_move_checking({ title: u.title });
    let server: TaskView[];
    try {
      server = await api.tasks(item.id);
    } catch {
      if (!alive) return;
      // 지금 상태를 모르면 보내지 않는다. 다시 누를 수 있게 둔다.
      undoOffer = u;
      status = m.tasks_undo_read_failed({ title: u.title });
      return;
    }
    if (!alive) return;
    ++reloadSeq;
    confirmed = server;
    tasks = overlaid(server);
    if (!undoStillFits(server, u)) {
      moveNotice = m.tasks_undo_changed({ title: u.title });
      status = moveNotice;
      await focusCard(u.id);
      return;
    }
    // 원래 이웃(바로 뒤, 아니면 바로 앞)이 그 열에 그대로 있으면 그 자리로, 둘 다 없으면 열 끝으로 되돌린다(그렇게 알린다).
    const back = backTo(server, u.from);
    await commitMove(u.id, back.plan, 'menu', u.title, undefined, {
      done: (title, lane) =>
        !back.exact
          ? m.tasks_undone_end({ title, status: STATUS_LABEL[lane] })
          : m.tasks_undone({ title, status: STATUS_LABEL[lane] }),
    });
    await focusCard(u.id);
  }
  async function focusCard(id: string): Promise<void> {
    await tick();
    document.querySelector<HTMLElement>(`[data-task-card="${CSS.escape(id)}"]`)?.focus();
  }

  // 화면을 떠나면 타이머를 멈추고 늦게 온 응답은 버린다. 항목이 바뀌면 앞 항목의 기다림을 버린다.
  $effect(() => {
    alive = true;
    return () => {
      alive = false;
      for (const id of Object.keys(timers)) clearTimers(id);
    };
  });
  $effect(() => {
    // item.id를 읽어 항목이 바뀔 때마다 다시 돈다(빈 id는 없다).
    if (item.id === '') return;
    untrack(() => {
      for (const id of Object.keys(timers)) clearTimers(id);
      pending = {};
      confirmed = [];
      undoOffer = undefined;
    });
  });

  async function reload(itemId: string = item.id): Promise<void> {
    const mine = ++reloadSeq;
    try {
      const server = await api.tasks(itemId);
      if (!alive || mine !== reloadSeq || itemId !== item.id) return;
      settleUnknown(server);
      confirmed = server;
      tasks = overlaid(server);
      if (undoOffer !== undefined && !undoStillFits(server, undoOffer)) undoOffer = undefined;
      error = undefined;
    } catch (e: unknown) {
      if (alive && mine === reloadSeq) error = errorText(e, m.tasks_load_failed());
    } finally {
      if (itemId === item.id) loadedFor = itemId;
    }
  }

  // 쓰기 권한으로 항목을 열면 반복 업무의 회차를 먼저 맞춘다(독자는 만들지 않는다, O-19 전까지의 방식).
  async function loadItem(itemId: string, write: boolean): Promise<void> {
    if (write) await api.syncRecurrences(itemId).catch(() => 0);
    await reload(itemId);
  }

  $effect(() => {
    void loadItem(item.id, item.access !== 'read' && !item.archived);
    void loadViews(item.id);
  });

  const canWrite = $derived(item.access !== 'read' && !item.archived);
  const board = $derived(lanes(shown));
  // 열 머리의 예상 합계(D-31 예상 시간). 거르기 중이면 보이는 카드 기준과 열 전체를 함께 보인다(실제 시간은 목록에 없어 넣지 않음).
  const allLanes = $derived(lanes(tasks));
  function laneEstimateText(lane: TaskStatus): string {
    const shownSum = laneEstimate(board[lane]);
    if (shownSum.count === 0) return '';
    const duration = durationText(shownSum.minutes * 60);
    const parts = [
      shownSum.missing === shownSum.count
        ? m.tasks_lane_estimate_none()
        : isFiltered(filter)
          ? m.tasks_lane_estimate_shown({ duration })
          : m.tasks_lane_estimate({ duration }),
    ];
    if (shownSum.missing > 0 && shownSum.missing < shownSum.count)
      parts.push(m.tasks_lane_estimate_missing({ count: shownSum.missing }));
    if (isFiltered(filter)) {
      const all = laneEstimate(allLanes[lane]);
      parts.push(m.tasks_lane_estimate_all({ duration: durationText(all.minutes * 60) }));
    }
    return parts.join(' · ');
  }
  const list = $derived(ordered ? listOrder(shown) : shown);
  const titleOf = (id: string | null): string =>
    id === null ? '' : (tasks.find((t) => t.id === id)?.title ?? '');

  // 이 보드가 기록을 쌓아 연 대화상자인지. 그렇다면 닫을 때 그 기록으로 돌아가, 뒤로 가기가 방금 닫은 카드를 다시
  // 열지 않고 보드를 떠나게 한다. 주소로 바로 들어온 업무는 항목 주소로 옮긴다.
  let openedHere = false;

  function open(id: string): void {
    router.navigate(itemTaskPath(item.workspace_id, item.id, id));
    openedHere = true;
  }

  // 주소로 연 대화상자는 여는 단추를 모르므로, 닫으면 마지막으로 연 업무의 카드로 초점을 돌린다.
  let returnTo: string | undefined;
  $effect(() => {
    if (openTaskId !== undefined) returnTo = openTaskId;
    else openedHere = false;
    // 다른 업무를 열었거나 닫혔으면 앞 업무에 대해 물은 것을 거둔다(새로 연 업무를 닫거나 그 입력을 버리지 않게).
    confirmClose = false;
  });

  // 업무 상세에 기준과 다르게 고친 칸이 있는지, 그래서 닫기 전에 버릴지 묻는 중인지. 물은 업무(`askedFor`)가 지금 연
  // 업무일 때만 버리고 닫는다. 계속 편집하면 물을 때 초점이 있던 곳(대화상자 밖이었으면 대화상자)으로 돌아간다.
  let detailUnsaved = $state(false);
  // 폼을 저장하는 동안은 닫지도 묻지도 않는다: 이미 보낸 저장은 서버에 남으므로 "버리고 닫기"가 거짓이 된다(일정 대화상자와 같음).
  let detailSaving = $state(false);
  let confirmClose = $state(false);
  let askedFor: string | undefined;
  // 상세 안의 단추(상위·하위·선행·후속·첫 회차)로 다른 업무를 열려다 물었으면 그 업무. 버리기를 고르면 닫지 않고 그것을
  // 연다. 물을 때마다 정한다(확인이 닫히는 동안 문구가 바뀌지 않게 지우지 않는다).
  let openNext = $state<string | undefined>(undefined);
  let keepFocus: HTMLElement | null = null;
  let dialogContent = $state<HTMLElement | null>(null);

  /** 버릴지 묻는다. 계속 편집하면 지금 초점이 있던 곳(대화상자 밖이었으면 대화상자)으로 돌아간다. */
  function ask(taskId: string, next: string | undefined): void {
    const active = document.activeElement;
    keepFocus =
      active instanceof HTMLElement && dialogContent?.contains(active) ? active : dialogContent;
    askedFor = taskId;
    openNext = next;
    confirmClose = true;
  }

  /** Esc·바깥 클릭·닫기 단추. 저장하지 않은 입력이 있으면 닫지 않고 버릴지 묻는다. */
  function requestClose(): void {
    if (confirmClose || detailSaving) return;
    if (detailUnsaved && openTaskId !== undefined) {
      ask(openTaskId, undefined);
      return;
    }
    close();
  }

  /**
   * 상세 안에서 다른 업무를 연다. 열면 폼을 새로 그리므로(`{#key openTaskId}`) 닫기와 같이 저장하지 않은 입력이 있으면
   * 버릴지 묻고, 저장하는 동안은 옮기지 않는다(CodeRabbit #82).
   */
  function requestOpen(id: string): void {
    if (confirmClose || detailSaving) return;
    if (detailUnsaved && openTaskId !== undefined && id !== openTaskId) {
      ask(openTaskId, id);
      return;
    }
    open(id);
  }

  function discard(): void {
    confirmClose = false;
    if (askedFor === undefined || askedFor !== openTaskId || detailSaving) return;
    if (openNext === undefined) close();
    else open(openNext);
  }

  function close(): void {
    if (openedHere) {
      // 저장하지 않은 편집 확인에서 머물기로 하면 돌아가지 않았으므로 다음 닫기도 뒤로 간다.
      openedHere = !router.back();
    } else {
      router.navigate(itemPath(item.workspace_id, item.id));
    }
  }

  /** 옮긴 뒤 화면 낭독기에 알리는 글. */
  function movedText(action: BoardAction, title: string, lane: TaskStatus): string {
    const values = { title, status: STATUS_LABEL[lane] };
    switch (action) {
      case 'up':
        return m.tasks_moved_up(values);
      case 'down':
        return m.tasks_moved_down(values);
      case 'left':
        return m.tasks_moved_left(values);
      case 'right':
        return m.tasks_moved_right(values);
    }
  }

  /**
   * 옮기기 한 번(단추·키보드·끌어서 놓기 공통). 화면에 먼저 반영하고 기존 이동 API로 저장한 뒤 서버 목록을 다시
   * 읽는다. 실패하면 이전 화면으로 되돌리고 이유를 알린 뒤 최신 상태(동시 변경·권한 철회·보관)를 다시 읽는다.
   */
  /** 메뉴로 옮길 수 있는 열의 이름(조사가 열마다 다르다). */
  const MOVE_TO: Record<TaskStatus, () => string> = {
    todo: () => m.tasks_move_to_todo(),
    in_progress: () => m.tasks_move_to_in_progress(),
    done: () => m.tasks_move_to_done(),
  };

  // 열마다 빠르게 더하기(Kaneo처럼 열 아래). 만들기는 "할 일"로 만들어지므로, 다른 열이면 같은 이동 저장 경로로 그
  // 열 끝에 옮긴다. 옮기기가 실패하면 업무는 "할 일"에 남고 실패를 알린다(만든 것은 사라지지 않는다).
  let adding = $state<TaskStatus | undefined>(undefined);
  // 달력 보기의 날짜 칸에서 이 항목에 업무 만들기(그 날짜가 마감일).
  let createDay = $state<string | undefined>(undefined);
  let boardCalendar = $state<HTMLElement | undefined>(undefined);
  /** 만들기 창이 닫히면 이 보드 달력의 그 날짜 단추로(같은 화면의 다른 달력 단추가 아니라). */
  async function focusCreate(day: string): Promise<void> {
    await tick();
    boardCalendar?.querySelector<HTMLElement>(`[data-create-day="${day}"]`)?.focus();
  }

  // 간트에서 일정 바꾸기(D-31). 최신 업무를 읽어 그 판으로 후속 업무 영향을 미리 보고, 있으면 묻고(승인·이 업무만·취소),
  // 없으면 바로 저장한다. 그사이 그 업무의 날짜가 바뀌었으면(다른 사람) 저장하지 않는다. 끝나면 목록을 다시 읽어
  // 막대를 서버 상태로 되돌리거나 맞춘다(취소·거절·실패·충돌 모두).
  interface GanttAsk {
    task: TaskView;
    input: TaskInput;
    baseVersion: number;
    proposals: ShiftProposalView[];
    undated: UndatedTaskView[];
    blocked: PlanBlockedView | null;
    retried: boolean;
    done: () => void;
  }
  let ganttAsk = $state<GanttAsk | undefined>(undefined);
  let ganttBusy = $state(false);
  let ganttNotice = $state<string | undefined>(undefined);
  // 저장했지만 날짜가 없어 옮기지 않은 후속 업무 알림(오류가 아니다, 날짜를 지어내지 않는다).
  let ganttInfo = $state('');
  const datesText = (start: string | null, due: string | null): string =>
    start !== null && due !== null && start !== due ? `${start}~${due}` : (due ?? start ?? '');

  async function reschedule(
    t: TaskView,
    dates: { start_date: string | null; due_date: string | null },
  ): Promise<void> {
    ganttNotice = undefined;
    ganttInfo = '';
    try {
      const fresh = await api.task(t.id);
      if (fresh.start_date !== t.start_date || fresh.due_date !== t.due_date) {
        ganttNotice = m.tasks_gantt_changed({ title: t.title });
        status = ganttNotice;
        await reload();
        return;
      }
      const input = { ...formOf(fresh), ...dates };
      const plan = await api.previewSchedule(t.id, dates.start_date, dates.due_date);
      if (plan.shifts.length === 0) {
        // 저장하다 후속 업무가 바뀌어 다시 묻게 되면 그 물음이 끝날 때까지 기다린다.
        let finish: () => void = () => undefined;
        const finished = new Promise<void>((resolve) => {
          finish = resolve;
        });
        const outcome = await saveSchedule(
          t,
          input,
          fresh.version,
          [],
          () => {
            finish();
          },
          plan.undated,
        );
        if (outcome === 'asked') await finished;
        return;
      }
      await new Promise<void>((done) => {
        ganttAsk = {
          task: t,
          input,
          baseVersion: fresh.version,
          proposals: plan.shifts,
          undated: plan.undated,
          blocked: plan.blocked,
          retried: false,
          done,
        };
      });
    } catch (e: unknown) {
      ganttNotice = errorText(e, m.tasks_gantt_failed({ title: t.title }));
      status = ganttNotice;
      await reload();
    }
  }

  /** 저장. 후속 업무가 그사이 바뀌어(shift_conflict) 다시 물어야 하면 `asked`. */
  async function saveSchedule(
    t: TaskView,
    input: TaskInput,
    baseVersion: number,
    shifts: ShiftProposalView[],
    /** 다시 물을 때 그 물음이 끝나면 부를 것(간트가 미리 보기를 거두는 때). */
    done: () => void = () => undefined,
    /** 날짜가 없어 옮기지 않는 후속 업무(저장한 뒤 알린다). */
    undated: UndatedTaskView[] = [],
  ): Promise<'saved' | 'asked' | 'stopped'> {
    const result = await api.updateTask(
      t.id,
      baseVersion,
      input,
      shifts.map((s) => ({
        task_id: s.task_id,
        base_version: s.base_version,
        start_date: s.start_date,
        due_date: s.due_date,
      })),
    );
    if (result.kind === 'conflict') {
      ganttNotice = m.tasks_gantt_conflict({ title: t.title });
      status = ganttNotice;
      await reload();
      return 'stopped';
    }
    if (result.kind === 'shift_conflict') {
      // 아무것도 바뀌지 않았다. 지금 상태로 다시 계산해 다시 묻는다(이제 옮길 것이 없으면 그대로 저장).
      const plan = await api.previewSchedule(t.id, input.start_date, input.due_date);
      if (plan.shifts.length === 0) {
        return saveSchedule(t, input, baseVersion, [], done, plan.undated);
      }
      ganttAsk = {
        task: t,
        input,
        baseVersion,
        proposals: plan.shifts,
        undated: plan.undated,
        blocked: plan.blocked,
        retried: true,
        done,
      };
      return 'asked';
    }
    const savedText =
      shifts.length > 0
        ? m.tasks_gantt_saved_shifts({ title: t.title, count: shifts.length })
        : m.tasks_gantt_saved({
            title: t.title,
            dates: datesText(input.start_date, input.due_date),
          });
    notifyDone(savedText);
    ganttInfo = undatedText(undated);
    status = [savedText, ganttInfo].filter((s) => s !== '').join(' ');
    await reload();
    return 'saved';
  }

  async function decideGantt(withShifts: boolean): Promise<void> {
    const a = ganttAsk;
    if (a === undefined || ganttBusy) return;
    ganttBusy = true;
    let outcome: 'saved' | 'asked' | 'stopped' = 'stopped';
    try {
      outcome = await saveSchedule(
        a.task,
        a.input,
        a.baseVersion,
        withShifts ? a.proposals : [],
        a.done,
        a.undated,
      );
    } catch (e: unknown) {
      ganttNotice = errorText(e, m.tasks_gantt_failed({ title: a.task.title }));
      status = ganttNotice;
      await reload();
    } finally {
      ganttBusy = false;
      if (outcome !== 'asked') {
        ganttAsk = undefined;
        a.done();
        // 창이 닫히면 그 업무의 간트 줄로 초점을 돌린다(알림 영역으로 가지 않게).
        void focusCard(a.task.id);
      }
    }
  }

  function cancelGantt(): void {
    const a = ganttAsk;
    // 저장을 보낸 뒤에는 취소할 수 없다(결과를 기다린다).
    if (a === undefined || ganttBusy) return;
    ganttAsk = undefined;
    status = m.tasks_gantt_cancelled({ title: a.task.title });
    a.done();
    void reload().then(() => focusCard(a.task.id));
  }
  async function quickAdd(lane: TaskStatus, requestId: string, title: string): Promise<void> {
    const itemId = item.id;
    const created = await api.createTask(itemId, requestId, title);
    // 만드는 사이 화면을 떠났거나 다른 항목으로 바뀌었으면 이어서 옮기지 않는다(타이머·요청이 사라진 화면에 남지 않게).
    // 업무는 이미 만들어졌으므로 "할 일"에 남는다.
    if (!alive || itemId !== item.id) return;
    if (created.status === lane) {
      await reload();
      return;
    }
    // 방금 만든 업무는 아직 목록에 없다. 만든 자리("할 일" 끝)에 먼저 넣어 이동 상태(저장 중·확인 못 함·다시 확인)를 그 카드에
    // 보이게 하고, 알림의 제목과 이동 전 자리를 넘긴다(전에는 목록을 다시 읽지 못하면 카드도 다시 확인 단추도 없었다).
    // 만들기 전에 시작한 목록 읽기는 이 카드가 없는 목록이라, 늦게 와서 카드를 지우지 않게 무효로 한다(이후 읽기만 따른다).
    if (!tasks.some((t) => t.id === created.id)) {
      ++reloadSeq;
      tasks = [...tasks, created];
    }
    // 서버가 돌려준 업무이므로 확정 목록에도 넣는다(이 카드와 이웃의 원점이 이 카드를 본다).
    if (!confirmed.some((t) => t.id === created.id)) confirmed = [...confirmed, created];
    await commitMove(created.id, { status: lane, beforeId: null }, 'menu', created.title);
  }

  /** 빠른 추가를 닫고(Esc) 그 열의 "업무 추가" 단추로 초점을 돌린다. */
  async function closeQuickAdd(lane: TaskStatus): Promise<void> {
    adding = undefined;
    await tick();
    document.querySelector<HTMLElement>(`[data-quick-add="${lane}"]`)?.focus();
  }

  function movedMessage(how: PendingMove['how'], title: string, lane: TaskStatus): string {
    return how === 'drag'
      ? m.tasks_moved_drag({ title, status: STATUS_LABEL[lane] })
      : how === 'menu'
        ? m.tasks_moved_menu({ title, status: STATUS_LABEL[lane] })
        : movedText(how, title, lane);
  }

  /**
   * 옮기기 한 번(단추·키보드·끌어서 놓기·메뉴 공통). 화면에 먼저 반영하고 기존 이동 API로 저장한다. 같은 카드는 앞선
   * 저장이 끝날 때까지 다시 옮기지 않는다(다른 카드는 그대로 옮길 수 있다). 서버가 처리하지 않았다고 답하면 되돌리고
   * 이유를 알린다. 결과를 모르면(응답 없음·게이트웨이 오류) 목록을 다시 읽어 대조한 뒤 확정한다.
   */
  async function commitMove(
    id: string,
    plan: BoardMove,
    how: PendingMove['how'],
    knownTitle?: string,
    /** 다시 시도: 이 판의 대기 항목을 그대로 교체한다(지우고 다시 넣는 사이에 같은 카드의 요청이 끼지 않게). */
    replacing?: number,
    /** 되돌리기: 보내기 직전 서버 자리와 그 이동 판을 조건으로 보내고(끼어든 변경은 서버가 409로 거절), 다시 되돌리기는 내놓지 않는다. */
    undo?: { done: (title: string, lane: TaskStatus) => string },
  ): Promise<boolean> {
    const title = knownTitle ?? titleOf(id);
    if (busy(id) && (replacing === undefined || pending[id]?.seq !== replacing)) {
      status = m.tasks_move_busy({ title });
      return false;
    }
    // 원점은 서버가 확정한 목록에서(화면 배열이 아니라). 확정 목록에 없는 카드(만드는 중)는 옮기지 않는다.
    const origin = originOf(confirmed, id);
    if (origin === undefined) return false;
    // 같은 카드를 다시 옮기면 앞선 이동의 되돌리기는 더 이상 맞지 않는다.
    if (undoOffer?.id === id) undoOffer = undefined;
    clearTimers(id);
    const seq = ++moveSeq;
    pending = {
      ...pending,
      [id]: {
        seq,
        plan,
        origin,
        phase: 'saving',
        title,
        how,
        inFlight: true,
        undoable: undo === undefined,
      },
    };
    tasks = applyMove(tasks, id, plan);
    status = m.tasks_move_saving({ title });
    timers[id] = [
      setTimeout(() => {
        if (!current(id, seq)) return;
        patch(id, seq, { phase: 'slow' });
        status = m.tasks_move_slow({ title });
      }, SLOW_MS),
      // 오래 걸리면 요청을 끊지 않고 서버 목록을 대조해 본다. 이미 반영됐으면 바로 확정한다.
      setTimeout(() => {
        if (current(id, seq)) void reconcile(id, seq);
      }, CHECK_MS),
    ];
    try {
      // 다시 시도·되돌리기는 보내기 직전에 읽은 서버 자리의 이동 판을 조건으로 보낸다(D-89): 그사이 다른 곳에서 이
      // 업무의 열·위치가 바뀌었으면 서버가 아무것도 바꾸지 않고 409 `move_conflict`를 돌려준다. 처음 옮기기는 조건 없이.
      const guarded = replacing !== undefined || undo !== undefined;
      const saved = await api.moveTask(
        id,
        plan.status,
        plan.beforeId,
        guarded ? origin.moveVersion : undefined,
      );
      if (!current(id, seq)) return true;
      // 성공 응답이 확정이다: 다시 읽기 전에 옮기는 다른 카드의 원점이 이 결과를 보도록 확정 목록에 먼저 반영한다.
      confirmed = applyMove(
        confirmed.map((t) => (t.id === id ? { ...t, move_version: saved.move_version } : t)),
        id,
        plan,
      );
      finish(id);
      await reload();
      clearMoveNotice();
      status =
        undo === undefined ? movedMessage(how, title, plan.status) : undo.done(title, plan.status);
      if (undo === undefined) offerUndo(id, title, origin, plan, saved.move_version);
      return true;
    } catch (e: unknown) {
      if (!current(id, seq)) return false;
      // 요청이 끝났으므로 "지연" 타이머가 대조 결과(확인 못 함 등)를 덮지 않게 멈춘다.
      clearTimers(id);
      patch(id, seq, { inFlight: false });
      if (classifyMoveError(e) === 'uncertain') return (await reconcile(id, seq)) === 'applied';
      finish(id);
      // 서버는 아무것도 바꾸지 않았다: 확정 목록 위에 남은 다른 카드의 기다리는 이동만 다시 얹는다.
      tasks = overlaid(confirmed);
      // 안내는 다시 읽기 전에 정한다: 다시 읽는 동안 카드는 풀려 있어 그사이 성공한 다음 이동의 안내를 늦게 덮지 않게.
      // 조건부 이동이 거절됐다(그사이 다른 곳에서 이 업무의 열·위치가 바뀜, D-89): 아무것도 바뀌지 않았다. 최신 상태를
      // 다시 읽어 보이고 다시 보내지 않는다. 항목 자체의 상태(보관·권한)가 바뀐 것이 아니므로 항목은 다시 읽지 않는다.
      const conflicted = e instanceof ApiError && e.code === 'move_conflict';
      const message = conflicted
        ? undo === undefined
          ? m.tasks_move_conflict({ title })
          : m.tasks_undo_changed({ title })
        : moveErrorMessage(e);
      moveNotice = message;
      status = message;
      if (!conflicted && e instanceof ApiError && (e.status === 403 || e.status === 409))
        onstale?.();
      await reload();
      return false;
    }
  }

  /**
   * 결과를 모르는 이동을 서버 목록과 대조해 확정한다. 요청이 아직 끝나지 않았으면 반영된 경우에만 확정하고 나머지는
   * 기다린다(늦게 반영될 수 있으므로 "저장 안 됨"으로 단정하지 않는다).
   */
  async function reconcile(
    id: string,
    seq: number,
  ): Promise<ReturnType<typeof reconcileMove> | undefined> {
    const p = pending[id];
    if (p?.seq !== seq) return undefined;
    patch(id, seq, { phase: 'checking' });
    status = m.tasks_move_checking({ title: p.title });
    const mine = ++reloadSeq;
    // 같은 카드의 대조가 겹치면(요청 중에 시작한 대조가 끝나기 전에 요청이 응답 없이 끝나 다시 대조) 마지막 것만 따른다.
    // 판정에는 목록을 읽기 시작할 때 요청이 아직 진행 중이었는지를 쓴다: 그때 읽은 목록은 요청이 반영되기 전일 수 있다.
    const check = (checks[id] = (checks[id] ?? 0) + 1);
    const stillInFlight = p.inFlight;
    const latest = (): boolean => current(id, seq) && checks[id] === check;
    let server: TaskView[];
    try {
      server = await api.tasks(item.id);
    } catch {
      if (!latest()) return undefined;
      patch(id, seq, { phase: pending[id]?.inFlight === true ? 'slow' : 'unknown' });
      status = m.tasks_move_unknown({ title: p.title });
      return undefined;
    }
    if (!latest()) return undefined;
    const verdict = reconcileMove(server, id, p.origin, p.plan);
    if (verdict === 'applied') {
      finish(id);
      clearMoveNotice();
      status = m.tasks_move_confirmed({ title: p.title, status: STATUS_LABEL[p.plan.status] });
      const confirmed = server.find((t) => t.id === id);
      if (p.undoable && confirmed !== undefined)
        offerUndo(id, p.title, p.origin, p.plan, confirmed.move_version);
    } else if (stillInFlight) {
      patch(id, seq, { phase: 'slow' });
      status = m.tasks_move_slow({ title: p.title });
    } else if (verdict === 'not_applied') {
      clearTimers(id);
      patch(id, seq, { phase: 'not_saved' });
      moveNotice = m.tasks_move_not_saved({ title: p.title });
      status = moveNotice;
    } else {
      finish(id);
      moveNotice =
        verdict === 'gone' ? m.tasks_move_error_gone() : m.tasks_move_changed({ title: p.title });
      status = moveNotice;
    }
    if (mine === reloadSeq) {
      confirmed = server;
      tasks = overlaid(server);
    }
    return verdict;
  }

  /** 결과를 확인하지 못한 이동을 다시 대조한다(사용자가 누름). */
  function recheck(id: string): void {
    const p = pending[id];
    if (p !== undefined && p.phase !== 'checking') void reconcile(id, p.seq);
  }

  /**
   * 저장되지 않은 이동을 다시 보낸다. 보내기 직전 서버를 한 번 더 읽어 그때까지 다른 곳에서 바뀌었으면 보내지 않고 알린다.
   * 읽은 뒤 보내기 전 사이의 변경은 읽은 이동 판을 조건으로 보내 서버가 막는다(D-89, 어긋나면 409 `move_conflict`).
   */
  async function retryMove(id: string): Promise<void> {
    const p = pending[id];
    if (p?.phase !== 'not_saved') return;
    patch(id, p.seq, { phase: 'checking' });
    status = m.tasks_move_checking({ title: p.title });
    let server: TaskView[];
    try {
      server = await api.tasks(item.id);
    } catch {
      if (!current(id, p.seq)) return;
      patch(id, p.seq, { phase: 'not_saved' });
      status = m.tasks_move_unknown({ title: p.title });
      return;
    }
    if (!current(id, p.seq)) return;
    const verdict = reconcileMove(server, id, p.origin, p.plan);
    ++reloadSeq;
    confirmed = server;
    tasks = overlaid(server, id);
    if (verdict === 'not_applied') {
      // 대기 항목을 지우지 않은 채 교체해 보낸다(그사이 끌기·단추가 같은 카드를 다시 보내지 않게).
      const sent = commitMove(id, replan(server, p.plan), p.how, p.title, p.seq);
      clearMoveNotice();
      await sent;
      return;
    }
    finish(id);
    if (verdict === 'applied') {
      clearMoveNotice();
      status = m.tasks_move_confirmed({ title: p.title, status: STATUS_LABEL[p.plan.status] });
    } else {
      moveNotice =
        verdict === 'gone' ? m.tasks_move_error_gone() : m.tasks_move_changed({ title: p.title });
      status = moveNotice;
    }
  }

  /** "저장 안 됨"을 받아들이고 카드를 서버 자리에 둔다. */
  function keepAsIs(id: string): void {
    finish(id);
    moveNotice = undefined;
  }

  const SAVE_LABEL: Record<SavePhase, () => string> = {
    saving: () => m.tasks_card_saving(),
    slow: () => m.tasks_card_slow(),
    checking: () => m.tasks_card_checking(),
    unknown: () => m.tasks_card_unknown(),
    not_saved: () => m.tasks_card_not_saved(),
  };

  /** 같은 열 안 순서(위·아래)는 기본 순서에서만 바꾼다(D-79). 단추·메뉴·키보드·끌기가 같은 규칙을 따른다. */
  const reorderable = (action: BoardAction): boolean =>
    ordered || (action !== 'up' && action !== 'down');

  async function move(id: string, action: BoardAction): Promise<void> {
    if (!reorderable(action)) {
      // 제목순 같은 정렬에서 위·아래는 화면 순서와 무관하게 저장된 기본 순서를 바꾸게 되므로 보내지 않는다.
      status = m.tasks_move_unordered({ title: titleOf(id) });
      return;
    }
    const plan = planBoardMove(shown, id, action);
    if (plan === undefined) return;
    await commitMove(id, plan, action);
    document.querySelector<HTMLElement>(`[data-task-card="${CSS.escape(id)}"]`)?.focus();
  }

  // 끌어서 옮기기(마우스·터치·펜, Pointer Events). 마우스는 카드 어디서나 끌고, 터치·펜은 손잡이(⠿)에서만 끈다
  // (카드 위를 손가락으로 쓸면 화면이 그대로 스크롤된다). 조금(6px) 움직이기 전에는 끌기가 아니어서 카드를 누르면
  // 그대로 열린다. Esc나 취소(pointercancel)는 아무것도 바꾸지 않는다.
  interface Drag {
    id: string;
    pointerId: number;
    startX: number;
    startY: number;
    x: number;
    y: number;
    active: boolean;
    slot: DropSlot | undefined;
  }
  const DRAG_THRESHOLD = 6;
  const SCROLL_EDGE = 48;
  let drag = $state<Drag | undefined>(undefined);
  // 끌기를 마친 직후 같은 손짓의 click이 카드를 열지 않게 한다.
  let justDragged = false;
  const dragging = $derived(drag !== undefined);

  function startDrag(event: PointerEvent, id: string, handle: boolean): void {
    if (!canWrite || drag !== undefined || !event.isPrimary || event.button !== 0) return;
    // 앞선 이동을 저장하는 카드는 끌지 않는다(같은 카드의 요청이 겹치지 않게).
    if (busy(id)) return;
    if (!handle && event.pointerType !== 'mouse') return;
    drag = {
      id,
      pointerId: event.pointerId,
      startX: event.clientX,
      startY: event.clientY,
      x: event.clientX,
      y: event.clientY,
      active: false,
      slot: undefined,
    };
  }

  /** 포인터 아래의 열과 그 안의 자리. 열 밖이면 undefined. */
  function slotAt(x: number, y: number, id: string): DropSlot | undefined {
    const laneEl = document.elementFromPoint(x, y)?.closest<HTMLElement>('[data-lane]');
    const lane = TASK_STATUSES.find((s) => s === laneEl?.dataset.lane);
    if (laneEl === null || laneEl === undefined || lane === undefined) return undefined;
    if (!ordered) return { status: lane, beforeId: null };
    const cards: CardBox[] = [];
    for (const card of laneEl.querySelectorAll<HTMLElement>('[data-board-card]')) {
      const box = card.getBoundingClientRect();
      cards.push({ id: card.dataset.boardCard ?? '', top: box.top, height: box.height });
    }
    return { status: lane, beforeId: dropBefore(cards, id, y) };
  }

  function onDragMove(event: PointerEvent): void {
    const d = drag;
    if (d?.pointerId !== event.pointerId) return;
    // 창 밖이나 다른 창에서 단추를 놓아 pointerup을 받지 못했으면(마우스 단추가 눌려 있지 않음) 놓을 자리를 정하지
    // 않고 취소한다. 그러지 않으면 끌기가 남아 다음 클릭이 카드를 엉뚱한 자리에 놓았다.
    if (event.pointerType === 'mouse' && event.buttons === 0) {
      cancelDrag();
      return;
    }
    d.x = event.clientX;
    d.y = event.clientY;
    if (!d.active) {
      if (Math.hypot(d.x - d.startX, d.y - d.startY) < DRAG_THRESHOLD) return;
      d.active = true;
      status = m.tasks_drag_started({ title: titleOf(d.id) });
    }
    event.preventDefault();
    window.getSelection()?.removeAllRanges();
    d.slot = slotAt(d.x, d.y, d.id);
    if (d.y < SCROLL_EDGE) window.scrollBy(0, -16);
    else if (d.y > window.innerHeight - SCROLL_EDGE) window.scrollBy(0, 16);
  }

  function onDragEnd(event: PointerEvent): void {
    const d = drag;
    if (d?.pointerId !== event.pointerId) return;
    drag = undefined;
    if (!d.active) return;
    justDragged = true;
    setTimeout(() => {
      justDragged = false;
    }, 0);
    const slot = slotAt(event.clientX, event.clientY, d.id) ?? d.slot;
    const plan = slot === undefined ? undefined : planDrop(shown, d.id, slot, ordered);
    if (plan === undefined) {
      status = m.tasks_drag_unchanged({ title: titleOf(d.id) });
      return;
    }
    void commitMove(d.id, plan, 'drag');
  }

  function cancelDrag(): void {
    const d = drag;
    drag = undefined;
    if (d?.active === true) status = m.tasks_drag_cancelled({ title: titleOf(d.id) });
  }

  $effect(() => {
    if (!dragging) return;
    const onKey = (event: KeyboardEvent): void => {
      if (event.key === 'Escape') {
        event.preventDefault();
        cancelDrag();
      }
    };
    window.addEventListener('pointermove', onDragMove, { passive: false });
    window.addEventListener('pointerup', onDragEnd);
    window.addEventListener('pointercancel', cancelDrag);
    window.addEventListener('keydown', onKey);
    // 끄는 중 다른 창·탭으로 가면 놓는 순간을 볼 수 없으므로 취소한다.
    window.addEventListener('blur', cancelDrag);
    return () => {
      window.removeEventListener('pointermove', onDragMove);
      window.removeEventListener('pointerup', onDragEnd);
      window.removeEventListener('pointercancel', cancelDrag);
      window.removeEventListener('keydown', onKey);
      window.removeEventListener('blur', cancelDrag);
    };
  });

  /** 놓을 자리 표시: 이 열의 이 카드 앞(null이면 열 끝)에 놓이면 참. 제자리면 표시하지 않는다. */
  function dropMark(lane: string, beforeId: string | null): boolean {
    const d = drag;
    if (d?.active !== true || d.slot === undefined) return false;
    if (d.slot.status !== lane || d.slot.beforeId !== beforeId) return false;
    return planDrop(shown, d.id, d.slot, ordered) !== undefined;
  }

  function onCardKey(event: KeyboardEvent, id: string): void {
    if (!event.altKey || !canWrite) return;
    const map: Partial<Record<string, BoardAction>> = {
      ArrowUp: 'up',
      ArrowDown: 'down',
      ArrowLeft: 'left',
      ArrowRight: 'right',
    };
    const action = map[event.key];
    if (action === undefined) return;
    event.preventDefault();
    void move(id, action);
  }
</script>

<section class="flex flex-col gap-3" aria-labelledby="tasks-heading">
  <div class="flex flex-wrap items-center gap-2">
    <h2 id="tasks-heading" class="text-lg font-semibold">{m.tasks_heading()}</h2>
    <div class="ml-auto flex gap-1" role="group" aria-label={m.tasks_view()}>
      {#each VIEWS as v (v)}
        <Button
          size="sm"
          variant={view === v ? 'secondary' : 'ghost'}
          aria-pressed={view === v}
          onclick={() => (view = v)}>{VIEW_LABEL[v]}</Button
        >
      {/each}
    </div>
  </div>

  <div
    class="flex flex-wrap items-end gap-2 text-sm"
    role="search"
    aria-label={m.tasks_filter_region()}
  >
    <label class="flex flex-col gap-0.5">
      <span class="text-xs text-muted-foreground">{m.tasks_filter_find()}</span>
      <input
        class={['w-40', fieldBox]}
        type="search"
        bind:value={filter.text}
        placeholder={m.tasks_filter_find_placeholder()}
      />
    </label>
    <label class="flex flex-col gap-0.5">
      <span class="text-xs text-muted-foreground">{m.tasks_status()}</span>
      <select class={fieldBox} bind:value={filter.status} aria-label={m.tasks_filter_status_aria()}>
        <option value="any">{m.tasks_filter_all()}</option>
        {#each TASK_STATUSES as s (s)}<option value={s}>{STATUS_LABEL[s]}</option>{/each}
      </select>
    </label>
    <label class="flex flex-col gap-0.5">
      <span class="text-xs text-muted-foreground">{m.tasks_assignee()}</span>
      <select
        class={fieldBox}
        bind:value={filter.assignee}
        aria-label={m.tasks_filter_assignee_aria()}
      >
        <option value="any">{m.tasks_filter_all()}</option>
        <option value="me">{m.tasks_filter_mine()}</option>
        <option value="none">{m.tasks_filter_unassigned()}</option>
        {#each people as [id, name] (id)}<option value={id}>{name}</option>{/each}
      </select>
    </label>
    <label class="flex flex-col gap-0.5">
      <span class="text-xs text-muted-foreground">{m.tasks_priority()}</span>
      <select
        class={fieldBox}
        bind:value={filter.priority}
        aria-label={m.tasks_filter_priority_aria()}
      >
        <option value="any">{m.tasks_filter_all()}</option>
        {#each ['urgent', 'high', 'medium', 'low', 'none'] as const as p (p)}<option value={p}
            >{PRIORITY_LABEL[p]}</option
          >{/each}
      </select>
    </label>
    <label class="flex flex-col gap-0.5">
      <span class="text-xs text-muted-foreground">{m.tasks_tag()}</span>
      <input
        class={['w-24', fieldBox]}
        bind:value={filter.tag}
        aria-label={m.tasks_filter_tag_aria()}
      />
    </label>
    <label class="flex flex-col gap-0.5">
      <span class="text-xs text-muted-foreground">{m.tasks_filter_due_from()}</span>
      <input
        class={fieldBox}
        type="date"
        bind:value={filter.dueFrom}
        aria-label={m.tasks_filter_due_from_aria()}
      />
    </label>
    <label class="flex flex-col gap-0.5">
      <span class="text-xs text-muted-foreground">{m.tasks_filter_due_to()}</span>
      <input
        class={fieldBox}
        type="date"
        bind:value={filter.dueTo}
        aria-label={m.tasks_filter_due_to_aria()}
      />
    </label>
    <label class="flex flex-col gap-0.5">
      <span class="text-xs text-muted-foreground">{m.tasks_sort()}</span>
      <select class={fieldBox} bind:value={sort} aria-label={m.tasks_sort()}>
        {#each Object.entries(SORT_LABEL) as [value, label] (value)}<option {value}>{label}</option
          >{/each}
      </select>
    </label>
    {#if isFiltered(filter) || sort !== 'position'}
      <Button
        size="sm"
        variant="ghost"
        onclick={() => {
          filter = { ...EMPTY_FILTER };
          sort = 'position';
        }}>{m.tasks_filter_reset()}</Button
      >
    {/if}
  </div>
  <div class="flex flex-wrap items-end gap-2 text-sm">
    <label class="flex flex-col gap-0.5">
      <span class="text-xs text-muted-foreground">{m.tasks_saved_views()}</span>
      <select
        aria-label={m.tasks_saved_views()}
        class={fieldBox}
        value={selectedView}
        onchange={(e: Event) => {
          if (e.currentTarget instanceof HTMLSelectElement) applyView(e.currentTarget.value);
        }}
      >
        <option value="">{m.tasks_saved_view_choose()}</option>
        {#each views as v (v.id)}<option value={v.id}>{v.name}</option>{/each}
      </select>
    </label>
    <label class="flex flex-col gap-0.5">
      <span class="text-xs text-muted-foreground">{m.tasks_view_name()}</span>
      <input
        class={['w-32', fieldBox]}
        bind:value={viewName}
        aria-label={m.tasks_view_name_aria()}
        maxlength={100}
      />
    </label>
    <Button size="sm" variant="outline" onclick={() => void saveView()}
      >{m.tasks_view_save()}</Button
    >
    {#if selectedView !== ''}
      <Button size="sm" variant="ghost" disabled={deletingView} onclick={() => void deleteView()}
        >{m.tasks_view_delete()}</Button
      >
    {/if}
    {#if isFiltered(filter)}
      <span class="text-xs text-muted-foreground" aria-live="polite"
        >{m.tasks_filter_shown({ shown: shown.length, total: tasks.length })}</span
      >
    {/if}
  </div>
  {#if moveNotice !== undefined}
    <p role="alert" class="text-sm text-destructive">{moveNotice}</p>
  {/if}
  {#if undoOffer !== undefined}
    {@const offer = undoOffer}
    <div class="flex flex-wrap items-center gap-2 text-sm" data-move-undo>
      <span>{m.tasks_undo_offer({ title: offer.title })}</span>
      <Button
        size="sm"
        variant="outline"
        aria-label={m.tasks_undo_label({ title: offer.title })}
        onclick={() => void undoMove()}>{m.tasks_undo()}</Button
      >
    </div>
  {/if}
  {#if error !== undefined}
    <p role="alert" class="text-sm text-destructive">{error}</p>
  {/if}

  {#if loading}
    <p class="text-sm text-muted-foreground" role="status">{m.modules_loading()}</p>
  {:else if view === 'board'}
    <p class="text-xs text-muted-foreground">{m.tasks_lane_estimate_hint()}</p>
    <div class="grid gap-3 md:grid-cols-3" data-testid="task-board">
      {#each TASK_STATUSES as lane (lane)}
        <section
          data-lane={lane}
          class={[
            'flex min-w-0 flex-col gap-2 rounded-lg border bg-muted/40 p-2',
            drag?.active === true && drag.slot?.status === lane && 'ring-2 ring-ring',
          ]}
          aria-label={m.tasks_lane_aria({ status: STATUS_LABEL[lane] })}
        >
          <!-- 열 이름 옆에 카드 수를 칩으로(참고 제품의 열 머리글 배치), 그 아래 예상 시간 합계. -->
          <h3 class="flex items-center gap-2 px-1 text-sm font-semibold">
            {STATUS_LABEL[lane]}
            <span
              class="rounded-full bg-background px-2 py-0.5 text-xs font-normal text-muted-foreground tabular-nums"
              >{board[lane].length}</span
            >
          </h3>
          {#if laneEstimateText(lane) !== ''}
            <p class="px-1 text-xs text-muted-foreground" data-lane-estimate={lane}>
              {laneEstimateText(lane)}
            </p>
          {/if}
          {#if board[lane].length === 0}
            <p class="px-1 text-xs text-muted-foreground" data-lane-empty={lane}>
              {isFiltered(filter) ? m.tasks_lane_empty_filtered() : m.tasks_lane_empty()}
            </p>
          {/if}
          <ul class="flex min-h-12 flex-col gap-2">
            {#each board[lane] as t (t.id)}
              {#if dropMark(lane, t.id)}
                <li aria-hidden="true" class="h-1 rounded bg-primary"></li>
              {/if}
              <li
                data-board-card={t.id}
                class={[
                  'group flex flex-col gap-1.5 rounded-md border bg-card p-2 shadow-xs',
                  canWrite && 'cursor-grab',
                  drag?.active === true && drag.id === t.id && 'opacity-40',
                ]}
                onpointerdown={(e: PointerEvent) => {
                  startDrag(e, t.id, false);
                }}
              >
                <div class="flex items-start gap-1">
                  {#if canWrite}
                    <span
                      aria-hidden="true"
                      title={m.tasks_drag_handle()}
                      class="-ml-1 shrink-0 cursor-grab touch-none px-1 text-muted-foreground select-none"
                      onpointerdown={(e: PointerEvent) => {
                        startDrag(e, t.id, true);
                      }}
                      oncontextmenu={(e: MouseEvent) => {
                        e.preventDefault();
                      }}>⠿</span
                    >
                  {/if}
                  <button
                    type="button"
                    data-task-card={t.id}
                    aria-keyshortcuts={canWrite
                      ? 'Alt+ArrowUp Alt+ArrowDown Alt+ArrowLeft Alt+ArrowRight'
                      : undefined}
                    onkeydown={(e) => {
                      onCardKey(e, t.id);
                    }}
                    class="flex min-w-0 flex-1 flex-col items-start gap-0.5 rounded text-left text-sm hover:underline focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
                    onclick={() => {
                      if (!justDragged) open(t.id);
                    }}
                  >
                    <span class="font-mono text-xs text-muted-foreground">{t.reference ?? ''}</span>
                    <span class="max-w-full min-w-0 font-medium break-words">{t.title}</span>
                  </button>
                  {#if canWrite}
                    <DropdownMenu.Root>
                      <DropdownMenu.Trigger>
                        {#snippet child({ props })}
                          <Button
                            {...props}
                            size="sm"
                            variant="ghost"
                            class="-mt-0.5 -mr-1 h-6 shrink-0 px-1.5 text-xs"
                            aria-label={m.tasks_move_menu({ title: t.title })}>⋯</Button
                          >
                        {/snippet}
                      </DropdownMenu.Trigger>
                      <DropdownMenu.Content align="end">
                        {#each TASK_STATUSES.filter((s) => s !== t.status) as target (target)}
                          <DropdownMenu.Item
                            disabled={busy(t.id)}
                            onSelect={() =>
                              void commitMove(t.id, { status: target, beforeId: null }, 'menu')}
                            >{MOVE_TO[target]()}</DropdownMenu.Item
                          >
                        {/each}
                        <!-- 같은 열 안 순서: 화살표 줄은 보조 표시(aria-hidden)이고 Alt+위·아래는 키보드에서만 되므로, 터치
                             화면의 낭독기 사용자도 쓸 수 있게 메뉴에도 둔다. 정렬이 기본 순서가 아니면 순서를 바꿀 수 없다. -->
                        {#if ordered}
                          <DropdownMenu.Separator />
                          {#each ['up', 'down'] as const as action (action)}
                            <DropdownMenu.Item
                              disabled={busy(t.id) ||
                                planBoardMove(shown, t.id, action) === undefined}
                              onSelect={() => void move(t.id, action)}
                              >{action === 'up'
                                ? m.tasks_move_up_menu()
                                : m.tasks_move_down_menu()}</DropdownMenu.Item
                            >
                          {/each}
                        {/if}
                      </DropdownMenu.Content>
                    </DropdownMenu.Root>
                  {/if}
                </div>
                {#if t.parent_id !== null}
                  <span class="text-xs text-muted-foreground">↳ {titleOf(t.parent_id)}</span>
                {/if}
                <div
                  class="flex flex-wrap items-center gap-x-2 gap-y-1 text-xs text-muted-foreground"
                >
                  {#if t.priority !== 'none'}<span
                      class={['rounded px-1.5 py-0.5 font-medium', PRIORITY_TONE[t.priority]]}
                      >{PRIORITY_LABEL[t.priority]}</span
                    >{/if}
                  {#if t.due_date !== null}<span>{m.tasks_card_due({ date: t.due_date })}</span
                    >{/if}
                  {#if t.checklist_total > 0}<span>☑ {t.checklist_done}/{t.checklist_total}</span
                    >{/if}
                  {#if t.assignees.length > 0}<span
                      >{t.assignees.map((a) => a.display_name).join(', ')}</span
                    >{/if}
                </div>
                {#each [pending[t.id]] as save (save?.seq ?? -1)}
                  {#if save !== undefined}
                    <div
                      class={[
                        'flex flex-wrap items-center gap-1.5 text-xs',
                        save.phase === 'unknown' || save.phase === 'not_saved'
                          ? 'text-destructive'
                          : 'text-muted-foreground',
                      ]}
                      data-save-state={save.phase}
                    >
                      <span>{SAVE_LABEL[save.phase]()}</span>
                      {#if save.phase === 'slow' || save.phase === 'unknown'}
                        <Button
                          size="sm"
                          variant="outline"
                          class="h-6 px-1.5 text-xs"
                          onclick={() => {
                            recheck(t.id);
                          }}>{m.tasks_move_recheck()}</Button
                        >
                      {:else if save.phase === 'not_saved'}
                        <Button
                          size="sm"
                          variant="outline"
                          class="h-6 px-1.5 text-xs"
                          onclick={() => void retryMove(t.id)}>{m.tasks_move_retry()}</Button
                        >
                        <Button
                          size="sm"
                          variant="ghost"
                          class="h-6 px-1.5 text-xs"
                          onclick={() => {
                            keepAsIs(t.id);
                          }}>{m.tasks_move_keep()}</Button
                        >
                      {/if}
                    </div>
                  {/if}
                {/each}
                {#if canWrite}
                  <!-- 마우스로 쓰는 화면에서는 가리키거나 카드 안에 초점이 있을 때만 보인다(자리는 그대로 둬 카드 높이가
                       바뀌지 않게). 터치 화면은 가리키기가 없으므로 늘 보인다. 키보드는 Alt+방향키를 쓴다. -->
                  <div
                    class="flex gap-1 transition-opacity pointer-fine:opacity-0 pointer-fine:group-focus-within:opacity-100 pointer-fine:group-hover:opacity-100"
                    aria-hidden="true"
                  >
                    {#each ['left', 'up', 'down', 'right'] as const as action (action)}
                      <Button
                        size="sm"
                        variant="ghost"
                        tabindex={-1}
                        class="h-6 px-1.5 text-xs"
                        disabled={busy(t.id) ||
                          !reorderable(action) ||
                          planBoardMove(shown, t.id, action) === undefined}
                        onclick={() => void move(t.id, action)}
                        >{action === 'left'
                          ? '←'
                          : action === 'right'
                            ? '→'
                            : action === 'up'
                              ? '↑'
                              : '↓'}</Button
                      >
                    {/each}
                  </div>
                {/if}
              </li>
            {/each}
            {#if dropMark(lane, null)}
              <li aria-hidden="true" class="h-1 rounded bg-primary"></li>
            {/if}
          </ul>
          {#if canWrite}
            {#if adding === lane}
              <CreateForm
                id={`new-task-${lane}`}
                label={m.tasks_quick_add_label({ status: STATUS_LABEL[lane] })}
                placeholder={m.tasks_new_placeholder()}
                submitLabel={m.tasks_create()}
                create={(requestId: string, title: string) => quickAdd(lane, requestId, title)}
                autofocus
                oncancel={() => void closeQuickAdd(lane)}
              />
            {:else}
              <Button
                size="sm"
                variant="ghost"
                class="self-start"
                data-quick-add={lane}
                onclick={() => (adding = lane)}>{m.tasks_quick_add()}</Button
              >
            {/if}
          {/if}
        </section>
      {/each}
    </div>
    {#if drag?.active === true}
      <div
        aria-hidden="true"
        class="pointer-events-none fixed z-50 max-w-60 truncate rounded-md border bg-card px-2 py-1 text-sm shadow-md"
        style:left={`${String(drag.x + 12)}px`}
        style:top={`${String(drag.y + 12)}px`}
      >
        {titleOf(drag.id)}
      </div>
    {/if}
    {#if canWrite}
      <p class="text-xs text-muted-foreground">
        {m.tasks_board_hint()}{ordered ? '' : ` ${m.tasks_board_hint_unordered()}`}
      </p>
    {/if}
  {:else if view === 'gantt'}
    {#if ganttNotice !== undefined}
      <p role="alert" class="text-sm text-destructive">{ganttNotice}</p>
    {/if}
    {#if ganttInfo !== ''}
      <p class="text-sm text-muted-foreground" data-testid="gantt-undated-notice">{ganttInfo}</p>
    {/if}
    <GanttChart tasks={shown} onopen={open} reschedule={canWrite ? reschedule : undefined} />
  {:else if view === 'calendar'}
    <div bind:this={boardCalendar}>
      <TaskCalendar
        tasks={shown}
        onopen={open}
        oncreate={canWrite
          ? (day: string) => {
              createDay = day;
            }
          : undefined}
      />
    </div>
  {:else}
    <table class="w-full text-sm" data-testid="task-list">
      <thead class="text-left text-muted-foreground">
        <tr
          ><th class="py-1 font-medium">{m.tasks_col_number()}</th><th class="font-medium"
            >{m.tasks_title()}</th
          ><th class="font-medium">{m.tasks_status()}</th><th class="font-medium"
            >{m.tasks_assignee()}</th
          ><th class="font-medium">{m.tasks_col_due()}</th><th class="font-medium"
            >{m.tasks_col_checklist()}</th
          ></tr
        >
      </thead>
      <tbody>
        {#each list as t (t.id)}
          <tr class="border-t">
            <td class="py-1 font-mono text-xs text-muted-foreground">{t.reference ?? ''}</td>
            <td class={t.parent_id === null ? '' : 'pl-4'}>
              <button
                type="button"
                data-task-card={t.id}
                class="text-left hover:underline focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
                onclick={() => {
                  open(t.id);
                }}>{t.parent_id === null ? '' : '↳ '}{t.title}</button
              >
            </td>
            <td>{STATUS_LABEL[t.status]}</td>
            <td>{t.assignees.map((a) => a.display_name).join(', ')}</td>
            <td>{t.due_date ?? ''}</td>
            <td
              >{t.checklist_total > 0
                ? `${String(t.checklist_done)}/${String(t.checklist_total)}`
                : ''}</td
            >
          </tr>
        {:else}
          <tr><td colspan="6" class="py-2 text-muted-foreground">{m.tasks_empty()}</td></tr>
        {/each}
      </tbody>
    </table>
  {/if}

  {#if canWrite}
    <CreateForm
      id="new-task"
      label={m.tasks_new_title()}
      placeholder={m.tasks_new_placeholder()}
      submitLabel={m.tasks_create()}
      create={async (requestId: string, title: string) => {
        await api.createTask(item.id, requestId, title);
        await reload();
      }}
    />
    <TaskCsvImport itemId={item.id} onimported={reload} />
  {/if}
  <p class="sr-only" aria-live="polite">{status}</p>
</section>

<!-- 닫기를 대화상자가 정하지 않고 requestClose()가 정한다(고친 입력이 있으면 열어 둔 채 묻는다). -->
<Dialog.Root
  bind:open={
    () => openTaskId !== undefined,
    (o: boolean) => {
      if (!o) requestClose();
    }
  }
>
  <Dialog.Content
    bind:ref={dialogContent}
    class="max-h-[90vh] overflow-y-auto sm:max-w-3xl"
    escapeKeydownBehavior={confirmClose || detailSaving ? 'ignore' : 'close'}
    interactOutsideBehavior={confirmClose || detailSaving ? 'ignore' : 'close'}
    showCloseButton={!detailSaving}
    onCloseAutoFocus={(e: Event) => {
      const card =
        returnTo === undefined
          ? null
          : document.querySelector<HTMLElement>(`[data-task-card="${CSS.escape(returnTo)}"]`);
      if (card !== null) {
        e.preventDefault();
        card.focus();
      }
    }}
  >
    <Dialog.Header>
      <Dialog.Title
        >{openTaskId === undefined ? '' : titleOf(openTaskId) || m.tasks_task()}</Dialog.Title
      >
      <Dialog.Description>{m.tasks_dialog_description()}</Dialog.Description>
    </Dialog.Header>
    {#if openTaskId !== undefined}
      {#key openTaskId}
        <TaskDetail
          taskId={openTaskId}
          {tasks}
          {workspaces}
          onchanged={() => void reload()}
          onopen={requestOpen}
          onunsaved={(value: boolean) => {
            detailUnsaved = value;
          }}
          onsaving={(value: boolean) => {
            detailSaving = value;
          }}
        />
      {/key}
    {/if}
    <DiscardChangesConfirm
      open={confirmClose}
      description={openNext === undefined ? undefined : m.tasks_discard_open_description()}
      confirmLabel={openNext === undefined ? undefined : m.tasks_discard_open_confirm()}
      onkeep={() => {
        confirmClose = false;
      }}
      ondiscard={discard}
      returnFocus={() => (keepFocus?.isConnected === true ? keepFocus : dialogContent)}
    />
  </Dialog.Content>
</Dialog.Root>

<CalendarTaskDialog
  workspaceId={item.workspace_id}
  day={createDay}
  item={{ id: item.id, title: item.title }}
  onclose={(created: boolean) => {
    const day = createDay;
    createDay = undefined;
    // 업무는 만들었지만 마감일을 넣지 못한 채 닫았다: 그 업무가 보드에 보이게 다시 읽는다.
    if (created) void reload();
    if (day !== undefined) void focusCreate(day);
  }}
  oncreated={(task: TaskDetailView) => {
    const day = createDay;
    createDay = undefined;
    // 만든 뒤에도 그 날짜의 만들기 단추로 초점을 돌린다(달력을 다시 읽어도 단추는 그대로 있다).
    if (day !== undefined) void focusCreate(day);
    notifyDone(
      m.events_task_created({
        title: task.title,
        day: task.due_date === null ? '' : formatDay(task.due_date),
      }),
    );
    void reload();
  }}
/>

<Dialog.Root
  open={ganttAsk !== undefined}
  onOpenChange={(o: boolean) => {
    if (!o) cancelGantt();
  }}
>
  <Dialog.Content
    class="sm:max-w-lg"
    escapeKeydownBehavior={ganttBusy ? 'ignore' : 'close'}
    interactOutsideBehavior={ganttBusy ? 'ignore' : 'close'}
    showCloseButton={!ganttBusy}
    onCloseAutoFocus={(e: Event) => {
      e.preventDefault();
    }}
  >
    {#if ganttAsk !== undefined}
      {@const ask = ganttAsk}
      <Dialog.Header>
        <Dialog.Title>{m.tasks_gantt_dialog_title({ title: ask.task.title })}</Dialog.Title>
        <Dialog.Description>
          {m.tasks_gantt_dialog_description({
            from: datesText(ask.task.start_date, ask.task.due_date),
            to: datesText(ask.input.start_date, ask.input.due_date),
          })}
        </Dialog.Description>
      </Dialog.Header>
      <ShiftProposals
        proposals={ask.proposals}
        undated={ask.undated}
        blocked={ask.blocked}
        retried={ask.retried}
        busy={ganttBusy}
        headingId="gantt-shift-heading"
        ondecide={(withShifts: boolean) => void decideGantt(withShifts)}
        oncancel={cancelGantt}
      />
    {/if}
  </Dialog.Content>
</Dialog.Root>
