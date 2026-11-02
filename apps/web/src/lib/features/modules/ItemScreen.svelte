<script lang="ts">
  import { savedViewsApi } from '$lib/features/events/saved-views-api';
  // 항목 하나: 제목·설명·공개 범위, 보관, 항목 아래 문서 (D-14, D-20, D-59).
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import Ellipsis from '@lucide/svelte/icons/ellipsis';
  import MarkButtons from '$lib/features/home/MarkButtons.svelte';
  import { taskScreenApi } from '$lib/features/tasks/screen-api';
  import DeleteButton from '$lib/features/trash/DeleteButton.svelte';
  import type { Module, ModuleKind, Workspace } from '@fvoci/api-client';
  import { getContext, onDestroy, onMount, tick, untrack } from 'svelte';
  import { createFormDraft } from '$lib/features/common/form-draft.svelte';
  import type { CreateFormContext } from '$lib/features/common/form-draft';
  import FormDraftOffer from '$lib/features/common/FormDraftOffer.svelte';
  import DiscardChangesConfirm from '$lib/features/common/DiscardChangesConfirm.svelte';
  import { object } from './draft-values';

  import type { ItemView, SummaryView } from '$lib/api/client';
  import { api, ApiError, newRequestId } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import {
    accountSafety,
    accountSwitching,
    notifyDone,
  } from '$lib/features/feedback/feedback.svelte';
  import FieldConflict from '$lib/features/common/FieldConflict.svelte';
  import { selectedFields } from '$lib/features/common/field-conflict';
  import {
    DiscardedTaskRead,
    readTaskSessionGeneration,
    type ContextCapture,
  } from '$lib/features/tasks/context-read';
  import {
    TASK_SESSION_RECOVERY,
    TaskSessionUnsettled,
    readTaskWithRecovery,
    sameTaskRequest,
    type TaskSessionRecovery,
  } from '$lib/features/tasks/session-recovery';
  import { me } from '$lib/me.svelte';
  import AttachmentsPanel from '$lib/features/files/AttachmentsPanel.svelte';
  import CoursePanel from '$lib/features/courses/CoursePanel.svelte';
  import DocumentTree from '$lib/features/documents/DocumentTree.svelte';
  import type { TreeMove } from '$lib/features/documents/tree';
  import { moveThenReload } from '$lib/features/documents/tree';
  import { forgetReferences } from '$lib/features/references/resolver';
  import Backlinks from '$lib/features/references/Backlinks.svelte';
  import TaskBoard from '$lib/features/tasks/TaskBoard.svelte';
  import type { TaskStartupSeed, TaskStartupReads } from '$lib/features/tasks/startup-reads';
  import { readConfig } from '$lib/features/tasks/filters';
  import ItemEvents from '$lib/features/events/ItemEvents.svelte';
  import CalendarScreen from '$lib/features/events/CalendarScreen.svelte';
  import TemplatePicker from '$lib/features/documents/TemplatePicker.svelte';
  import RecordsPanel from '$lib/features/records/RecordsPanel.svelte';
  import StudyPanel from '$lib/features/study/StudyPanel.svelte';
  import ItemTimePanel from '$lib/features/time/ItemTimePanel.svelte';
  import CreateForm from '$lib/features/workspaces/CreateForm.svelte';
  import { workspaceNav } from '$lib/features/nav/workspace-nav.svelte';
  import { m } from '$lib/paraglide/messages.js';
  import { org } from '$lib/org.svelte';
  import type { ItemGroupName, TaskViewName } from '$lib/router.svelte';
  import {
    ITEM_GROUP_DEFAULT,
    itemDocumentPath,
    itemGroupPath,
    itemGroupsFor,
    modulePath,
    router,
    staysOnItem,
    TASK_VIEWS,
    workspacePath,
  } from '$lib/router.svelte';
  import { readSession, writeSession } from '$lib/storage';
  import { Badge } from '$lib/ui/badge';
  import { Button } from '$lib/ui/button';
  import { Checkbox } from '$lib/ui/checkbox';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';
  import { Textarea } from '$lib/ui/textarea';
  import * as Collapsible from '$lib/ui/collapsible';
  import * as DropdownMenu from '$lib/ui/dropdown-menu';

  import GroupSwitch from './GroupSwitch.svelte';
  import ItemCopy from './ItemCopy.svelte';
  import ItemProfilePanel from './ItemProfilePanel.svelte';
  import { itemStatusLabel, type ItemProfileRead } from './profile';
  import {
    itemInfoFields,
    nextComparison,
    parseComparison,
    parseInfoInput,
    type ItemComparison,
    type ItemInfoInput,
  } from './item-conflict';
  // 활동(D-119). 기록 묶음 안의 항목 활동 섹션.
  import ItemActivityTab from '$lib/features/activity/ItemActivityTab.svelte';

  interface Props {
    aggregate?: boolean;
    initialContext?: TaskStartupSeed | undefined;
    workspace: Workspace;
    itemId: string;
    /** 주소에서 연 업무(상세 대화상자). */
    taskId?: string | undefined;
    workspaces: Workspace[];
  }

  const {
    workspace,
    itemId,
    workspaces,
    taskId,
    aggregate = false,
    initialContext,
  }: Props = $props();
  let consumedContext = false;
  let pendingTaskReads: { navigationGeneration: number; reads: TaskStartupReads } | undefined;
  function takeStartupReads(id: string): TaskStartupReads | undefined {
    const seed = pendingTaskReads;
    pendingTaskReads = undefined;
    return seed?.reads.itemId === id && seed.navigationGeneration === router.navigationGeneration
      ? seed.reads
      : undefined;
  }
  // 같은 항목 안 묶음·보기·상세 경로 변경도 화면에 즉시 전달한다(D-121).
  const itemRoute = $derived(
    router.route.name === 'item' && router.route.itemId === itemId ? router.route : undefined,
  );
  // 주소 객체가 바뀌어도(업무 대화상자 열고 닫기) 같은 항목이면 다시 불러오지 않는다.
  const currentItemId = $derived(itemId);

  let item = $state<ItemView | undefined>(undefined);
  let mod = $state<Module | undefined>(undefined);
  let docs = $state<SummaryView[]>([]);
  // 항목 일정 목록에서 바꾸면 달력을, 달력에서 바꾸면 목록을 다시 읽게 하는 값(둘 다 일정 묶음).
  let calendarVersion = $state(0);
  let listVersion = $state(0);
  let error = $state<string | undefined>(undefined);
  let infoOpen = $state(false);
  let profileRead = $state<ItemProfileRead | undefined>(undefined);
  let profileRefresh = $state(0);
  let profileDirty = $state(false);
  let copy = $state<{ start: () => Promise<void> } | undefined>(undefined);
  let moreButton = $state<HTMLButtonElement | null>(null);
  let deleting = $state(false);

  // 묶음(D-121, 사용자 R7 A). 주소의 묶음이 이 종류에 있으면 그것, 없으면(뿌리 경로·다른 종류의 묶음) 이 탭의 기억 ›
  // 종류별 기본 묶음으로 주소를 바꾼다(replaceState). 주소가 이 항목 경로가 아닐 때(독립 렌더)는 `chosen`을 쓴다.
  // `view`는 업무 묶음으로 돌아갈 때 쓸 마지막 업무 보기다.
  let chosen = $state<{ group: ItemGroupName; view: TaskViewName } | undefined>(undefined);
  const groups = $derived(mod === undefined ? [] : itemGroupsFor(mod.kind));
  const routeGroup = $derived(
    itemRoute?.group !== undefined && groups.includes(itemRoute.group)
      ? itemRoute.group
      : undefined,
  );
  const group = $derived(mod === undefined ? undefined : (routeGroup ?? chosen?.group));
  const GROUP_LABEL: Record<ItemGroupName, () => string> = {
    tasks: m.modules_item_group_tasks,
    docs: m.modules_item_group_docs,
    events: m.modules_item_group_events,
    records: m.modules_item_group_records,
    references: m.modules_item_group_references,
    plan: m.modules_item_group_plan,
    episodes: m.modules_item_group_episodes,
    activities: m.modules_item_group_activities,
  };
  // 첫 묶음이 기록 종류인 모듈(연구 문헌, 취미 활동). 그 종류는 첫 묶음에만, 나머지 종류는 공통 기록 묶음에 보인다.
  const FIRST_GROUP_RECORDS: Partial<Record<ModuleKind, readonly string[]>> = {
    research: ['reference'],
    hobby: ['activity'],
  };
  const firstRecords = $derived(mod === undefined ? undefined : FIRST_GROUP_RECORDS[mod.kind]);

  // 이 탭에서 항목마다 기억하는 묶음. 저장소 키에 사용자와 조직을 넣는다(D-121). 기억은 편의일 뿐이라 깨졌으면 버린다.
  const groupKey = (id: string): string => org.key(`item-group:${id}`);
  function remembered(id: string): { group?: string; view?: TaskViewName } {
    let raw: unknown = null;
    try {
      raw = JSON.parse(readSession(groupKey(id)) ?? 'null');
    } catch {
      return {};
    }
    if (!object(raw)) return {};
    const view = TASK_VIEWS.find((v) => v === raw.view);
    return {
      ...(typeof raw.group === 'string' ? { group: raw.group } : {}),
      ...(view === undefined ? {} : { view }),
    };
  }
  function remember(id: string, next: { group: ItemGroupName; view: TaskViewName }): void {
    writeSession(groupKey(id), JSON.stringify(next));
  }

  // 불러온 항목의 id. 다른 항목으로 옮기면 불러오기 전까지 앞 항목의 `mod`가 남아 있으므로, 묶음은 불러온 항목이 지금
  // 항목일 때만 정한다(앞 종류의 기본 묶음으로 새 항목 주소를 바꾸거나 헛되이 저장 보기를 읽지 않게).
  const loadedItemId = $derived(item?.id);
  // 화면이 사라졌는지. 저장 보기를 기다리는 사이 다른 화면으로 갔으면 늦은 응답이 이 항목으로 끌어오지 않는다.
  let destroyed = false;
  onDestroy(() => {
    destroyed = true;
  });
  // 묶음 단추를 누를 때마다 오른다. 업무 단추의 저장 보기를 기다리는 사이 다른 묶음을 골랐으면 늦은 응답을 버린다.
  let selectSeq = 0;

  $effect(() => {
    const kind = mod?.kind;
    const id = currentItemId;
    const route = itemRoute;
    if (kind === undefined || loadedItemId !== id) return;
    untrack(() => {
      const available = itemGroupsFor(kind);
      const memory = remembered(id);
      const lastView = chosen?.view ?? memory.view ?? 'board';
      if (route?.group !== undefined && available.includes(route.group)) {
        chosen = {
          group: route.group,
          view: route.group === 'tasks' ? (route.view ?? 'board') : lastView,
        };
        remember(id, chosen);
        return;
      }
      if (route === undefined && chosen !== undefined && available.includes(chosen.group)) return;
      const start = available.find((g) => g === memory.group) ?? ITEM_GROUP_DEFAULT[kind];
      chosen = { group: start, view: lastView };
      if (route === undefined) return;
      if (start !== 'tasks') {
        router.replace(itemGroupPath(workspace.id, id, start));
        return;
      }
      // 업무 묶음의 보기는 주소에 없으니 저장한 기본 보기 › 세션 순으로 정해 주소에 넣는다.
      const at = router.path;
      const mine = loadSeq;
      void taskEntryView(id, lastView).then((view) => {
        if (destroyed || router.path !== at || !stillOn(id, mine)) return;
        router.replace(itemGroupPath(workspace.id, id, 'tasks', view));
      });
    });
  });

  /**
   * 주소에 보기가 없이 업무 묶음에 들어갈 때(뿌리 경로·묶음 전환)의 업무 보기. 묶음 선택에는 저장한 보기가 없지만 업무 묶음
   * 안의 보기는 URL > 저장한 보기 > 세션이다(D-121 개정 2026-10-27). 기본으로 표시한 저장 보기가 하나면 그 보기(업무 보드가
   * 처음 열 때 고르는 규칙과 같다), 아니면 이 탭에서 마지막으로 본 보기, 없으면 보드다. 못 읽으면 세션 순으로 넘어간다.
   */
  async function taskEntryView(id: string, session: TaskViewName): Promise<TaskViewName> {
    const views = await savedViewsApi.savedViews(id).catch(() => []);
    const defaults = views.filter((v) => v.config.default === true);
    const saved = defaults.length === 1 ? defaults[0] : undefined;
    return saved === undefined ? session : readConfig(saved.config).view;
  }

  async function selectGroup(next: ItemGroupName): Promise<void> {
    const id = currentItemId;
    const mine = loadSeq;
    selectSeq += 1;
    const ticket = selectSeq;
    const at = router.path;
    const last = chosen?.view ?? 'board';
    const view = next === 'tasks' ? await taskEntryView(id, last) : last;
    // 기다리는 사이 다른 묶음을 고르거나(차례), 주소가 바뀌거나, 화면이 사라졌으면 늦은 응답으로 옮기지 않는다
    // (뿌리 경로 쪽과 같은 규칙).
    if (destroyed || ticket !== selectSeq || router.path !== at || !stillOn(id, mine)) return;
    const path = itemGroupPath(workspace.id, id, next, view);
    router.navigate(path);
    if (router.path !== path) return;
    chosen = { group: next, view };
    remember(id, chosen);
  }
  // 머리(정보·프로필·복제)는 묶음과 상관없이 마운트된 채다. 같은 항목 안에서 묶음·업무 보기만 바뀌면 그 입력은 남으므로
  // 묻지 않는다. 다른 항목·화면으로 가면 지금처럼 묻는다(코디네이터 지시 2026-10-27).
  const staysOnThisItem = (next: string): boolean => staysOnItem(next, currentItemId);

  let title = $state('');
  let description = $state('');
  let shared = $state(false);
  let formError = $state<string | undefined>(undefined);
  let docTemplate = $state('');
  // 새 항목 문서를 보내는 동안 템플릿 선택을 잠근다. 그 값도 초안(draftContext)이라 바꾸면 성공한 만들기를 버리고, 다시
  // 내면 새 요청 id로 두 번째 문서가 생긴다(NotesScreen·WorkspaceScreen과 같다).
  let creatingDoc = $state(false);

  // 마지막으로 서버 값으로 채운 입력란 값. 입력란이 이 값과 같으면 아직 고치지 않은 것이다. `version`은 저장할 때 보낼
  // 기준 판(D-104)이다: 입력 중에 다시 읽은 항목의 판으로 옮기면 그사이 다른 사람이 고친 것을 덮으므로, 입력란을
  // 서버 값으로 채울 때(또는 내 저장의 응답)만 옮긴다.
  let filled = $state({ title: '', description: '', shared: false, version: 0 });
  // 판 충돌(409) 뒤 칸마다 고르는 비교(D-94 개정, D-104). 닫아도 저장하기 전까지는 처음 입력을 지키려고 남긴다.
  let comparison = $state<ItemComparison<ItemInfoInput>>();
  // 저장이 권한 상실(403·404)이나 확인된 보관으로 거절되면 다시 보내지 않는다. 404면 비교한 서버 값도 숨긴다.
  let writeBlocked = $state(false);
  let hiddenForAccess = $state(false);
  const infoDirty = $derived(
    item !== undefined &&
      (comparison !== undefined ||
        title !== filled.title ||
        description !== filled.description ||
        shared !== filled.shared),
  );
  const infoDraft = createFormDraft({
    name: () => `item-info:${itemId}`,
    snapshot: () => ({ title, description, shared, filled, comparison }),
    parse: (raw) => {
      if (
        !object(raw) ||
        typeof raw.title !== 'string' ||
        typeof raw.description !== 'string' ||
        typeof raw.shared !== 'boolean' ||
        !object(raw.filled)
      )
        return undefined;
      const base = raw.filled;
      if (
        typeof base.title !== 'string' ||
        typeof base.description !== 'string' ||
        typeof base.shared !== 'boolean' ||
        typeof base.version !== 'number'
      )
        return undefined;
      return {
        title: raw.title,
        description: raw.description,
        shared: raw.shared,
        filled: {
          title: base.title,
          description: base.description,
          shared: base.shared,
          version: base.version,
        },
        // 다른 항목의 비교는 되살리지 않는다(초안 키가 같아도 대상이 다르면 버린다).
        comparison: parseComparison(raw.comparison, itemId, parseInfoInput),
      };
    },
    restore: (value) => {
      infoOpen = true;
      title = value.title;
      description = value.description;
      shared = value.shared;
      filled = value.filled;
      comparison = value.comparison;
    },
    reset: () => {
      // 비교를 먼저 비운다. 버린 비교가 초안에 다시 쓰이면 다음에 되살리자고 한다(CR-305-2).
      comparison = undefined;
      title = item?.title ?? '';
      description = item?.description ?? '';
      shared = item?.visibility === 'workspace';
      filled = { title, description, shared, version: item?.version ?? 0 };
      saving = false;
    },
    dirty: () => infoDirty,
    busy: () => saving,
    stays: staysOnThisItem,
  });
  onMount(infoDraft.attach);

  function fill(next: ItemView): void {
    item = next;
    title = next.title;
    description = next.description;
    shared = next.visibility === 'workspace';
    filled = { title, description, shared, version: next.version };
    comparison = undefined;
    writeBlocked = false;
    hiddenForAccess = false;
  }

  /**
   * 항목을 다시 읽었을 때(정보 저장·템플릿 표시 뒤): 고치지 않은 입력란만 새 값으로, 입력 중인 칸은 그대로 둔다.
   * `base`는 "고치지 않았다"를 가를 기준 값(기본은 마지막으로 채운 서버 값, 저장 응답에서는 보낸 값). `own`이면 내 저장의
   * 응답이라 그 판을 다음 저장의 기준으로 삼는다. 아니면 입력 중인 칸이 없을 때만 옮긴다(입력 중이면 옛 판을 두어, 그사이
   * 바뀐 것을 저장이 409로 알린다).
   */
  function refresh(
    next: ItemView,
    base: { title: string; description: string; shared: boolean } = filled,
    own = false,
  ): void {
    item = next;
    // 비교 중에는 검토하는 값과 기준을 다시 읽은 값으로 바꾸지 않는다(409만 바꾼다).
    if (comparison?.open === true) return;
    const nextShared = next.visibility === 'workspace';
    const editing =
      title !== base.title || description !== base.description || shared !== base.shared;
    if (title === base.title) title = next.title;
    if (description === base.description) description = next.description;
    if (shared === base.shared) shared = nextShared;
    filled = {
      title: next.title,
      description: next.description,
      shared: nextShared,
      version: own || !editing ? next.version : filled.version,
    };
  }

  // 항목을 (다시) 불러올 때마다 늘리는 차례. A → B → A로 돌아오면 항목 id는 같아도 새로 읽은 화면이다.
  let loadSeq = 0;
  // 지금 화면의 항목을 읽은 사용자. 항목 정보 저장의 기대 사용자 머리로 보낸다(D-96, #307): 그사이 이 창의 사용자가
  // 바뀌었어도 앞 사용자가 읽은 화면의 쓰기는 새 세션에서 409 account_changed로 막힌다.
  let itemReader = '';
  // 버린 첫 읽기 뒤의 "다시 시도". 누르면 같은 항목을 새 문맥으로 다시 읽는다.
  let loadAttempt = $state(0);
  let retryLoad = $state(false);
  const recovery = getContext<TaskSessionRecovery | undefined>(TASK_SESSION_RECOVERY);
  /**
   * 첫 읽기의 문맥. 이 화면은 항목 단위라 같은 항목 안의 묶음·보기 이동(주소 변경)은 같은 요청으로 본다. 다른 항목·다시
   * 불러오기(차례), 사용자, 세션·계정 안전 세대가 바뀌면 다른 요청이다.
   */
  const itemReadContext = (): ContextCapture => ({
    userId: me.userId,
    sequence: loadSeq,
    navigationGeneration: router.itemGeneration,
    sessionGeneration: readTaskSessionGeneration(),
    safetyGeneration: accountSafety.generation,
    path: currentItemId,
  });

  $effect(() => {
    const id = currentItemId;
    const generation = router.itemGeneration;
    if (loadAttempt < 0) return;
    loadSeq += 1;
    retryLoad = false;
    let cancelled = false;
    const isCurrent = (): boolean => !cancelled && generation === router.itemGeneration;
    item = undefined;
    mod = undefined;
    docs = [];
    pendingTaskReads = undefined;
    chosen = undefined;
    error = undefined;
    infoOpen = false;
    profileRead = undefined;
    profileRefresh = 0;
    profileDirty = false;
    deleting = false;
    // 앞 항목의 오류, 새 문서에 고른 템플릿을 새 항목에 남기지 않는다(다른 워크스페이스 항목으로
    // 옮겨 가면 그 템플릿은 목록에 없어 보이지 않는데도 쓰였다).
    formError = undefined;
    comparison = undefined;
    writeBlocked = false;
    hiddenForAccess = false;
    docTemplate = '';
    const seed = untrack(() =>
      !consumedContext &&
      initialContext?.itemId === id &&
      initialContext.navigationGeneration === router.navigationGeneration
        ? initialContext
        : undefined,
    );
    consumedContext = true;
    const reader = untrack(() => me.userId);
    const adopt = ({ it, found }: { it: ItemView; found: Module }, list: SummaryView[]) => {
      if (!isCurrent()) return;
      itemReader = reader;
      fill(it);
      // 새로 만든 빈 항목의 첫 진입만 편집을 연다. 판·내용으로 기존 빈 항목을 추측하지 않는다.
      const entryKey = org.key('new-item-info');
      infoOpen = readSession(entryKey) === id && !it.archived && it.access !== 'read';
      if (readSession(entryKey) === id) writeSession(entryKey, null);
      mod = found;
      workspaceNav.currentItem = { itemId: it.id, moduleId: it.module_id };
      docs = list;
    };
    const fail = (e: unknown) => {
      if (!isCurrent()) return;
      error =
        e instanceof ApiError && e.status === 404
          ? m.modules_item_not_found()
          : m.modules_item_load_failed();
    };
    if (untrack(() => aggregate) && seed?.result.kind === 'loaded') {
      const value = seed.result.value;
      if (seed.reads !== undefined)
        pendingTaskReads = { navigationGeneration: seed.navigationGeneration, reads: seed.reads };
      // The App already joined session lists and context. Adopt within this flush.
      try {
        untrack(() => {
          adopt({ it: value.item, found: value.module }, value.documents);
        });
      } catch (cause: unknown) {
        untrack(() => {
          fail(cause);
        });
      }
    } else if (untrack(() => aggregate)) {
      // 업무 묶음의 첫 읽기(task-context)는 세션 세대 울타리를 지난다. 다른 탭의 세션 알림으로 버려지면 App이 /me로
      // 같은 사용자임을 확인한 뒤 한 번 다시 읽는다. 다른 사용자면 다시 읽지 않는다(#305 E, #298 SR-298-4). 확인이
      // 실패해도 이 요청이 아직 현재면 오류와 다시 시도를 둔다(가림이 걷히면 보인다, #305 F·CR-4).
      const captured = untrack(itemReadContext);
      const current = (): boolean => isCurrent() && sameTaskRequest(captured, itemReadContext());
      void readTaskWithRecovery(
        captured,
        itemReadContext,
        () => current() && !accountSafety.switching && !accountSafety.restoring,
        recovery,
        () => taskScreenApi.context(id),
        seed?.result.kind === 'failed' ? Promise.reject(seed.result.error) : undefined,
        {
          loaded: (read) => {
            if (current()) adopt({ it: read.item, found: read.module }, read.documents);
          },
        },
      ).catch((cause: unknown) => {
        if (cause instanceof DiscardedTaskRead || cause instanceof TaskSessionUnsettled) {
          if (!current()) return;
          error = m.modules_item_load_failed();
          retryLoad = true;
          return;
        }
        fail(cause);
      });
    } else {
      void Promise.all([
        api.item(id).then(async (it) => ({ it, found: await api.module(it.module_id) })),
        api.itemDocuments(id),
      ])
        .then(([resource, list]) => {
          adopt(resource, list);
        })
        .catch(fail);
    }
    return () => {
      cancelled = true;
    };
  });

  // 접근 등급은 서버 app_access 결과를 그대로 쓴다(D-113). 보관은 등급이 아니라 아래 canEdit에서 본다.
  const moduleArchived = $derived(mod?.archived === true);
  const shownItem = $derived(item);
  // 보관된 항목과 보관된 모듈의 항목은 읽기 전용이다(서버도 쓰기를 거부한다: app_item_archived 55000). 등급(app_grade_item)은 보관을
  // 반영하지 않으므로 소유자·쓰기 권한자에게도 편집 폼과 "보관" 단추 대신 아래의 "보관 해제" 단추만 보인다.
  const canEdit = $derived(
    item !== undefined && item.access !== 'read' && !item.archived && !moduleArchived,
  );
  const isOwner = $derived(item?.access === 'owner');
  const infoFields = $derived(itemInfoFields(isOwner));

  // 이 화면은 다른 항목으로 옮겨 가도 다시 만들어지지 않는다. 기다리는 사이 옮겨 갔으면 시작한 항목의 결과·오류를
  // 지금 항목 화면에 넣지 않는다(B 주소에 A가 들어앉지 않게). 같은 항목으로 돌아왔어도 시작할 때의 차례(`mine`)가
  // 아니면 버린다(앞 A 화면의 늦은 저장이 새 A 화면에 "저장했습니다."를 띄우거나 새로 읽은 값을 옛 값으로 되돌렸다).
  const stillOn = (id: string, mine: number): boolean => currentItemId === id && loadSeq === mine;

  /**
   * 항목 정보를 저장하는 중. 공개 범위는 체크를 바꿨을 때만 보내므로, 공개로 바꾼 저장이 끝나기 전에 체크를 풀고 다시
   * 저장하면 그 저장은 아직 비공개인 `filled`와 같아 공개 범위를 보내지 않았다. 먼저 보낸 공개가 서버에 남는데 화면은
   * 비공개로 보였다. 끝날 때까지 다시 보내지 않고, 다음 저장은 응답으로 채운 값과 비교한다.
   */
  let saving = $state(false);

  async function save(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    await submit(filled.version);
  }

  /**
   * 입력란을 `baseVersion` 판에 대한 고치기로 보낸다. 보통은 입력란을 채운 판이고, 충돌 알림의 "내 입력으로 다시
   * 저장"은 알림이 보인 최신본의 판이다(그 뒤 또 바뀌었으면 다시 409).
   */
  async function submit(baseVersion: number): Promise<void> {
    if (item === undefined || saving || writeBlocked) return;
    const attempt = infoDraft.capture();
    if (attempt === undefined) return;
    if (title.trim() === '') {
      formError = m.modules_title_required();
      return;
    }
    formError = undefined;
    const id = item.id;
    const mine = loadSeq;
    const captured = writeContext();
    const owns = (): boolean =>
      stillOn(id, mine) && infoDraft.isOwner(attempt) && sameWriteContext(captured);
    // 공개 범위는 체크를 바꿨을 때만 보낸다. 늘 보내면 연 뒤 다른 소유 권한자가 바꾼 공개 범위를 되돌렸다(감사 W13).
    const visibility: 'workspace' | 'private' | undefined =
      shared === filled.shared ? undefined : shared ? 'workspace' : 'private';
    // 보낸 입력. 응답을 기다리는 사이 더 고친 칸은 응답 값으로 덮지 않는다(보내지 않은 입력이 사라지지 않게).
    const sent = { title, description, shared };
    saving = true;
    try {
      const result = await api.updateItem(
        id,
        baseVersion,
        {
          title: title.trim(),
          description,
          ...(visibility === undefined ? {} : { visibility }),
        },
        itemReader,
      );
      if (result.kind === 'conflict') {
        // 그사이 다른 사람이 고쳤다. 서버는 아무것도 바꾸지 않았다. 입력은 그대로 두고 바로 칸마다 고르게 한다
        // (D-94 개정, D-104). 다른 사용자·세션·항목 화면의 응답이면 비교값을 보이지 않는다(D-96).
        if (owns()) openInfoComparison(result.current);
        return;
      }
      forgetReferences();
      if (!owns()) return;
      if (infoDraft.isCurrent(attempt)) {
        item = result.item;
        infoDraft.complete(attempt);
      }
      comparison = undefined;
      refresh(result.item, sent, true);
      // 일반 완료는 토스트다(D-82). 더 고친 칸이 남아 있으면 알리지 않는다(그 입력은 아직 저장되지 않았다).
      if (title === filled.title && description === filled.description && shared === filled.shared)
        notifyDone(m.common_saved());
    } catch (e: unknown) {
      if (owns()) {
        formError = errorText(e, m.modules_save_failed());
        await infoWriteFailed(e, id, owns);
      }
    } finally {
      if (infoDraft.isOwner(attempt)) saving = false;
    }
  }

  // 쓰기 응답을 받을 때의 문맥(D-96): 사용자·세션 세대·계정 안전 세대가 그대로여야 결과를 화면에 넣는다.
  const writeContext = () => ({
    userId: me.userId,
    session: readTaskSessionGeneration(),
    safety: accountSafety.generation,
  });
  function sameWriteContext(captured: ReturnType<typeof writeContext>): boolean {
    const now = writeContext();
    return (
      captured.userId === now.userId &&
      captured.session === now.session &&
      captured.safety === now.safety &&
      !accountSafety.switching &&
      !accountSafety.restoring
    );
  }
  function blockDenied(cause: unknown): void {
    if (!(cause instanceof ApiError)) return;
    if (cause.status === 403 || cause.status === 404) writeBlocked = true;
    if (cause.status === 404) hiddenForAccess = true;
  }
  /**
   * 거절된 저장. 403·404는 저장을 막는다. 409 `invalid_state`는 보관과 일시 충돌(직렬화·교착)이 같은 코드라, 쓸 수 있는지
   * 한 번 다시 읽어 보관·권한 상실이 확인될 때만 막는다(SR-302-1). 그 사이 문맥이 바뀌었으면 버린다(CR-A).
   */
  async function infoWriteFailed(cause: unknown, id: string, owns: () => boolean): Promise<void> {
    blockDenied(cause);
    if (!(cause instanceof ApiError) || cause.status !== 409 || cause.code !== 'invalid_state')
      return;
    try {
      const fresh = await api.itemProfile(id);
      if (owns()) writeBlocked = !fresh.can_edit;
    } catch (failure: unknown) {
      if (owns()) blockDenied(failure);
    }
  }

  const infoInput = (): ItemInfoInput => ({ title, description, shared });
  const serverInfo = (next: ItemView): ItemInfoInput => ({
    title: next.title,
    description: next.description,
    shared: next.visibility === 'workspace',
  });
  /** 입력 기준(`filled`)·이번 입력·409의 서버 값을 비교한다. 409의 `changed`는 양쪽 변경 판정이 아니라 쓰지 않는다. */
  function openInfoComparison(current: ItemView): void {
    comparison = nextComparison($state.snapshot(comparison), infoFields, {
      target: current.id,
      base: { title: filled.title, description: filled.description, shared: filled.shared },
      mine: $state.snapshot(infoInput()),
      server: serverInfo(current),
      version: current.version,
      baseVersion: filled.version,
    });
    if (current.archived || current.access === 'read') writeBlocked = true;
  }
  /** 고른 값을 입력에 넣고 검토한 판을 기준으로 삼는다. `saveNow`면 그 판으로 저장한다(자동 재전송 없음). */
  async function applyInfoChoices(saveNow: boolean): Promise<void> {
    const ask = comparison;
    const current = item;
    if (ask === undefined || !ask.open || current === undefined || saving || writeBlocked) return;
    // 초안에서 되살린 비교가 지금 읽은 판보다 옛 판이면 옛 값으로 바꾸거나 기준 판을 낮추지 않고 새로 읽은 판과 다시
    // 비교한다(CR-305-1). 요청은 보내지 않는다.
    if (current.version > ask.version) {
      openInfoComparison($state.snapshot(current));
      return;
    }
    const candidate = selectedFields(
      infoFields,
      $state.snapshot(ask.mine),
      $state.snapshot(ask.server),
      ask.choices,
    );
    title = candidate.title;
    description = candidate.description;
    shared = candidate.shared;
    filled = { ...ask.server, version: ask.version };
    if (!saveNow) {
      ask.open = false;
      await tick();
      document.getElementById('item-title')?.focus();
      return;
    }
    if (infoFields.every((field) => field.same(candidate, ask.server))) {
      // 고른 값이 모두 서버 최신값이다. 보낼 것이 없다.
      item = {
        ...current,
        title: ask.server.title,
        description: ask.server.description,
        visibility: ask.server.shared ? 'workspace' : 'private',
        version: ask.version,
      };
      comparison = undefined;
      notifyDone(m.common_conflict_equal());
      return;
    }
    await submit(ask.version);
  }
  /** 비교를 닫고 이번 입력(또는 처음 입력)과 그때의 기준으로 돌아간다. 비교는 처음 입력을 지키려 남긴다. */
  function leaveInfoComparison(original: boolean): void {
    const ask = comparison;
    if (ask === undefined || !ask.open || saving) return;
    const input = original ? ask.original : ask.mine;
    const base = original ? ask.originalBase : ask.base;
    title = input.title;
    description = input.description;
    shared = input.shared;
    filled = { ...base, version: original ? ask.originalVersion : ask.baseVersion };
    ask.open = false;
    void tick().then(() => document.getElementById('item-title')?.focus());
  }
  const infoEdited = (a: ItemInfoInput, b: ItemInfoInput): boolean =>
    !infoFields.every((field) => field.same(a, b));

  async function toggleTemplate(): Promise<void> {
    if (item === undefined) return;
    const id = item.id;
    const mine = loadSeq;
    try {
      await api.setItemTemplate(id, !item.is_template);
      const next = await api.item(id);
      if (stillOn(id, mine)) refresh(next);
    } catch (e: unknown) {
      if (stillOn(id, mine)) formError = errorText(e, m.modules_change_failed());
    }
  }

  async function toggleArchive(): Promise<void> {
    if (item === undefined) return;
    const id = item.id;
    const mine = loadSeq;
    try {
      const next = await api.archiveItem(id, !item.archived);
      if (stillOn(id, mine)) fill(next);
    } catch (e: unknown) {
      if (stillOn(id, mine)) formError = errorText(e, m.modules_change_failed());
    }
  }

  /** 항목을 다시 읽어 입력 중인 칸은 두고 넣는다. 그사이 다른 항목으로 옮겨 갔으면(돌아왔어도) 버린다. */
  function reread(id: string, apply: (next: ItemView) => void): void {
    const mine = loadSeq;
    api
      .item(id)
      .then((next) => {
        if (stillOn(id, mine)) apply(next);
      })
      .catch(() => undefined);
  }

  // 경로: 워크스페이스는 늘(항목을 못 읽어도 돌아갈 곳), 모듈은 읽은 뒤에.
  const crumbs = $derived([
    { path: workspacePath(workspace.id), label: workspace.name },
    ...(mod === undefined
      ? []
      : [{ path: modulePath(workspace.id, mod.id), label: `${mod.name} · ${mod.key}` }]),
  ]);
</script>

{#each item === undefined ? [] : infoDraft.offers as entry (entry.key)}
  <FormDraftOffer
    restorable={canEdit && !accountSwitching()}
    onrestore={() => {
      if (canEdit && !accountSwitching()) infoDraft.restore(entry);
    }}
    ondiscard={() => {
      infoDraft.discard(entry);
    }}
  />
{/each}
<DiscardChangesConfirm
  open={infoDraft.leave.open}
  onkeep={infoDraft.leave.keep}
  ondiscard={infoDraft.discardAndLeave}
/>

<nav aria-label={m.modules_breadcrumb()} class="-mb-6 min-w-0 text-sm text-muted-foreground">
  <ol class="flex min-w-0 items-center gap-1">
    {#each crumbs as crumb, index (crumb.path)}
      <li class="flex max-w-full min-w-0 shrink items-center gap-1">
        {#if index > 0}<ChevronRight class="size-3.5 shrink-0" aria-hidden="true" />{/if}
        <a
          class="truncate rounded-sm underline-offset-4 hover:text-foreground hover:underline focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
          href={crumb.path}
          title={crumb.label}
          onclick={(e: MouseEvent) => {
            if (e.metaKey || e.ctrlKey || e.shiftKey || e.button !== 0) return;
            e.preventDefault();
            router.navigate(crumb.path);
          }}>{crumb.label}</a
        >
      </li>
    {/each}
  </ol>
</nav>

{#if error !== undefined}
  <p role="alert" class="text-destructive">{error}</p>
  {#if retryLoad}
    <div>
      <Button variant="outline" onclick={() => (loadAttempt += 1)}>{m.common_retry()}</Button>
    </div>
  {/if}
{:else if item === undefined}
  <p aria-live="polite">{m.modules_loading()}</p>
{:else}
  {@const current = item}
  {@const view = shownItem ?? current}
  <section class="flex flex-col gap-6" aria-labelledby="item-heading">
    <div class="flex min-w-0 flex-wrap items-baseline gap-2" data-item-header>
      <span class="font-mono text-sm text-muted-foreground">{current.reference}</span>
      <h1 id="item-heading" class="min-w-0 text-xl font-semibold [overflow-wrap:anywhere]">
        {current.title}
      </h1>
      <Badge variant="outline"
        >{current.visibility === 'private'
          ? m.modules_private()
          : m.modules_shared_workspace()}</Badge
      >
      <Badge variant="outline">{itemStatusLabel(current.status)}</Badge>
      {#if current.archived || moduleArchived}<Badge variant="secondary"
          >{m.modules_archived_readonly()}</Badge
        >{/if}
      {#if current.is_template}<Badge variant="secondary">{m.modules_template()}</Badge>{/if}
      <div class="ml-auto flex flex-wrap items-center gap-1">
        <MarkButtons kind="item" id={current.id} />
        {#if isOwner || workspace.access !== 'read'}
          <DropdownMenu.Root>
            <DropdownMenu.Trigger>
              {#snippet child({ props })}
                <Button
                  {...props}
                  variant="ghost"
                  size="icon"
                  bind:ref={moreButton}
                  aria-label={m.modules_more_actions()}
                >
                  <Ellipsis class="size-4" aria-hidden="true" />
                </Button>
              {/snippet}
            </DropdownMenu.Trigger>
            <DropdownMenu.Content align="end">
              {#if isOwner && (canEdit || current.archived)}
                <DropdownMenu.Item onSelect={() => void toggleArchive()}>
                  {current.archived ? m.modules_unarchive() : m.modules_archive()}
                </DropdownMenu.Item>
              {/if}
              {#if workspace.access !== 'read' && mod !== undefined}
                <DropdownMenu.Item onSelect={() => void copy?.start()}
                  >{m.modules_copy()}</DropdownMenu.Item
                >
                {#if canEdit}
                  <DropdownMenu.Item onSelect={() => void toggleTemplate()}>
                    {current.is_template ? m.modules_template_unset() : m.modules_template_set()}
                  </DropdownMenu.Item>
                {/if}
              {/if}
              {#if isOwner}
                <DropdownMenu.Separator />
                <DropdownMenu.Item variant="destructive" onSelect={() => (deleting = true)}>
                  {m.trash_delete_what({ what: m.modules_delete_item_what() })}
                </DropdownMenu.Item>
              {/if}
            </DropdownMenu.Content>
          </DropdownMenu.Root>
        {/if}
      </div>
    </div>

    {#if isOwner}
      <DeleteButton
        bind:confirming={deleting}
        trigger={false}
        oncancel={() => moreButton?.focus()}
        what={m.modules_delete_item_what()}
        remove={() => api.trashItem(current.id)}
        ondeleted={() => {
          router.navigate(modulePath(workspace.id, current.module_id));
        }}
      />
    {/if}
    {#if mod !== undefined && workspace.access !== 'read'}
      <ItemCopy
        bind:this={copy}
        item={current}
        module={mod}
        stays={staysOnThisItem}
        trigger={false}
        returnFocus={moreButton}
      />
    {/if}
    <Collapsible.Root bind:open={infoOpen} class="min-w-0">
      <div class="flex min-w-0 flex-wrap items-start gap-2">
        <Collapsible.Trigger
          disabled={infoDirty || profileDirty || saving}
          class="flex min-w-0 flex-1 items-center gap-2 rounded-md px-2 py-2 text-left hover:bg-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          aria-describedby={infoDirty || profileDirty ? 'item-info-unsaved' : undefined}
        >
          <ChevronDown class={['size-4 shrink-0', infoOpen && 'rotate-180']} aria-hidden="true" />
          <span
            class="flex min-w-0 flex-wrap items-baseline gap-x-3 gap-y-1 [overflow-wrap:anywhere]"
          >
            <span class="font-medium">{m.modules_item_info()}</span>
            <span id="item-info-profile-status" role="status" class="text-sm text-muted-foreground">
              {itemStatusLabel(current.status)} ·
              {#if profileRead?.status === 'ready'}
                {profileRead.profile.leader === null
                  ? m.modules_summary_no_leader()
                  : m.modules_summary_leader({ name: profileRead.profile.leader.display_name })} · {m.modules_member_count(
                  { count: profileRead.profile.members.length },
                )}
              {:else if profileRead?.status === 'failed'}
                <span class="text-destructive">{profileRead.error}</span>
              {:else}
                {m.modules_loading()}
              {/if}
              · {current.visibility === 'private'
                ? m.modules_private()
                : m.modules_shared_workspace()}
            </span>
          </span>
        </Collapsible.Trigger>
        {#if profileRead?.status === 'failed'}
          <Button
            size="sm"
            variant="outline"
            aria-describedby="item-info-profile-status"
            onclick={() => (profileRefresh += 1)}>{m.common_retry()}</Button
          >
        {/if}
      </div>
      <p id="item-info-unsaved" role="status" class="text-sm text-muted-foreground empty:hidden">
        {#if infoDirty || profileDirty}{m.modules_info_unsaved()}{/if}
      </p>
      <!-- 접어도 초안·프로필 조회·이탈 가드를 유지한다. 숨긴 입력은 초점 순서에서 빠진다. -->
      <Collapsible.Content forceMount>
        <div hidden={!infoOpen}>
          <div class="flex min-w-0 flex-col gap-6 pt-3">
            {#if canEdit}
              {#if comparison?.open === true && !hiddenForAccess}
                {@const ask = comparison}
                <!-- 판 충돌(D-94 개정, D-104): 칸마다 내 입력·서버 최신을 고른다. 고르는 동안 폼은 잠근다. -->
                <FieldConflict
                  fields={infoFields}
                  base={ask.base}
                  mine={ask.mine}
                  server={ask.server}
                  bind:choices={ask.choices}
                  busy={saving}
                  blocked={writeBlocked}
                  repeated={ask.repeated}
                  onsave={() => void applyInfoChoices(true)}
                  onedit={() => void applyInfoChoices(false)}
                  oncancel={() => {
                    leaveInfoComparison(false);
                  }}
                  onoriginal={infoEdited(ask.original, ask.mine)
                    ? () => {
                        leaveInfoComparison(true);
                      }
                    : undefined}
                />
              {/if}
              <!-- 404로 비교를 숨기면 폼을 잠그지 않는다. 입력에 닿고 저장은 writeBlocked가 막는다(CR-306-3). -->
              <form
                class="flex flex-col gap-3"
                onsubmit={(e) => void save(e)}
                novalidate
                aria-label={m.modules_item_info()}
                inert={comparison?.open === true && !hiddenForAccess}
              >
                <div class="flex flex-col gap-1">
                  <Label for="item-title">{m.modules_title()}</Label>
                  <Input id="item-title" bind:value={title} maxlength={200} />
                </div>
                <div class="flex flex-col gap-1">
                  <Label for="item-description">{m.modules_description()}</Label>
                  <Textarea
                    id="item-description"
                    bind:value={description}
                    maxlength={2000}
                    rows={3}
                    class="min-h-20"
                  />
                </div>
                {#if isOwner}
                  <div class="flex items-center gap-2">
                    <Checkbox id="item-shared" bind:checked={shared} />
                    <Label for="item-shared">{m.modules_share_with_members()}</Label>
                  </div>
                {/if}
                <div class="flex gap-2">
                  <Button type="submit" disabled={saving}>{m.common_save()}</Button>
                </div>
              </form>
            {:else}
              {#if current.description !== ''}
                <p class="whitespace-pre-wrap">{current.description}</p>
              {/if}
            {/if}
            {#if mod !== undefined}
              <!-- 머리: 묶음과 상관없이 늘 보이는 항목 프로필과 복제·템플릿(R7 A가 묶음을 정하지 않은 영역). -->
              <ItemProfilePanel
                item={view}
                onprofile={(next: ItemProfileRead) => {
                  if (next.itemId === currentItemId) profileRead = next;
                }}
                refresh={profileRefresh}
                ondirty={(next: boolean) => (profileDirty = next)}
                onrestore={() => (infoOpen = true)}
                kind={mod.kind}
                stays={staysOnThisItem}
                onchanged={() => {
                  reread(current.id, refresh);
                }}
              />
            {/if}
          </div>
        </div>
      </Collapsible.Content>
    </Collapsible.Root>
    {#if formError !== undefined}
      <p class="text-sm text-destructive" role="alert">{formError}</p>
    {/if}

    {#if mod !== undefined}
      <!-- 묶음(D-121): 고른 묶음만 마운트한다. 다른 묶음으로 가면 그 묶음의 입력은 이탈 확인(D-125)을 거친다. -->
      <GroupSwitch
        label={m.modules_item_groups_label()}
        options={groups.map((g) => ({
          id: g,
          label: GROUP_LABEL[g](),
          href: itemGroupPath(workspace.id, current.id, g, chosen?.view),
        }))}
        current={group}
        onselect={(g: ItemGroupName) => void selectGroup(g)}
      />
      <div class="flex flex-col gap-6" data-item-group={group}>
        {#if group === 'references' || group === 'activities'}
          <RecordsPanel item={current} kind={mod.kind} canWrite={canEdit} only={firstRecords} />
        {:else if group === 'plan'}
          <StudyPanel itemId={current.id} />
        {:else if group === 'episodes'}
          <CoursePanel itemId={current.id} />
        {:else if group === 'tasks'}
          <TaskBoard
            {aggregate}
            initialReads={takeStartupReads}
            item={view}
            {workspaces}
            routeView={itemRoute?.view}
            openTaskId={itemRoute === undefined ? taskId : itemRoute.taskId}
            locked={moduleArchived}
            onstale={() => {
              // 권한·보관이 바뀌었을 수 있다. 입력 중인 칸은 두고, 항목 상태와 고치지 않은 칸은 새로 읽은 값으로 바꾼다
              // (옛 값이 남으면 다음 저장이 그사이 바뀐 제목·설명을 되돌린다).
              reread(current.id, refresh);
            }}
          />
        {:else if group === 'docs'}
          <section class="flex flex-col gap-3" aria-labelledby="item-docs-heading">
            <h2 id="item-docs-heading" class="text-lg font-semibold">{m.modules_documents()}</h2>
            <DocumentTree
              {docs}
              label={m.modules_documents_tree({ title: current.title })}
              empty={m.modules_documents_empty()}
              hrefFor={(id: string) => itemDocumentPath(workspace.id, current.id, id)}
              navigate={(path: string) => {
                router.navigate(path);
              }}
              canWrite={canEdit}
              onmove={(id: string, move: TreeMove) =>
                // 실패해도(권한 철회·동시 변경) 최신 목록을 다시 읽는다.
                moveThenReload(
                  () => api.setParent(id, move.parentId, move.beforeId),
                  async () => {
                    // 다시 읽는 사이 다른 항목으로 옮겼으면(같은 항목으로 돌아왔어도) 이 목록을 그 화면에 넣지 않는다.
                    const requested = current.id;
                    const mine = loadSeq;
                    const list = await api.itemDocuments(requested);
                    if (stillOn(requested, mine)) docs = list;
                  },
                )}
              oncreatechild={async (parentId: string) => {
                const doc = await api.createItemDocument(
                  current.id,
                  newRequestId(),
                  m.modules_new_document_default(),
                  parentId,
                );
                router.navigate(itemDocumentPath(workspace.id, current.id, doc.id));
              }}
            />
            {#if canEdit}
              <TemplatePicker
                workspaceId={workspace.id}
                id="new-item-document-template"
                bind:value={docTemplate}
                disabled={creatingDoc}
              />
              <CreateForm
                id="new-item-document"
                draftName={`item-document-new:${current.id}`}
                bind:draftContext={docTemplate}
                bind:busy={creatingDoc}
                label={m.modules_new_document_title()}
                placeholder={m.modules_new_document_placeholder()}
                submitLabel={m.modules_create_document()}
                create={async (requestId: string, docTitle: string, context: CreateFormContext) => {
                  const doc = await api.createItemDocument(
                    current.id,
                    requestId,
                    docTitle,
                    null,
                    docTemplate === '' ? null : docTemplate,
                  );
                  if (context.saved())
                    router.navigate(itemDocumentPath(workspace.id, current.id, doc.id));
                }}
              />
            {/if}
          </section>
          <Backlinks kind="item" id={current.id} {workspaces} />
        {:else if group === 'events'}
          <!-- 일정 목록과 달력은 같은 일정을 따로 읽는다. 한쪽에서 바꾸면 다른 쪽이 다시 읽게 알린다. -->
          <ItemEvents
            item={view}
            locked={moduleArchived}
            version={listVersion}
            onchanged={() => {
              calendarVersion += 1;
            }}
          />
          <CalendarScreen
            {workspace}
            scope={{ itemId: current.id }}
            item={view}
            locked={moduleArchived || view.archived}
            embedded
            version={calendarVersion}
            onchanged={() => {
              listVersion += 1;
            }}
          />
        {:else if group === 'records'}
          <RecordsPanel item={current} kind={mod.kind} canWrite={canEdit} except={firstRecords} />
          <!-- 항목은 내 합계만 보며 시작 설정은 없다(D-117). -->
          <ItemTimePanel itemId={current.id} />
          <AttachmentsPanel target={{ kind: 'item', id: current.id }} canWrite={canEdit} />
          <!-- 활동(D-119) -->
          <ItemActivityTab itemId={current.id} workspaceId={workspace.id} />
        {/if}
      </div>
    {/if}
  </section>
{/if}
