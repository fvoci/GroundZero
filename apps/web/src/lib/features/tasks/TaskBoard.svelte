<script lang="ts">
  // 항목의 업무 (D-31): 칸반·목록·간트·달력, 검색·필터·정렬과 저장한 보기, 업무 만들기, 상세 대화상자.
  // 칸반 카드는 버튼과 키보드(Alt+화살표)로 열·순서를 옮긴다. 상세는 주소(/t/{업무})로도 열린다.
  import type { Workspace } from '@fvoci/api-client';

  import type { ItemView, SavedViewView, TaskStatus, TaskView } from '$lib/api/client';
  import { api, newRequestId, TASK_STATUSES } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import { me } from '$lib/me.svelte';
  import { m } from '$lib/paraglide/messages.js';
  import CreateForm from '$lib/features/workspaces/CreateForm.svelte';
  import TaskCsvImport from './TaskCsvImport.svelte';
  import { itemPath, itemTaskPath, router } from '$lib/router.svelte';
  import { Button } from '$lib/ui/button';
  import * as Dialog from '$lib/ui/dialog';

  import type { BoardAction } from './board';
  import { lanes, listOrder, planBoardMove } from './board';
  import type { TaskFilter, TaskSort, TaskViewMode } from './filters';
  import {
    applyFilter,
    assigneesOf,
    EMPTY_FILTER,
    isFiltered,
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
  }

  const { item, workspaces, openTaskId }: Props = $props();

  let tasks = $state<TaskView[]>([]);
  let view = $state<TaskViewMode>('board');
  let error = $state<string | undefined>(undefined);
  let status = $state('');

  // 검색·필터·정렬과 저장한 보기(D-31).
  let filter = $state<TaskFilter>({ ...EMPTY_FILTER });
  let sort = $state<TaskSort>('position');
  let views = $state<SavedViewView[]>([]);
  let selectedView = $state('');
  let viewName = $state('');
  let viewRequest = newRequestId();
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
      const saved = await api.saveView(item.id, viewRequest, name, {
        view,
        filter: { ...filter },
        sort,
      });
      viewRequest = newRequestId();
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

  function open(id: string): void {
    router.navigate(itemTaskPath(item.workspace_id, item.id, id));
  }

  // 주소로 연 대화상자는 여는 단추를 모르므로, 닫으면 마지막으로 연 업무의 카드로 초점을 돌린다.
  let returnTo: string | undefined;
  $effect(() => {
    if (openTaskId !== undefined) returnTo = openTaskId;
  });

  function close(): void {
    router.navigate(itemPath(item.workspace_id, item.id));
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

  async function move(id: string, action: BoardAction): Promise<void> {
    const plan = planBoardMove(shown, id, action);
    if (plan === undefined) return;
    try {
      await api.moveTask(id, plan.status, plan.beforeId);
      await reload();
      status = movedText(action, titleOf(id), plan.status);
      document.querySelector<HTMLElement>(`[data-task-card="${CSS.escape(id)}"]`)?.focus();
    } catch (e: unknown) {
      error = errorText(e, m.tasks_move_failed());
    }
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
  {#if error !== undefined}
    <p role="alert" class="text-sm text-destructive">{error}</p>
  {/if}

  {#if view === 'board'}
    <div class="grid gap-3 md:grid-cols-3" data-testid="task-board">
      {#each TASK_STATUSES as lane (lane)}
        <section
          class="flex min-w-0 flex-col gap-2 rounded-md border p-2"
          aria-label={m.tasks_lane_aria({ status: STATUS_LABEL[lane] })}
        >
          <h3 class="text-sm font-semibold">
            {STATUS_LABEL[lane]}
            <span class="font-normal text-muted-foreground">{board[lane].length}</span>
          </h3>
          <ul class="flex flex-col gap-2">
            {#each board[lane] as t (t.id)}
              <li class="flex flex-col gap-1 rounded-md border bg-card p-2">
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
                    open(t.id);
                  }}
                >
                  <span class="shrink-0 font-mono text-xs text-muted-foreground"
                    >{t.reference ?? ''}</span
                  >
                  <span class="min-w-0 break-words">{t.title}</span>
                </button>
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
                {/if}
              </li>
            {/each}
          </ul>
        </section>
      {/each}
    </div>
    {#if canWrite}
      <p class="text-xs text-muted-foreground">{m.tasks_board_hint()}</p>
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
