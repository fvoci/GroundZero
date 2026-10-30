import { tick, untrack } from 'svelte';
import { SvelteURLSearchParams } from 'svelte/reactivity';
import type { ItemView } from '$lib/api/client';
import { newRequestId } from '$lib/api/client';
import type { SavedViewView } from '$lib/features/events/saved-views-model';
import { savedViewsApi } from '$lib/features/events/saved-views-api';
import { canonicalJson } from '$lib/features/documents/drafts';
import { notifyDone } from '$lib/features/feedback/feedback.svelte';
import { createFormDraft } from '$lib/features/common/form-draft.svelte';
import type { createLeaveGuard } from '$lib/features/common/leave-guard.svelte';
import { errorText } from '$lib/api/errors';
import { org } from '$lib/org.svelte';
import { readSession, writeSession } from '$lib/storage';
import { itemViewPath, router } from '$lib/router.svelte';
import { m } from '$lib/paraglide/messages.js';
import type { TaskFilter, TaskSort, TaskViewMode } from './filters';
import { EMPTY_FILTER, isRecord, readConfig } from './filters';

interface Context {
  readonly item: ItemView;
  readonly routeView: TaskViewMode | undefined;
  readonly leave: ReturnType<typeof createLeaveGuard>;
  error: string | undefined;
  status: string;
}

/** URL > saved view > session; all view input and request identity share this owner. */
export function createBoardViews(context: Context) {
  let view = $state<TaskViewMode>('board');
  // 검색·필터·정렬과 저장한 보기(D-31).
  let filter = $state<TaskFilter>({ ...EMPTY_FILTER });
  let sort = $state<TaskSort>('position');
  let filtersOpen = $state(false);
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
  const isViewRequest = (id: string): boolean => viewRequest?.id === id;
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
    preserve: false,
    name: () => `task-view:${context.item.id}`,
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
      filter = value.config.filter;
      sort = value.config.sort;
      viewDefault = value.config.default;
      viewRequest = value.request;
      // 보기는 다른 보기 전환처럼 주소로 바꾼다(D-121). 되살린 이 입력을 버릴지 이 보드가 다시 묻지 않는다.
      context.leave.proceed(() => {
        changeView(value.config.view);
      });
    },
    reset: () => {
      viewName = views.find((v) => v.id === selectedView)?.name ?? '';
      viewRequest = null;
    },
    dirty: () => viewDirty,
  });
  let contextTouched = false;
  /**
   * 사용자가 보기·필터·정렬·검색을 만졌으면 늦게 온 기본 저장 보기를 적용하지 않는다. 새 업무 제목·CSV 칸처럼 보기와
   * 무관한 입력은 세지 않는다(R5 최종 A 확인 필요 3). 보기 단추와 저장한 보기 고르기는 각자 켠다.
   */
  function touchContext(event: Event): void {
    if (event.target instanceof Element && event.target.closest('[role="search"]') !== null)
      contextTouched = true;
  }
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
  let contextFor = untrack(() => contextKey(context.item.id));
  restoreContext(untrack(() => context.item.id));
  $effect(() => {
    const id = context.item.id;
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
  // 경로의 보기는 저장 문맥보다 우선하며 경로 없는 진입은 주소를 바꾸지 않는다.
  function applyUrl(): void {
    const route = router.route;
    if (context.routeView !== undefined) view = context.routeView;
    if (route.name !== 'item' || route.itemId !== context.item.id) return;
    if (context.routeView === undefined && route.view !== undefined) view = route.view;
    const query = new SvelteURLSearchParams(window.location.search);
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
    router.navigate(itemViewPath(context.item.workspace_id, context.item.id, next));
    if (router.path === itemViewPath(context.item.workspace_id, context.item.id, next)) view = next;
  }
  async function loadViews(itemId: string, restore = false): Promise<void> {
    const key = contextKey(itemId);
    const loaded = await savedViewsApi.savedViews(itemId).catch(() => []);
    if (itemId !== context.item.id || key !== contextKey(context.item.id)) return;
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
    context.leave.request(() => {
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
      context.error = m.tasks_view_name_required();
      return;
    }
    const itemId = context.item.id;
    if (viewDefault && views.some((v) => v.name !== name && v.config.default === true)) {
      context.error = m.tasks_default_already_set();
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
          ? await savedViewsApi.createView(itemId, requestId, name, config)
          : await savedViewsApi.updateView(
              requestedTarget.id,
              requestId,
              requestedTarget.version,
              config,
            );
      if (!viewDraft.isOwner(draftAttempt)) return;
      // 응답이 준 보기(저장한 것 또는 409의 최신)는 목록 다시 읽기가 실패해도 남긴다. 비면 다시 저장이 PUT 대신 POST가
      // 되어 또 409였다(CodeRabbit).
      const returned = result.kind === 'saved' ? result.view : result.current;
      const reread = await savedViewsApi.savedViews(itemId).catch(() => views);
      if (context.item.id !== itemId) return;
      views = withView(reread, returned);
      if (selectedView === selectedAtStart) selectedView = returned.id;
      if (result.kind !== 'saved') {
        viewRequest = null;
        // 다른 탭이 이 이름으로 먼저 만들거나 저장했다. 서버는 아무것도 바꾸지 않았다. 지금 화면의 설정과 이름은 그대로
        // 두고 최신 목록만 읽는다(최신 설정은 목록에서 고르면 보이고, 다시 저장하면 최신 판을 기준으로 이 설정이 된다).
        context.status = '';
        context.error = m.tasks_view_conflict({ name });
        return;
      }
      if (!viewDraft.complete(draftAttempt)) {
        // 저장하는 사이 이름·검색어·필터가 바뀌었다. 바뀐 입력은 두되, 보낸 요청은 저장돼 소진됐다. 남기면
        // `viewDirty`가 참으로 굳어 떠나기 확인과 되살리기 제안이 계속 떴다(R5 최종 A FA-3).
        // 그사이 다시 저장을 눌러 새 요청이 나갔으면 그것은 두고 이 요청만 비운다.
        if (isViewRequest(requestId)) viewRequest = null;
        viewDraft.persist();
      }
      notifyDone(m.tasks_view_saved({ name }));
      context.error = undefined;
    } catch (e: unknown) {
      context.error = errorText(e, m.tasks_view_save_failed());
    }
  }

  async function deleteView(): Promise<void> {
    if (selectedView === '' || deletingView) return;
    deletingView = true;
    try {
      await savedViewsApi.deleteView(selectedView);
      selectedView = '';
      askingViewDelete = false;
      await loadViews(context.item.id);
      notifyDone(m.tasks_view_deleted());
    } catch (e: unknown) {
      context.error = errorText(e, m.tasks_view_delete_failed());
    } finally {
      deletingView = false;
    }
  }

  return {
    get view() {
      return view;
    },
    set view(value: typeof view) {
      view = value;
    },
    get filter() {
      return filter;
    },
    set filter(value: typeof filter) {
      filter = value;
    },
    get sort() {
      return sort;
    },
    set sort(value: typeof sort) {
      sort = value;
    },
    get filtersOpen() {
      return filtersOpen;
    },
    set filtersOpen(value: typeof filtersOpen) {
      filtersOpen = value;
    },
    get views() {
      return views;
    },
    set views(value: typeof views) {
      views = value;
    },
    get selectedView() {
      return selectedView;
    },
    set selectedView(value: typeof selectedView) {
      selectedView = value;
    },
    get deletingView() {
      return deletingView;
    },
    set deletingView(value: typeof deletingView) {
      deletingView = value;
    },
    get askingViewDelete() {
      return askingViewDelete;
    },
    set askingViewDelete(value: typeof askingViewDelete) {
      askingViewDelete = value;
    },
    get viewDeleteConfirm() {
      return viewDeleteConfirm;
    },
    set viewDeleteConfirm(value: typeof viewDeleteConfirm) {
      viewDeleteConfirm = value;
    },
    get viewName() {
      return viewName;
    },
    set viewName(value: typeof viewName) {
      viewName = value;
    },
    get viewDefault() {
      return viewDefault;
    },
    set viewDefault(value: typeof viewDefault) {
      viewDefault = value;
    },
    get viewRequest() {
      return viewRequest;
    },
    set viewRequest(value: typeof viewRequest) {
      viewRequest = value;
    },
    get viewDirty() {
      return viewDirty;
    },
    viewDraft,
    touchContext,
    changeView,
    loadViews,
    applyView,
    saveView,
    deleteView,
    askViewDelete,
    stopViewDelete,
  };
}
