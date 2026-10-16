<script lang="ts">
  // 항목의 업무 (D-31): 목록과 칸반, 업무 만들기, 상세 대화상자.
  // 칸반 카드는 버튼과 키보드(Alt+화살표)로 열·순서를 옮긴다. 상세는 주소(/t/{업무})로도 열린다.
  import type { Workspace } from '@fvoci/api-client';

  import type { ItemView, TaskView } from '$lib/api/client';
  import { api, ApiError, TASK_STATUSES } from '$lib/api/client';
  import CreateForm from '$lib/features/workspaces/CreateForm.svelte';
  import { itemPath, itemTaskPath, router } from '$lib/router.svelte';
  import { Button } from '$lib/ui/button';
  import * as Dialog from '$lib/ui/dialog';

  import type { BoardAction } from './board';
  import { lanes, listOrder, planBoardMove } from './board';
  import { PRIORITY_LABEL, STATUS_LABEL } from './labels';
  import TaskDetail from './TaskDetail.svelte';

  interface Props {
    item: ItemView;
    workspaces: Workspace[];
    /** 주소에서 연 업무. */
    openTaskId: string | undefined;
  }

  const { item, workspaces, openTaskId }: Props = $props();

  let tasks = $state<TaskView[]>([]);
  let view = $state<'list' | 'board'>('board');
  let error = $state<string | undefined>(undefined);
  let status = $state('');

  async function reload(itemId: string = item.id): Promise<void> {
    try {
      tasks = await api.tasks(itemId);
      error = undefined;
    } catch (e: unknown) {
      error = e instanceof ApiError ? e.message : '업무를 불러오지 못했습니다.';
    }
  }

  $effect(() => {
    void reload(item.id);
  });

  const canWrite = $derived(item.access !== 'read' && !item.archived);
  const board = $derived(lanes(tasks));
  const list = $derived(listOrder(tasks));
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

  const ACTION_LABEL: Record<BoardAction, string> = {
    up: '위로',
    down: '아래로',
    left: '왼쪽 열로',
    right: '오른쪽 열로',
  };

  async function move(id: string, action: BoardAction): Promise<void> {
    const plan = planBoardMove(tasks, id, action);
    if (plan === undefined) return;
    try {
      await api.moveTask(id, plan.status, plan.beforeId);
      await reload();
      status = `${titleOf(id)}: ${ACTION_LABEL[action]} 옮겼습니다(${STATUS_LABEL[plan.status]}).`;
      document.querySelector<HTMLElement>(`[data-task-card="${CSS.escape(id)}"]`)?.focus();
    } catch (e: unknown) {
      error = e instanceof ApiError ? e.message : '옮기지 못했습니다.';
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
    <h2 id="tasks-heading" class="text-lg font-semibold">업무</h2>
    <div class="ml-auto flex gap-1" role="group" aria-label="보기">
      <Button
        size="sm"
        variant={view === 'board' ? 'secondary' : 'ghost'}
        aria-pressed={view === 'board'}
        onclick={() => (view = 'board')}>칸반</Button
      >
      <Button
        size="sm"
        variant={view === 'list' ? 'secondary' : 'ghost'}
        aria-pressed={view === 'list'}
        onclick={() => (view = 'list')}>목록</Button
      >
    </div>
  </div>
  {#if error !== undefined}
    <p role="alert" class="text-sm text-destructive">{error}</p>
  {/if}

  {#if view === 'board'}
    <div class="grid gap-3 md:grid-cols-3" data-testid="task-board">
      {#each TASK_STATUSES as lane (lane)}
        <section
          class="flex min-w-0 flex-col gap-2 rounded-md border p-2"
          aria-label={`${STATUS_LABEL[lane]} 열`}
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
                  {#if t.due_date !== null}<span>마감 {t.due_date}</span>{/if}
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
                        disabled={planBoardMove(tasks, t.id, action) === undefined}
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
      <p class="text-xs text-muted-foreground">
        카드에서 Enter로 열고, Alt+왼쪽·오른쪽으로 열을, Alt+위·아래로 순서를 바꿉니다.
      </p>
    {/if}
  {:else}
    <table class="w-full text-sm" data-testid="task-list">
      <thead class="text-left text-muted-foreground">
        <tr
          ><th class="py-1 font-medium">번호</th><th class="font-medium">제목</th><th
            class="font-medium">상태</th
          ><th class="font-medium">담당</th><th class="font-medium">마감</th><th class="font-medium"
            >체크</th
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
          <tr><td colspan="6" class="py-2 text-muted-foreground">업무가 없습니다.</td></tr>
        {/each}
      </tbody>
    </table>
  {/if}

  {#if canWrite}
    <CreateForm
      id="new-task"
      label="새 업무 제목"
      placeholder="예: 로그인 화면"
      submitLabel="업무 만들기"
      create={async (requestId: string, title: string) => {
        await api.createTask(item.id, requestId, title);
        await reload();
      }}
    />
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
      <Dialog.Title>{openTaskId === undefined ? '' : titleOf(openTaskId) || '업무'}</Dialog.Title>
      <Dialog.Description>업무의 상태·담당·일정·체크리스트를 고칩니다.</Dialog.Description>
    </Dialog.Header>
    {#if openTaskId !== undefined}
      {#key openTaskId}
        <TaskDetail
          taskId={openTaskId}
          subtasks={tasks.filter((t) => t.parent_id === openTaskId)}
          {workspaces}
          onchanged={() => void reload()}
          onopen={open}
        />
      {/key}
    {/if}
  </Dialog.Content>
</Dialog.Root>
