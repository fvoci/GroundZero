<script lang="ts">
  import type { TaskView } from './model';
  import type { BoardAction, BoardMove } from './board';
  import { planBoardMove } from './board';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';
  import * as DropdownMenu from '$lib/ui/dropdown-menu';
  import TaskTimeStart from '$lib/features/time/TaskTimeStart.svelte';
  import { PRIORITY_LABEL, PRIORITY_TONE } from './labels';
  interface Actions {
    startDrag: (event: PointerEvent, id: string, handle: boolean) => void;
    onCardKey: (event: KeyboardEvent, id: string) => void;
    open: (id: string) => void;
    busy: (id: string) => boolean;
    moveAndFocus: (id: string, plan: BoardMove, how: BoardAction | 'menu') => Promise<void>;
    move: (id: string, how: BoardAction) => Promise<void>;
    titleOf: (id: string | null) => string;
    recheck: (id: string) => void;
    retryMove: (id: string) => Promise<void>;
    keepAsIs: (id: string) => void;
    reorderable: (how: BoardAction) => boolean;
  }
  type SavePhase = 'saving' | 'slow' | 'checking' | 'unknown' | 'not_saved';
  const {
    t,
    canWrite,
    ordered,
    statusNames,
    shown,
    justDragged,
    dragging,
    save: pendingSave,
    actions,
  }: {
    t: TaskView;
    canWrite: boolean;
    ordered: boolean;
    statusNames: string[];
    shown: TaskView[];
    justDragged: boolean;
    dragging: boolean;
    save: { seq: number; phase: SavePhase } | undefined;
    actions: Actions;
  } = $props();
  const {
    startDrag,
    onCardKey,
    open,
    busy,
    moveAndFocus,
    move,
    titleOf,
    recheck,
    retryMove,
    keepAsIs,
    reorderable,
  } = $derived(actions);
  const SAVE_LABEL: Record<SavePhase, () => string> = {
    saving: m.tasks_card_saving,
    slow: m.tasks_card_slow,
    checking: m.tasks_card_checking,
    unknown: m.tasks_card_unknown,
    not_saved: m.tasks_card_not_saved,
  };
</script>

<li
  data-board-card={t.id}
  class={[
    'group flex min-w-0 flex-col gap-1.5 rounded-md border bg-card p-2 shadow-xs',
    canWrite && 'cursor-grab',
    dragging && 'opacity-40',
  ]}
  onpointerdown={(e: PointerEvent) => {
    startDrag(e, t.id, false);
  }}
>
  <!-- 제목은 전체 폭을 쓰고, DOM의 제목 → 시간 기록 → 메뉴 순서는 유지한다. -->
  <div class="grid min-w-0 grid-cols-[minmax(0,1fr)_auto] items-start gap-x-1 gap-y-1.5">
    <div class="col-start-1 row-start-1 flex min-w-0 items-start gap-1">
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
      <span aria-hidden="true" class="min-w-0 font-mono text-xs text-muted-foreground"
        >{t.reference ?? ''}</span
      >
    </div>
    <button
      type="button"
      data-task-card={t.id}
      aria-label={`${t.reference ?? ''} ${t.title}`.trim()}
      aria-keyshortcuts={canWrite
        ? 'Alt+ArrowUp Alt+ArrowDown Alt+ArrowLeft Alt+ArrowRight'
        : undefined}
      onkeydown={(e) => {
        onCardKey(e, t.id);
      }}
      class="col-span-full row-start-2 min-w-0 rounded text-left text-sm hover:underline focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
      onclick={() => {
        if (!justDragged) open(t.id);
      }}
    >
      <span class="block w-full min-w-0 font-medium break-words">{t.title}</span>
    </button>
    <div class="col-start-2 row-start-1 flex items-start gap-1">
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
                onSelect={() => void moveAndFocus(t.id, { status: target, beforeId: null }, 'menu')}
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
  </div>
  {#if t.parent_id !== null}
    <span class="text-xs text-muted-foreground">↳ {titleOf(t.parent_id)}</span>
  {/if}
  <div class="flex flex-wrap items-center gap-x-2 gap-y-1 text-xs text-muted-foreground">
    {#if t.priority !== 'none'}<span
        class={['rounded px-1.5 py-0.5 font-medium', PRIORITY_TONE[t.priority]]}
        >{PRIORITY_LABEL[t.priority]}</span
      >{/if}
    {#if t.due_date !== null}<span>{m.tasks_card_due({ date: t.due_date })}</span>{/if}
    {#if t.checklist_total > 0}<span>☑ {t.checklist_done}/{t.checklist_total}</span>{/if}
    {#if t.assignees.length > 0}<span>{t.assignees.map((a) => a.display_name).join(', ')}</span
      >{/if}
  </div>
  {#each [pendingSave] as save (save?.seq ?? -1)}
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
