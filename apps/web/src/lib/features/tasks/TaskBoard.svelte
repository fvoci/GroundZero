<script lang="ts">
  // 항목의 업무 (D-31): 칸반·목록·간트·달력, 검색·필터·정렬과 저장한 보기, 업무 만들기, 상세 대화상자.
  // 칸반 카드는 버튼과 키보드(Alt+화살표)로 열·순서를 옮긴다. 상세는 주소(/t/{업무})로도 열린다.
  import type { Workspace } from '@fvoci/api-client';
  import { untrack } from 'svelte';

  import type { ItemView, SavedViewView, TaskStatus, TaskView } from '$lib/api/client';
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

  import type { BoardAction, BoardMove, CardBox, DropSlot } from './board';
  import {
    applyMove,
    dropBefore,
    lanes,
    listOrder,
    moveErrorMessage,
    planBoardMove,
    planDrop,
  } from './board';
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
  import { PRIORITY_LABEL, STATUS_LABEL } from './labels';
  import GanttChart from './GanttChart.svelte';
  import TaskCalendar from './TaskCalendar.svelte';
  import TaskDetail from './TaskDetail.svelte';

  interface Props {
    item: ItemView;
    workspaces: Workspace[];
    /** 주소에서 연 업무. */
    openTaskId: string | undefined;
    /** 옮기기가 권한·보관 때문에 거절되면 항목을 다시 읽게 한다(쓰기 단추를 끄기 위해). */
    onstale?: () => void;
  }

  const { item, workspaces, openTaskId, onstale }: Props = $props();

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
    try {
      const config = { view, filter: { ...filter }, sort };
      const requestId = viewKey.for({ item: item.id, name, config });
      const saved = await api.saveView(item.id, requestId, name, config);
      viewKey.done(requestId);
      await loadViews(item.id);
      selectedView = saved.id;
      status = m.tasks_view_saved({ name });
      error = undefined;
    } catch (e: unknown) {
      error = errorText(e, m.tasks_view_save_failed());
    }
  }

  async function deleteView(): Promise<void> {
    if (selectedView === '') return;
    try {
      await api.deleteView(selectedView);
      selectedView = '';
      await loadViews(item.id);
      status = m.tasks_view_deleted();
    } catch (e: unknown) {
      error = errorText(e, m.tasks_view_delete_failed());
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

  async function reload(itemId: string = item.id): Promise<void> {
    try {
      tasks = await api.tasks(itemId);
      error = undefined;
    } catch (e: unknown) {
      error = errorText(e, m.tasks_load_failed());
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
  });

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
  async function quickAdd(lane: TaskStatus, requestId: string, title: string): Promise<void> {
    const created = await api.createTask(item.id, requestId, title);
    if (created.status === lane) {
      await reload();
      return;
    }
    await commitMove(created.id, { status: lane, beforeId: null }, 'menu');
  }

  async function commitMove(
    id: string,
    plan: BoardMove,
    how: BoardAction | 'drag' | 'menu',
  ): Promise<boolean> {
    const before = tasks;
    const title = titleOf(id);
    tasks = applyMove(tasks, id, plan);
    try {
      await api.moveTask(id, plan.status, plan.beforeId);
      await reload();
      moveNotice = undefined;
      status =
        how === 'drag'
          ? m.tasks_moved_drag({ title, status: STATUS_LABEL[plan.status] })
          : how === 'menu'
            ? m.tasks_moved_menu({ title, status: STATUS_LABEL[plan.status] })
            : movedText(how, title, plan.status);
      return true;
    } catch (e: unknown) {
      tasks = before;
      const message = moveErrorMessage(e);
      await reload();
      moveNotice = message;
      status = message;
      if (e instanceof ApiError && (e.status === 403 || e.status === 409)) onstale?.();
      return false;
    }
  }

  async function move(id: string, action: BoardAction): Promise<void> {
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
    return () => {
      window.removeEventListener('pointermove', onDragMove);
      window.removeEventListener('pointerup', onDragEnd);
      window.removeEventListener('pointercancel', cancelDrag);
      window.removeEventListener('keydown', onKey);
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
      <Button size="sm" variant="ghost" onclick={() => void deleteView()}
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
  {#if error !== undefined}
    <p role="alert" class="text-sm text-destructive">{error}</p>
  {/if}

  {#if view === 'board'}
    <div class="grid gap-3 md:grid-cols-3" data-testid="task-board">
      {#each TASK_STATUSES as lane (lane)}
        <section
          data-lane={lane}
          class={[
            'flex min-w-0 flex-col gap-2 rounded-md border p-2',
            drag?.active === true && drag.slot?.status === lane && 'ring-2 ring-ring',
          ]}
          aria-label={m.tasks_lane_aria({ status: STATUS_LABEL[lane] })}
        >
          <h3 class="text-sm font-semibold">
            {STATUS_LABEL[lane]}
            <span class="font-normal text-muted-foreground">{board[lane].length}</span>
          </h3>
          <ul class="flex min-h-12 flex-col gap-2">
            {#each board[lane] as t (t.id)}
              {#if dropMark(lane, t.id)}
                <li aria-hidden="true" class="h-1 rounded bg-primary"></li>
              {/if}
              <li
                data-board-card={t.id}
                class={[
                  'flex flex-col gap-1 rounded-md border bg-card p-2',
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
                    class="flex items-baseline gap-2 rounded text-left text-sm hover:underline focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
                    onclick={() => {
                      if (!justDragged) open(t.id);
                    }}
                  >
                    <span class="shrink-0 font-mono text-xs text-muted-foreground"
                      >{t.reference ?? ''}</span
                    >
                    <span class="min-w-0 break-words">{t.title}</span>
                  </button>
                </div>
                {#if t.parent_id !== null}
                  <span class="text-xs text-muted-foreground">↳ {titleOf(t.parent_id)}</span>
                {/if}
                <div class="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                  {#if t.priority !== 'none'}<span>{PRIORITY_LABEL[t.priority]}</span>{/if}
                  {#if t.due_date !== null}<span>{m.tasks_card_due({ date: t.due_date })}</span
                    >{/if}
                  {#if t.checklist_total > 0}<span>☑ {t.checklist_done}/{t.checklist_total}</span
                    >{/if}
                  {#if t.assignees.length > 0}<span
                      >{t.assignees.map((a) => a.display_name).join(', ')}</span
                    >{/if}
                </div>
                {#if canWrite}
                  <div class="flex gap-1" aria-hidden="true">
                    {#each ['left', 'up', 'down', 'right'] as const as action (action)}
                      <Button
                        size="sm"
                        variant="ghost"
                        tabindex={-1}
                        class="h-6 px-1.5 text-xs"
                        disabled={planBoardMove(shown, t.id, action) === undefined ||
                          (!ordered && (action === 'up' || action === 'down'))}
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
                  <DropdownMenu.Root>
                    <DropdownMenu.Trigger>
                      {#snippet child({ props })}
                        <Button
                          {...props}
                          size="sm"
                          variant="ghost"
                          class="h-6 self-start px-1.5 text-xs"
                          aria-label={m.tasks_move_menu({ title: t.title })}>⋯</Button
                        >
                      {/snippet}
                    </DropdownMenu.Trigger>
                    <DropdownMenu.Content align="start">
                      {#each TASK_STATUSES.filter((s) => s !== t.status) as target (target)}
                        <DropdownMenu.Item
                          onSelect={() =>
                            void commitMove(t.id, { status: target, beforeId: null }, 'menu')}
                          >{MOVE_TO[target]()}</DropdownMenu.Item
                        >
                      {/each}
                    </DropdownMenu.Content>
                  </DropdownMenu.Root>
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
              />
            {:else}
              <Button size="sm" variant="ghost" class="self-start" onclick={() => (adding = lane)}
                >{m.tasks_quick_add()}</Button
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
    <GanttChart tasks={shown} onopen={open} />
  {:else if view === 'calendar'}
    <TaskCalendar tasks={shown} onopen={open} />
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

<Dialog.Root
  open={openTaskId !== undefined}
  onOpenChange={(o: boolean) => {
    if (!o) close();
  }}
>
  <Dialog.Content
    class="max-h-[90vh] overflow-y-auto sm:max-w-3xl"
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
          onopen={open}
        />
      {/key}
    {/if}
  </Dialog.Content>
</Dialog.Root>
