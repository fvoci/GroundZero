<script lang="ts">
  // 항목의 업무 (D-31): 칸반·목록·간트·달력, 검색·필터·정렬과 저장한 보기, 업무 만들기, 상세 대화상자.
  // 칸반 카드는 버튼과 키보드(Alt+화살표)로 열·순서를 옮긴다. 상세는 주소(/t/{업무})로도 열린다.
  import type { TaskStatusDefinition, Workspace } from '@fvoci/api-client';
  import { tick, untrack } from 'svelte';
  import { SvelteMap, SvelteSet } from 'svelte/reactivity';

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
  import { api, ApiError, newRequestId } from '$lib/api/client';
  import { canonicalJson } from '$lib/features/documents/drafts';
  import { errorText } from '$lib/api/errors';
  import { org } from '$lib/org.svelte';
  import { readSession, writeSession } from '$lib/storage';
  import StatusEditor from './StatusEditor.svelte';
  import { me } from '$lib/me.svelte';
  import { m } from '$lib/paraglide/messages.js';
  import CreateForm from '$lib/features/workspaces/CreateForm.svelte';
  import TaskCsvImport from './TaskCsvImport.svelte';
  import { itemViewPath, itemTaskPath, router } from '$lib/router.svelte';
  import { Checkbox } from '$lib/ui/checkbox';
  import { Button } from '$lib/ui/button';
  import * as Collapsible from '$lib/ui/collapsible';
  import * as DropdownMenu from '$lib/ui/dropdown-menu';
  import * as Dialog from '$lib/ui/dialog';
  import { Input } from '$lib/ui/input';
  import { NativeSelect } from '$lib/ui/native-select';
  import { durationText } from '$lib/features/time/format';
  import TaskTimeStart from '$lib/features/time/TaskTimeStart.svelte';
  import CalendarTaskDialog from '$lib/features/events/CalendarTaskDialog.svelte';
  import DiscardChangesConfirm from '$lib/features/common/DiscardChangesConfirm.svelte';
  import type { StoredFormDraft } from '$lib/features/common/form-draft';
  import {
    clearFormDraft,
    holdFormDraft,
    listFormDrafts,
    removeFormDraftAt,
    storeFormDraft,
  } from '$lib/features/common/form-draft';
  import { createLeaveGuard } from '$lib/features/common/leave-guard.svelte';
  import { createFormDraft } from '$lib/features/common/form-draft.svelte';
  import FormDraftOffer from '$lib/features/common/FormDraftOffer.svelte';
  import { accountSwitching, notifyDone } from '$lib/features/feedback/feedback.svelte';
  import { formatDay, formatInstant } from '$lib/display';

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
    hiddenConditionCount,
    isFiltered,
    isRecord,
    readConfig,
    sortTasks,
    VIEWS,
  } from './filters';
  import { PRIORITY_LABEL, PRIORITY_TONE, undatedText } from './labels';
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
    /** 항목 화면이 읽은 URL 보기. 독립 렌더에서는 라우터의 같은 항목 경로를 읽는다. */
    routeView?: TaskViewMode | undefined;
    /** 옮기기가 권한·보관 때문에 거절되면 항목을 다시 읽게 한다(쓰기 단추를 끄기 위해). */
    onstale?: () => void;
    /** 항목 밖의 읽기 전용 사유(보관된 모듈). 서버가 쓰기를 거부하므로 추가·이동 조작을 끈다. */
    locked?: boolean;
    /** 같은 화면의 다른 곳(항목 달력)에서 업무를 만들 때마다 늘어난다(목록을 다시 읽는다). */
    tasksVersion?: number;
  }

  const {
    item,
    workspaces,
    openTaskId,
    routeView,
    onstale,
    locked = false,
    tasksVersion = 0,
  }: Props = $props();
  $effect(() => {
    if (tasksVersion === 0) return;
    untrack(() => {
      void reload();
    });
  });

  const editingUser = untrack(() => me.userId);
  let accountBlocked = $state(false);
  const hiddenForAccount = $derived(accountBlocked || me.userId !== editingUser);
  let definitions = $state<TaskStatusDefinition[]>([]);
  const statusNames = $derived(definitions.map((s) => s.name));
  let editingStatuses = $state(false);
  let tasks = $state<TaskView[]>([]);
  let view = $state<TaskViewMode>('board');
  let error = $state<string | undefined>(undefined);
  let status = $state('');
  // 옮기기 실패 안내. 항목을 다시 읽어 목록을 새로 받아도(error를 지움) 남는다. 다음 옮기기가 성공하면 지운다.
  let moveNotice = $state<string | undefined>(undefined);
  let moveConflict = $state<{ id: string; title: string; plan: BoardMove } | undefined>();
  let savingConflict = $state(false);
  async function saveMoveMine(): Promise<void> {
    const mine = moveConflict;
    if (mine === undefined || savingConflict) return;
    const latest = confirmed.find((t) => t.id === mine.id);
    if (latest === undefined) return;
    savingConflict = true;
    try {
      if (
        await commitMove(
          mine.id,
          replan(confirmed, mine.plan),
          'menu',
          mine.title,
          undefined,
          undefined,
          latest.move_version,
        )
      )
        moveConflict = undefined;
    } finally {
      savingConflict = false;
    }
  }

  // 검색·필터·정렬과 저장한 보기(D-31).
  let filter = $state<TaskFilter>({ ...EMPTY_FILTER });
  let sort = $state<TaskSort>('position');
  let filtersOpen = $state(false);
  const hiddenConditions = $derived(hiddenConditionCount(filter, sort));
  let views = $state<SavedViewView[]>([]);
  let selectedView = $state('');
  // 보기를 지우는 중(두 번 눌러 같은 보기를 다시 지우려다 404 실패를 보이지 않게).
  let deletingView = $state(false);
  // 저장한 보기 지우기를 묻는 중. 확인 단추로 초점을 옮기고, 그만두면 지우기 단추로 돌려준다.
  let askingViewDelete = $state(false);
  let viewDeleteConfirm = $state<HTMLButtonElement | null>(null);
  async function askViewDelete(): Promise<void> {
    askingViewDelete = true;
    await tick();
    viewDeleteConfirm?.focus();
  }
  async function stopViewDelete(): Promise<void> {
    askingViewDelete = false;
    await tick();
    document.getElementById('view-delete')?.focus();
  }
  let viewName = $state('');
  let viewDefault = $state(false);
  interface ViewRequest {
    id: string;
    payload: string;
    target: { id: string; version: number } | null;
  }
  let viewRequest = $state<ViewRequest | null>(null);
  const viewDirty = $derived(
    viewRequest !== null ||
      viewName.trim() !== (views.find((v) => v.id === selectedView)?.name ?? ''),
  );
  interface ViewDraft {
    name: string;
    config: { view: TaskViewMode; filter: TaskFilter; sort: TaskSort; default: boolean };
    request: ViewRequest | null;
  }
  const viewDraft = createFormDraft<ViewDraft>({
    name: () => `task-view:${item.id}`,
    snapshot: () => ({
      name: viewName,
      config: { view, filter: { ...filter }, sort, default: viewDefault },
      request: viewRequest,
    }),
    parse: (value) => {
      if (!isRecord(value) || typeof value.name !== 'string' || !isRecord(value.config))
        return undefined;
      let request: ViewRequest | null = null;
      if (
        isRecord(value.request) &&
        typeof value.request.id === 'string' &&
        typeof value.request.payload === 'string'
      ) {
        const target = value.request.target;
        if (target === null) {
          request = { id: value.request.id, payload: value.request.payload, target: null };
        } else if (
          isRecord(target) &&
          typeof target.id === 'string' &&
          typeof target.version === 'number'
        ) {
          request = {
            id: value.request.id,
            payload: value.request.payload,
            target: { id: target.id, version: target.version },
          };
        }
      }
      return {
        name: value.name,
        config: { ...readConfig(value.config), default: value.config.default === true },
        request,
      };
    },
    restore: (value) => {
      contextTouched = true;
      viewName = value.name;
      view = value.config.view;
      filter = value.config.filter;
      sort = value.config.sort;
      viewDefault = value.config.default;
      viewRequest = value.request;
    },
    reset: () => {
      viewName = views.find((v) => v.id === selectedView)?.name ?? '';
      viewRequest = null;
    },
    dirty: () => viewDirty,
  });
  let contextTouched = false;
  // 같은 이름·설정의 재시도는 같은 요청 id, 고치면 새 id.
  // 보기·필터·정렬은 이 탭에서 항목마다 기억한다. 다른 화면에 갔다 돌아와도 그대로다(이 브라우저 탭에만 남는 개인
  // 편의라 저장한 보기와 다르다). 저장소를 쓸 수 없으면 기억하지 않는다.
  const contextKey = (itemId: string): string => org.key(`board:${itemId}`);
  function restoreContext(itemId: string): void {
    let raw: unknown = null;
    try {
      raw = JSON.parse(readSession(contextKey(itemId)) ?? 'null');
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
  let contextFor = untrack(() => contextKey(item.id));
  restoreContext(untrack(() => item.id));
  $effect(() => {
    const id = item.id;
    const snapshot = JSON.stringify({ view, filter, sort });
    const key = contextKey(id);
    if (contextFor !== key) {
      // 다른 항목으로 바뀌었다: 그 항목의 맥락을 불러온다(지금 것을 그 항목 이름으로 남기지 않는다).
      contextFor = key;
      contextTouched = false;
      untrack(() => {
        restoreContext(id);
      });
      return;
    }
    try {
      writeSession(contextKey(id), snapshot);
    } catch {
      // 저장소를 쓸 수 없으면 기억하지 않는다.
    }
  });
  // 작업자 A: 경로의 보기는 저장 문맥보다 우선하며 경로 없는 진입은 주소를 바꾸지 않는다.
  function applyUrl(): void {
    const route = router.route;
    if (routeView !== undefined) view = routeView;
    if (route.name !== 'item' || route.itemId !== item.id) return;
    if (routeView === undefined && route.view !== undefined) view = route.view;
    const query = new URLSearchParams(window.location.search);
    const queryStatus = query.get('status');
    const querySort = query.get('sort');
    if (queryStatus !== null) filter.status = queryStatus;
    if (querySort !== null) sort = readConfig({ sort: querySort }).sort;
  }
  $effect(() => {
    applyUrl();
  });
  function changeView(next: TaskViewMode): void {
    contextTouched = true;
    router.navigate(itemViewPath(item.workspace_id, item.id, next));
    if (router.path === itemViewPath(item.workspace_id, item.id, next)) view = next;
  }
  const shown = $derived(sortTasks(applyFilter(tasks, filter, me.userId), sort));
  const people = $derived(assigneesOf(tasks));
  const ordered = $derived(sort === 'position');

  async function loadViews(itemId: string, restore = false): Promise<void> {
    const key = contextKey(itemId);
    const loaded = await api.savedViews(itemId).catch(() => []);
    if (itemId !== item.id || key !== contextKey(item.id)) return;
    views = loaded;
    if (!restore || contextTouched) return;
    const defaults = loaded.filter((v) => v.config.default === true);
    const chosen = defaults.length === 1 ? defaults[0] : undefined;
    if (chosen === undefined) return;
    const config = readConfig(chosen.config);
    view = config.view;
    filter = config.filter;
    sort = config.sort;
    selectedView = chosen.id;
    viewName = chosen.name;
    viewDefault = true;
    applyUrl();
  }

  /** `list`에 `saved`가 없거나 더 오래된 판이면 `saved`로 바꾼다(이름순은 다음 다시 읽기에 맡긴다). */
  function withView(list: SavedViewView[], saved: SavedViewView): SavedViewView[] {
    const found = list.find((v) => v.id === saved.id);
    if (found !== undefined && found.version >= saved.version) return list;
    return found === undefined
      ? [...list, saved]
      : list.map((v) => (v.id === saved.id ? saved : v));
  }

  function applyView(id: string): void {
    leave.request(() => {
      applyViewNow(id);
    });
  }
  function applyViewNow(id: string): void {
    askingViewDelete = false;
    selectedView = id;
    const v = views.find((x) => x.id === id);
    if (v === undefined) return;
    const config = readConfig(v.config);
    filter = config.filter;
    sort = config.sort;
    viewName = v.name;
    viewDefault = v.config.default === true;
    changeView(config.view);
    applyUrl();
  }

  async function saveView(): Promise<void> {
    const name = viewName.trim();
    if (name === '') {
      error = m.tasks_view_name_required();
      return;
    }
    const itemId = item.id;
    if (viewDefault && views.some((v) => v.name !== name && v.config.default === true)) {
      error = m.tasks_default_already_set();
      return;
    }
    const config = { view, filter: { ...filter }, sort, default: viewDefault };
    // 목록에 같은 이름의 보기가 있으면 읽은 판으로 그 보기를 고치고, 없으면 새로 만든다(D-103). 재시도 키에 대상과
    // 판을 넣어, 목록을 다시 읽은 뒤의 저장은 새 요청이 된다.
    const target = views.find((v) => v.name === name);
    // 기다리는 사이 사용자가 목록에서 다른 보기를 고르면 늦은 응답이 그 선택을 되돌리지 않는다.
    const selectedAtStart = selectedView;
    const payload = canonicalJson({
      item: itemId,
      name,
      config,
    });
    if (viewRequest?.payload !== payload)
      viewRequest = {
        id: newRequestId(),
        payload,
        target: target === undefined ? null : { id: target.id, version: target.version },
      };
    const requestId = viewRequest.id;
    const requestedTarget = viewRequest.target;
    const draftAttempt = viewDraft.capture();
    if (draftAttempt === undefined) return;
    try {
      const result =
        requestedTarget === null
          ? await api.createView(itemId, requestId, name, config)
          : await api.updateView(requestedTarget.id, requestId, requestedTarget.version, config);
      if (!viewDraft.isOwner(draftAttempt)) return;
      // 응답이 준 보기(저장한 것 또는 409의 최신)는 목록 다시 읽기가 실패해도 남긴다. 비면 다시 저장이 PUT 대신 POST가
      // 되어 또 409였다(CodeRabbit).
      const returned = result.kind === 'saved' ? result.view : result.current;
      const reread = await api.savedViews(itemId).catch(() => views);
      if (item.id !== itemId) return;
      views = withView(reread, returned);
      if (selectedView === selectedAtStart) selectedView = returned.id;
      if (result.kind !== 'saved') {
        viewRequest = null;
        // 다른 탭이 이 이름으로 먼저 만들거나 저장했다. 서버는 아무것도 바꾸지 않았다. 지금 화면의 설정과 이름은 그대로
        // 두고 최신 목록만 읽는다(최신 설정은 목록에서 고르면 보이고, 다시 저장하면 최신 판을 기준으로 이 설정이 된다).
        status = '';
        error = m.tasks_view_conflict({ name });
        return;
      }
      viewDraft.complete(draftAttempt);
      notifyDone(m.tasks_view_saved({ name }));
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
      askingViewDelete = false;
      await loadViews(item.id);
      notifyDone(m.tasks_view_deleted());
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
          ? m.tasks_undone_end({ title, status: lane })
          : m.tasks_undone({ title, status: lane }),
    });
    await focusCard(u.id);
  }
  async function focusCard(id: string): Promise<void> {
    await tick();
    document.querySelector<HTMLElement>(`[data-task-card="${CSS.escape(id)}"]`)?.focus();
  }
  /**
   * 키보드·메뉴로 옮기고 그 카드에 초점을 둔다. 카드는 저장 전에 새 열에 다시 그려지므로(앞 요소는 사라짐) 바로 새 자리로
   * 초점을 옮긴다: 저장이 늦거나 끝나지 않아도 초점이 문서 처음으로 빠지지 않게. 저장이 끝나면(실패면 원래 자리로 돌아와
   * 다시 그려짐) 초점을 잃은 경우에만 다시 둔다. 기다리는 동안 다른 곳으로 옮긴 초점은 빼앗지 않는다. 화면을 떠난 뒤 온
   * 응답도 초점을 건드리지 않는다(같은 항목을 다시 열어 새 보드가 있어도 그 카드로 초점을 가져가지 않게).
   */
  async function moveAndFocus(
    id: string,
    plan: BoardMove,
    how: BoardAction | 'menu',
  ): Promise<void> {
    const sent = commitMove(id, plan, how);
    await focusCard(id);
    await sent;
    if (!alive) return;
    const active = document.activeElement;
    if (active === null || active === document.body) await focusCard(id);
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
      moveConflict = undefined;
    });
  });

  async function reload(itemId: string = item.id): Promise<void> {
    const mine = ++reloadSeq;
    try {
      const [server, statuses] = await Promise.all([api.tasks(itemId), api.taskStatuses(itemId)]);
      if (!alive || mine !== reloadSeq || itemId !== item.id) return;
      definitions = [...statuses].sort((a, b) => a.position - b.position);
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
    if (write) await api.syncRecurrences(editingUser, itemId).catch(() => 0);
    await reload(itemId);
  }

  $effect(() => {
    if (hiddenForAccount) return;
    void loadItem(item.id, item.access !== 'read' && !item.archived);
    void loadViews(item.id, true);
  });

  const canWrite = $derived(
    !hiddenForAccount && item.access !== 'read' && !item.archived && !locked,
  );
  const board = $derived(lanes(shown, statusNames));
  // 열 머리의 예상 합계(D-31 예상 시간). 거르기 중이면 보이는 카드 기준과 열 전체를 함께 보인다(실제 시간은 목록에 없어 넣지 않음).
  const allLanes = $derived(lanes(tasks, statusNames));
  function laneEstimateText(lane: TaskStatus): string {
    const shownSum = laneEstimate(board[lane] ?? []);
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
      const all = laneEstimate(allLanes[lane] ?? []);
      parts.push(m.tasks_lane_estimate_all({ duration: durationText(all.minutes * 60) }));
    }
    return parts.join(' · ');
  }
  const list = $derived(ordered ? listOrder(shown) : shown);
  const titleOf = (id: string | null): string =>
    id === null ? '' : (tasks.find((t) => t.id === id)?.title ?? '');

  function open(id: string): void {
    const navigate = (): void => {
      router.navigate(itemTaskPath(item.workspace_id, item.id, id));
    };
    // 같은 보드 위에 상세를 여는 동안 새 업무·보기 입력은 그대로 남는다. 상세 간 이동은 requestOpen의 확인을 거친다.
    if (openTaskId === undefined) leave.proceed(navigate);
    else navigate();
  }

  // 주소로 연 대화상자는 여는 단추를 모르므로, 닫으면 마지막으로 연 업무의 카드로 초점을 돌린다.
  let returnTo: string | undefined;
  function restoreClosedTaskFocus(id: string): void {
    const itemId = item.id;
    // 폐기 확인과 상세가 함께 닫힐 때는 두 focus scope가 정리된 다음 카드로 돌린다.
    void tick().then(() => {
      requestAnimationFrame(() => {
        if (!alive || item.id !== itemId || openTaskId !== undefined) return;
        document.querySelector<HTMLElement>(`[data-task-card="${CSS.escape(id)}"]`)?.focus();
      });
    });
  }
  $effect(() => {
    if (openTaskId !== undefined) returnTo = openTaskId;
    // 다른 업무를 열었거나 닫혔으면 앞 업무에 대해 물은 것을 거둔다(새로 연 업무를 닫거나 그 입력을 버리지 않게).
    leave.keep();
  });

  // 계속 편집하면 물을 때 초점이 있던 곳(대화상자 밖이었으면 대화상자)으로 돌아간다.
  let detailUnsaved = $state(false);
  // 폼을 저장하는 동안은 닫지도 묻지도 않는다: 이미 보낸 저장은 서버에 남으므로 "버리고 닫기"가 거짓이 된다(일정 대화상자와 같음).
  let detailSaving = $state(false);
  // 열린 업무의 고친 입력이 이 탭의 브라우저 초안으로 남았는지(TaskDetail이 알린다). 전환 중 떠나기 경고의 기준.
  let detailDraftKept = $state(true);
  let detailEditor = $state<{ discardChildInput(): boolean }>();
  // 상세 안의 단추(상위·하위·선행·후속·첫 회차)로 다른 업무를 열려다 물었으면 그 업무. 버리기를 고르면 닫지 않고 그것을
  // 연다. 물을 때마다 정한다(확인이 닫히는 동안 문구가 바뀌지 않게 지우지 않는다).
  let openNext = $state<string | undefined>(undefined);
  let keepFocus: HTMLElement | null = null;
  let dialogContent = $state<HTMLElement | null>(null);
  // 상세 안의 이동·닫기는 보드 입력을 없애지 않는다. 실제 보드 이탈만 보드 입력까지 확인·폐기한다.
  let detailNavigation = false;
  let discardBoardInput = true;

  const leave = createLeaveGuard({
    dirty: () =>
      (detailUnsaved && openTaskId !== undefined) ||
      (!detailNavigation && (newDirty.size > 0 || viewDirty)),
    busy: () => detailSaving,
    identity: () => openTaskId,
    onAsk: () => {
      discardBoardInput = !detailNavigation;
      const active = document.activeElement;
      keepFocus =
        active instanceof HTMLElement && dialogContent?.contains(active) ? active : dialogContent;
      openNext = undefined;
    },
  });

  $effect(() =>
    leave.attach({
      // 다른 계정으로 바뀌어 다시 읽는 중: 고친 입력이 앞 계정의 초안으로 남았으면 막지 않는다(D-96·#88: 막고 남으면 앞 계정의
      // 화면과 입력이 새 계정의 세션으로 요청을 보낸다). 남기지 못했으면(저장소 사용 불가·용량 초과) 보통 떠나기처럼 경고해
      // 입력을 잃지 않게 한다 — 문서 편집기(DocumentEditor)와 같은 규칙. 전환이 아니면 전처럼 경고한다.
      beforeUnload: () => {
        if (openTaskId === undefined)
          return accountSwitching()
            ? (viewDirty && !viewDraft.kept) || [...newKept.values()].some((kept) => !kept)
            : newDirty.size > 0 || viewDirty;
        // 편집 가능 여부(canEdit)와 무관하게 "고친 입력이 있는데 초안을 남기지 못했는지"만 본다(TaskDetail이 알린다; 고친 칸이
        // 없거나 폼이 사라지면 true). 저장이 409 account_changed로 끝나 폼이 잠긴 뒤에도 입력은 지켜야 한다.
        if (accountSwitching())
          return (
            !detailDraftKept ||
            (viewDirty && !viewDraft.kept) ||
            [...newKept.values()].some((kept) => !kept)
          );
        return detailUnsaved || detailSaving || newDirty.size > 0 || viewDirty;
      },
    }),
  );

  /** Esc·바깥 클릭·닫기 단추. 저장하지 않은 입력이 있으면 닫지 않고 버릴지 묻는다. */
  function requestClose(): void {
    requestDetail(close);
  }

  function requestDetail(action: () => void): void {
    detailNavigation = true;
    try {
      leave.request(action);
    } finally {
      detailNavigation = false;
    }
  }

  /**
   * 상세 안에서 다른 업무를 연다. 열면 폼을 새로 그리므로(`{#key openTaskId}`) 닫기와 같이 저장하지 않은 입력이 있으면
   * 버릴지 묻고, 저장하는 동안은 옮기지 않는다(CodeRabbit #82).
   */
  function requestOpen(id: string): void {
    if (leave.open || detailSaving) return;
    if (id === openTaskId) {
      leave.proceed(() => {
        open(id);
      });
      return;
    }
    requestDetail(() => {
      open(id);
    });
    openNext = id;
  }

  function close(): void {
    router.navigate(itemViewPath(item.workspace_id, item.id, 'board'));
  }

  /** 옮긴 뒤 화면 낭독기에 알리는 글. */
  function movedText(action: BoardAction, title: string, lane: TaskStatus): string {
    const values = { title, status: lane };
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

  // 열마다 빠르게 더하기(Kaneo처럼 열 아래). 만들기는 "할 일"로 만들어지므로, 다른 열이면 같은 이동 저장 경로로 그
  // 열 끝에 옮긴다. 옮기기가 실패하면 업무는 "할 일"에 남고 실패를 알린다(만든 것은 사라지지 않는다).
  let adding = $state<TaskStatus | undefined>(undefined);
  let newDirty = $state<Set<string | null>>(new Set());
  let newKept = $state<Map<string | null, boolean>>(new Map());

  // 만들지 않은 새 업무 제목의 브라우저 보존(D-96·D-128; 문서 초안 D-27과 같은 규칙, common/form-draft.ts). 아래 "새 업무
  // 제목"과 열의 빠른 추가 입력을 이 사용자·이 탭의 키에 남기고, 다시 읽은 뒤 같은 계정이 이 항목의 보드를 열면 되살릴지
  // 묻는다. 되살리면 그 입력란에 제목과 그때의 request_id를 시드하고 사용자가 "업무 만들기"를 눌러야 보낸다(같은 제목이면 같은
  // request_id라 응답을 잃었던 만들기도 중복되지 않는다). 만들었거나 명시적으로 버리거나 비우면 지우고, 이탈 전에도 확인한다.
  interface NewTaskDraft {
    lane: string | null;
    name: string;
    requestId: string | null;
    savedAt: string;
  }
  function newDraftName(itemId: string, lane: string | null): string {
    return `task-new:${itemId}:${lane === null ? '-' : encodeURIComponent(lane)}`;
  }
  function parseNewTaskDraft(value: unknown): NewTaskDraft | undefined {
    if (typeof value !== 'object' || value === null) return undefined;
    if (!('lane' in value && 'name' in value && 'requestId' in value && 'savedAt' in value))
      return undefined;
    const { lane, name, requestId, savedAt } = value;
    if (
      (lane !== null && typeof lane !== 'string') ||
      typeof name !== 'string' ||
      (requestId !== null && typeof requestId !== 'string') ||
      typeof savedAt !== 'string'
    )
      return undefined;
    return { lane, name, requestId, savedAt };
  }
  /**
   * 남은 입력을 읽고 이 탭의 키에 있는 것은 보류 키로 옮긴다. 입력란(자식)이 그려지며 비어 있다고 알려 이 탭의 키를 지우기
   * 전에 끝나야 하므로 처음에는 효과가 아니라 초기화에서 부른다.
   */
  function loadNewDrafts(itemId: string): StoredFormDraft<NewTaskDraft>[] {
    return listFormDrafts(`task-new:${itemId}`, parseNewTaskDraft).map((entry) =>
      holdFormDraft(newDraftName(itemId, entry.value.lane), entry),
    );
  }
  let draftsItem = untrack(() => item.id);
  let newDrafts = $state(loadNewDrafts(draftsItem));
  $effect(() => {
    const itemId = item.id;
    if (itemId === draftsItem) return;
    draftsItem = itemId;
    untrack(() => {
      newDrafts = loadNewDrafts(itemId);
    });
  });
  function keepNewDraft(
    lane: string | null,
    draft: { name: string; requestId: string | null } | undefined,
  ): void {
    if (me.userId !== editingUser) return;
    const next = new SvelteSet(newDirty);
    if (draft === undefined) next.delete(lane);
    else next.add(lane);
    newDirty = next;
    const name = newDraftName(item.id, lane);
    const kept = new SvelteMap(newKept);
    if (draft === undefined) {
      clearFormDraft(name);
      kept.delete(lane);
    } else {
      kept.set(
        lane,
        storeFormDraft(name, {
          lane,
          name: draft.name,
          requestId: draft.requestId,
          savedAt: new Date().toISOString(),
        } satisfies NewTaskDraft),
      );
    }
    newKept = kept;
  }
  // 되살린 입력의 시드. 열의 빠른 추가는 열릴 때 읽고, 아래 폼은 다시 그려 읽는다.
  let quickInitial = $state<{ lane: string; name: string; requestId: string | null } | undefined>();
  let newTaskInitial = $state<{ name: string; requestId: string | null } | undefined>();
  let newTaskSeed = $state(0);
  // 되살린 열의 시드는 그 열의 폼이 열려 있는 동안만 산다. 닫거나 다른 열을 열면 지워, 같은 열을 다시 열 때 옛 제목과 옛
  // request_id가 다시 채워지지 않게 한다(CodeRabbit #141; 아래 폼의 시드는 {#key newTaskSeed}로 되살릴 때만 새로 읽는다).
  $effect(() => {
    if (quickInitial !== undefined && adding !== quickInitial.lane) quickInitial = undefined;
  });
  function restoreNewDraft(entry: StoredFormDraft<NewTaskDraft>): void {
    const kept = entry.value;
    removeFormDraftAt(entry.key);
    newDrafts = newDrafts.filter((e) => e.key !== entry.key);
    const seed = { name: kept.name, requestId: kept.requestId };
    if (kept.lane !== null && statusNames.includes(kept.lane)) {
      quickInitial = { lane: kept.lane, ...seed };
      adding = kept.lane;
    } else {
      // 그 열이 이제 없으면 아래 폼(기본 상태)으로 되살린다.
      newTaskInitial = seed;
      newTaskSeed += 1;
    }
  }
  function discardNewDraft(entry: StoredFormDraft<NewTaskDraft>): void {
    removeFormDraftAt(entry.key);
    newDrafts = newDrafts.filter((e) => e.key !== entry.key);
  }
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
    noShiftNeeded = shifts.length === 0,
  ): Promise<'saved' | 'asked' | 'stopped'> {
    const result = await api.updateTask(
      editingUser,
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
    notifyDone(noShiftNeeded ? `${savedText} ${m.tasks_gantt_no_shifts()}` : savedText);
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
        a.proposals.length === 0,
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
    const created = await api
      .createTask(editingUser, itemId, requestId, title, null, { status: lane })
      .catch((e: unknown) => {
        if (e instanceof ApiError && e.code === 'account_changed') accountBlocked = true;
        throw e;
      });
    if (!alive || itemId !== item.id) return;
    ++reloadSeq;
    confirmed = [...confirmed.filter((t) => t.id !== created.id), created];
    tasks = overlaid(confirmed);
    notifyDone(m.common_saved());
    await reload();
  }

  /** 빠른 추가를 닫고(Esc) 그 열의 "업무 추가" 단추로 초점을 돌린다. */
  async function closeQuickAdd(lane: TaskStatus): Promise<void> {
    adding = undefined;
    await tick();
    [...document.querySelectorAll<HTMLElement>('[data-quick-add]')]
      .find((button) => button.dataset.quickAdd === lane)
      ?.focus();
  }

  function movedMessage(how: PendingMove['how'], title: string, lane: TaskStatus): string {
    return how === 'drag'
      ? m.tasks_moved_drag({ title, status: lane })
      : how === 'menu'
        ? m.tasks_moved_menu({ title, status: lane })
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
    explicitVersion?: number,
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
        editingUser,
        id,
        plan.status,
        plan.beforeId,
        explicitVersion ?? (guarded ? origin.moveVersion : undefined),
      );
      if (!current(id, seq)) return true;
      // 성공 응답이 확정이다: 다시 읽기 전에 옮기는 다른 카드의 원점이 이 결과를 보도록 확정 목록에 먼저 반영한다.
      // #50: 응답 전 시작한 조회는 이 성공 판을 덮지 못한다. 다른 카드 원점도 최신 응답을 본다.
      ++reloadSeq;
      confirmed = applyMove(
        confirmed.map((t) => (t.id === id ? saved : t)),
        id,
        plan,
      );
      finish(id);
      tasks = overlaid(confirmed);
      await reload();
      if (!alive || seq !== moveSeq) return true;
      clearMoveNotice();
      status =
        undo === undefined ? movedMessage(how, title, plan.status) : undo.done(title, plan.status);
      if (undo === undefined) offerUndo(id, title, origin, plan, saved.move_version);
      return true;
    } catch (e: unknown) {
      if (!current(id, seq)) return false;
      if (e instanceof ApiError && e.code === 'account_changed') {
        accountBlocked = true;
        finish(id);
        return false;
      }
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
      if (conflicted) moveConflict = { id, title, plan };
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
      status = m.tasks_move_confirmed({ title: p.title, status: p.plan.status });
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
      status = m.tasks_move_confirmed({ title: p.title, status: p.plan.status });
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
    const plan = planBoardMove(shown, id, action, statusNames);
    if (plan === undefined) return;
    await moveAndFocus(id, plan, action);
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
    const lane = statusNames.find((s) => s === laneEl?.dataset.lane);
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

{#if hiddenForAccount}<p role="alert">{m.tasks_account_changed()}</p>{/if}
<section
  hidden={hiddenForAccount}
  inert={hiddenForAccount}
  class="flex flex-col gap-3"
  aria-labelledby="tasks-heading"
  oninput={() => {
    contextTouched = true;
  }}
  onchange={() => {
    contextTouched = true;
  }}
>
  <div class="flex flex-wrap items-center gap-2">
    <h2 id="tasks-heading" class="text-lg font-semibold">{m.tasks_heading()}</h2>
    <div class="ml-auto flex flex-wrap gap-1" role="group" aria-label={m.tasks_view()}>
      {#each VIEWS as v (v)}
        <Button
          size="sm"
          variant={view === v ? 'secondary' : 'ghost'}
          aria-pressed={view === v}
          onclick={() => {
            changeView(v);
          }}>{VIEW_LABEL[v]}</Button
        >
      {/each}
    </div>
  </div>

  {#each newDrafts as entry (entry.key)}
    <div
      class="flex flex-wrap items-center gap-2 rounded border px-3 py-2 text-sm"
      role="region"
      aria-label={m.tasks_draft_region()}
      data-testid="task-new-draft-offer"
    >
      <span
        >{entry.value.lane === null
          ? m.tasks_new_draft_found({
              title: entry.value.name,
              time: formatInstant(entry.value.savedAt),
            })
          : m.tasks_new_draft_found_lane({
              status: entry.value.lane,
              title: entry.value.name,
              time: formatInstant(entry.value.savedAt),
            })}</span
      >
      {#if canWrite}
        <Button
          size="sm"
          variant="outline"
          onclick={() => {
            restoreNewDraft(entry);
          }}>{m.tasks_draft_restore()}</Button
        >
      {/if}
      <Button
        size="sm"
        variant="ghost"
        onclick={() => {
          discardNewDraft(entry);
        }}>{m.tasks_draft_discard()}</Button
      >
    </div>
  {/each}

  {#if item.access === 'owner' && !locked}
    <Button
      variant="outline"
      class="self-start"
      aria-expanded={editingStatuses}
      onclick={() => (editingStatuses = !editingStatuses)}>{m.tasks_status_edit()}</Button
    >
    {#if editingStatuses}
      <StatusEditor
        itemId={item.id}
        statuses={definitions}
        archived={item.archived}
        onchanged={reload}
      />
    {/if}
  {/if}

  <!-- 찾기와 "필터·정렬" 단추만 늘 보이고 나머지 조건·저장한 보기는 펼쳐서 쓴다(좁은 화면에서 업무보다 조건이 먼저 한
       화면을 차지했다). 접혀 있어도 단추가 걸린 조건 수를 보인다. 접어도 조건 값은 그대로다. -->
  <Collapsible.Root
    bind:open={filtersOpen}
    class="flex flex-col gap-2 text-sm"
    role="search"
    aria-label={m.tasks_filter_region()}
  >
    <div class="flex flex-wrap items-end gap-2">
      <label class="flex flex-col gap-0.5">
        <span class="text-xs text-muted-foreground">{m.tasks_filter_find()}</span>
        <Input
          class="w-40"
          type="search"
          bind:value={filter.text}
          placeholder={m.tasks_filter_find_placeholder()}
        />
      </label>
      <Collapsible.Trigger>
        {#snippet child({ props })}
          <Button {...props} variant="outline" data-testid="task-filter-toggle">
            {m.tasks_filter_toggle()}
            {#if hiddenConditions > 0}
              <span
                class="rounded-full bg-primary px-1.5 text-xs text-primary-foreground tabular-nums"
                aria-hidden="true">{hiddenConditions}</span
              >
              <span class="sr-only">{m.tasks_filter_active({ count: hiddenConditions })}</span>
            {/if}
          </Button>
        {/snippet}
      </Collapsible.Trigger>
      {#if isFiltered(filter) || sort !== 'position'}
        <Button
          variant="ghost"
          onclick={() => {
            filter = { ...EMPTY_FILTER };
            sort = 'position';
          }}>{m.tasks_filter_reset()}</Button
        >
      {/if}
      {#if isFiltered(filter)}
        <span class="self-center text-xs text-muted-foreground" aria-live="polite"
          >{m.tasks_filter_shown({ shown: shown.length, total: tasks.length })}</span
        >
      {/if}
    </div>
    <Collapsible.Content class="flex flex-col gap-2">
      <div class="flex flex-wrap items-end gap-2">
        <label class="flex flex-col gap-0.5">
          <span class="text-xs text-muted-foreground">{m.tasks_status()}</span>
          <NativeSelect
            class="w-full"
            bind:value={filter.status}
            aria-label={m.tasks_filter_status_aria()}
          >
            <option value="any">{m.tasks_filter_all()}</option>
            {#if filter.status !== 'any' && !statusNames.includes(filter.status)}
              <option value={filter.status}>{filter.status}</option>
            {/if}
            {#each statusNames as s (s)}<option value={s}>{s}</option>{/each}
          </NativeSelect>
        </label>
        <label class="flex flex-col gap-0.5">
          <span class="text-xs text-muted-foreground">{m.tasks_assignee()}</span>
          <NativeSelect
            class="w-full"
            bind:value={filter.assignee}
            aria-label={m.tasks_filter_assignee_aria()}
          >
            <option value="any">{m.tasks_filter_all()}</option>
            <option value="me">{m.tasks_filter_mine()}</option>
            <option value="none">{m.tasks_filter_unassigned()}</option>
            {#each people as [id, name] (id)}<option value={id}>{name}</option>{/each}
          </NativeSelect>
        </label>
        <label class="flex flex-col gap-0.5">
          <span class="text-xs text-muted-foreground">{m.tasks_priority()}</span>
          <NativeSelect
            class="w-full"
            bind:value={filter.priority}
            aria-label={m.tasks_filter_priority_aria()}
          >
            <option value="any">{m.tasks_filter_all()}</option>
            {#each ['urgent', 'high', 'medium', 'low', 'none'] as const as p (p)}<option value={p}
                >{PRIORITY_LABEL[p]}</option
              >{/each}
          </NativeSelect>
        </label>
        <label class="flex flex-col gap-0.5">
          <span class="text-xs text-muted-foreground">{m.tasks_tag()}</span>
          <Input class="w-24" bind:value={filter.tag} aria-label={m.tasks_filter_tag_aria()} />
        </label>
        <label class="flex flex-col gap-0.5">
          <span class="text-xs text-muted-foreground">{m.tasks_filter_due_from()}</span>
          <Input
            class="w-auto"
            type="date"
            bind:value={filter.dueFrom}
            aria-label={m.tasks_filter_due_from_aria()}
          />
        </label>
        <label class="flex flex-col gap-0.5">
          <span class="text-xs text-muted-foreground">{m.tasks_filter_due_to()}</span>
          <Input
            class="w-auto"
            type="date"
            bind:value={filter.dueTo}
            aria-label={m.tasks_filter_due_to_aria()}
          />
        </label>
        <label class="flex flex-col gap-0.5">
          <span class="text-xs text-muted-foreground">{m.tasks_sort()}</span>
          <NativeSelect class="w-full" bind:value={sort} aria-label={m.tasks_sort()}>
            {#each Object.entries(SORT_LABEL) as [value, label] (value)}<option {value}
                >{label}</option
              >{/each}
          </NativeSelect>
        </label>
      </div>
      <div class="flex flex-wrap items-end gap-2">
        <label class="flex flex-col gap-0.5">
          <span class="text-xs text-muted-foreground">{m.tasks_saved_views()}</span>
          <NativeSelect
            aria-label={m.tasks_saved_views()}
            class="w-full"
            value={selectedView}
            onchange={(e: Event) => {
              if (e.currentTarget instanceof HTMLSelectElement) applyView(e.currentTarget.value);
            }}
          >
            <option value="">{m.tasks_saved_view_choose()}</option>
            {#each views as v (v.id)}<option value={v.id}>{v.name}</option>{/each}
          </NativeSelect>
        </label>
        {#each viewDraft.offers as entry (entry.key)}
          <FormDraftOffer
            onrestore={() => {
              viewDraft.restore(entry);
            }}
            ondiscard={() => {
              viewDraft.discard(entry);
            }}
          />
        {/each}
        <label class="flex flex-col gap-0.5">
          <span class="text-xs text-muted-foreground">{m.tasks_view_name()}</span>
          <Input
            class="w-32"
            bind:value={viewName}
            aria-label={m.tasks_view_name_aria()}
            maxlength={100}
          />
        </label>
        <label class="flex items-center gap-2"
          ><Checkbox bind:checked={viewDefault} />{m.tasks_default_view()}</label
        >
        <Button variant="outline" onclick={() => void saveView()}>{m.tasks_view_save()}</Button>
        {#if selectedView !== '' && askingViewDelete}
          <!-- 저장한 보기 지우기는 되돌릴 수 없어 한 번 더 묻는다(다른 지우기와 같다). -->
          <span class="flex flex-wrap items-center gap-1 self-center text-sm">
            <span id="view-delete-warning"
              >{m.tasks_view_delete_confirm({
                name: views.find((v) => v.id === selectedView)?.name ?? '',
              })}</span
            >
            <Button
              variant="destructive"
              bind:ref={viewDeleteConfirm}
              aria-describedby="view-delete-warning"
              disabled={deletingView}
              onclick={() => void deleteView()}>{m.tasks_view_delete()}</Button
            >
            <Button variant="ghost" onclick={() => void stopViewDelete()}
              >{m.common_cancel()}</Button
            >
          </span>
        {:else if selectedView !== ''}
          <Button id="view-delete" variant="ghost" onclick={() => void askViewDelete()}
            >{m.tasks_view_delete()}</Button
          >
        {/if}
      </div>
    </Collapsible.Content>
  </Collapsible.Root>
  {#if moveNotice !== undefined}
    <div class="flex flex-wrap items-center gap-2">
      <p role="alert" class="text-sm text-destructive">{moveNotice}</p>
      <Button
        size="sm"
        variant="ghost"
        onclick={() => {
          moveNotice = undefined;
          moveConflict = undefined;
        }}>{m.common_close()}</Button
      >
    </div>
  {/if}
  {#if moveConflict !== undefined}
    <div class="flex flex-wrap items-center gap-2">
      <p>
        {m.tasks_move_requested({ title: moveConflict.title, status: moveConflict.plan.status })}
      </p>
      <Button variant="outline" disabled={savingConflict} onclick={() => void reload()}
        >{m.tasks_move_latest()}</Button
      >
      <Button disabled={savingConflict || !canWrite} onclick={() => void saveMoveMine()}
        >{m.tasks_save_mine()}</Button
      >
    </div>
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
    <div class="grid gap-3 @2xl:grid-cols-3" data-testid="task-board">
      {#each statusNames as lane (lane)}
        <section
          data-lane={lane}
          class={[
            'flex min-w-0 flex-col gap-2 rounded-lg border bg-muted/40 p-2',
            drag?.active === true && drag.slot?.status === lane && 'ring-2 ring-ring',
          ]}
          aria-label={m.tasks_lane_aria({ status: lane })}
        >
          <!-- 열 이름 옆에 카드 수를 칩으로(참고 제품의 열 머리글 배치), 그 아래 예상 시간 합계. -->
          <h3 class="flex items-center gap-2 px-1 text-sm font-semibold">
            <span
              class="size-2 rounded-full bg-muted-foreground"
              style:background-color={definitions.find((s) => s.name === lane)?.color}
              aria-hidden="true"
            ></span>
            {lane}
            <span
              class="rounded-full bg-background px-2 py-0.5 text-xs font-normal text-muted-foreground tabular-nums"
              >{board[lane]?.length ?? 0}</span
            >
          </h3>
          {#if laneEstimateText(lane) !== ''}
            <p class="px-1 text-xs text-muted-foreground" data-lane-estimate={lane}>
              {laneEstimateText(lane)}
            </p>
          {/if}
          {#if (board[lane]?.length ?? 0) === 0}
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
                  <!-- 시간 기록 시작은 업무 카드 안에 둔다(D-117). -->
                  <TaskTimeStart taskId={t.id} title={t.title} />
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
                        {#each statusNames.filter((s) => s !== t.status) as target (target)}
                          <DropdownMenu.Item
                            disabled={busy(t.id)}
                            onSelect={() =>
                              void moveAndFocus(t.id, { status: target, beforeId: null }, 'menu')}
                            >{m.tasks_move_to_status({ status: target })}</DropdownMenu.Item
                          >
                        {/each}
                        <!-- 같은 열 안 순서: 화살표 줄은 보조 표시(aria-hidden)이고 Alt+위·아래는 키보드에서만 되므로, 터치
                             화면의 낭독기 사용자도 쓸 수 있게 메뉴에도 둔다. 정렬이 기본 순서가 아니면 순서를 바꿀 수 없다. -->
                        {#if ordered}
                          <DropdownMenu.Separator />
                          {#each ['up', 'down'] as const as action (action)}
                            <DropdownMenu.Item
                              disabled={busy(t.id) ||
                                planBoardMove(shown, t.id, action, statusNames) === undefined}
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
                          planBoardMove(shown, t.id, action, statusNames) === undefined}
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
                label={m.tasks_quick_add_label({ status: lane })}
                placeholder={m.tasks_new_placeholder()}
                submitLabel={m.tasks_create()}
                create={(requestId: string, title: string) => quickAdd(lane, requestId, title)}
                autofocus
                oncancel={() => void closeQuickAdd(lane)}
                initial={quickInitial?.lane === lane ? quickInitial : undefined}
                onchange={(draft: { name: string; requestId: string | null } | undefined) => {
                  keepNewDraft(lane, draft);
                }}
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
    <!-- 열 머리 예상 시간의 뜻. 업무보다 먼저 보이지 않게 보드 아래에 둔다. -->
    <p class="text-xs text-muted-foreground">{m.tasks_lane_estimate_hint()}</p>
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
    <!-- 여섯 칸 표는 좁은 화면에서 옆으로 넘치므로 표만 가로로 스크롤한다(기록 표와 같다). -->
    <div class="overflow-x-auto">
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
                <!-- 시간 기록 시작은 업무 행 안에 둔다(D-117). -->
                <TaskTimeStart taskId={t.id} title={t.title} />
              </td>
              <td>{t.status}</td>
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
    </div>
  {/if}

  {#if canWrite}
    {#key newTaskSeed}
      <CreateForm
        id="new-task"
        label={m.tasks_new_title()}
        placeholder={m.tasks_new_placeholder()}
        submitLabel={m.tasks_create()}
        create={async (requestId: string, title: string) => {
          await api.createTask(editingUser, item.id, requestId, title);
          await reload();
        }}
        initial={newTaskInitial}
        onchange={(draft: { name: string; requestId: string | null } | undefined) => {
          keepNewDraft(null, draft);
        }}
      />
    {/key}
    <TaskCsvImport itemId={item.id} statuses={definitions} onimported={reload} />
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
    escapeKeydownBehavior={leave.open || detailSaving ? 'ignore' : 'close'}
    interactOutsideBehavior={leave.open || detailSaving ? 'ignore' : 'close'}
    showCloseButton={!detailSaving}
    onCloseAutoFocus={(e: Event) => {
      if (returnTo !== undefined) {
        e.preventDefault();
        restoreClosedTaskFocus(returnTo);
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
          bind:this={detailEditor}
          taskId={openTaskId}
          {tasks}
          {workspaces}
          onchanged={() => void reload()}
          onopen={requestOpen}
          onunsaved={(value: boolean) => {
            detailUnsaved = value;
          }}
          ondraftkept={(kept: boolean) => {
            detailDraftKept = kept;
          }}
          onsaving={(value: boolean) => {
            detailSaving = value;
          }}
        />
      {/key}
    {/if}
  </Dialog.Content>
</Dialog.Root>

<DiscardChangesConfirm
  open={leave.open}
  description={openNext === undefined ? undefined : m.tasks_discard_open_description()}
  confirmLabel={openNext === undefined ? undefined : m.tasks_discard_open_confirm()}
  onkeep={leave.keep}
  ondiscard={() => {
    if (detailEditor?.discardChildInput() === false) return;
    if (discardBoardInput) {
      for (const lane of newDirty) clearFormDraft(newDraftName(item.id, lane));
      newDirty = new Set();
      newKept = new Map();
      newTaskInitial = undefined;
      newTaskSeed += 1;
      adding = undefined;
      quickInitial = undefined;
      viewName = views.find((v) => v.id === selectedView)?.name ?? '';
      viewDraft.persist();
    }
    leave.discard();
  }}
  returnFocus={() => (keepFocus?.isConnected === true ? keepFocus : dialogContent)}
/>

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
