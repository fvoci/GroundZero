<script lang="ts">
  import type { ComponentProps } from 'svelte';
  import type { TaskStatusDefinition } from '@fvoci/api-client';
  import type { TaskView } from './model';
  import type { DropSlot } from './board';
  import BoardCard from './BoardCard.svelte';
  import BoardLane from './BoardLane.svelte';
  import BoardQuickAdd from './BoardQuickAdd.svelte';
  import { m } from '$lib/paraglide/messages.js';

  interface Props {
    boardElement: HTMLDivElement | undefined;
    adding: string | undefined;
    statusNames: string[];
    definitions: TaskStatusDefinition[];
    board: Record<string, TaskView[]>;
    laneEstimateText: (lane: string) => string;
    titleOf: (id: string | null) => string;
    filtered: boolean;
    drag:
      { id: string; active: boolean; slot: DropSlot | undefined; x: number; y: number } | undefined;
    dropMark: (lane: string, beforeId: string | null) => boolean;
    pending: Record<string, ComponentProps<typeof BoardCard>['save']>;
    cards: Omit<ComponentProps<typeof BoardCard>, 't' | 'dragging' | 'save'>;
    quickInitial: { lane: string; name: string; requestId: string | null } | undefined;
    quickAdd: (lane: string, requestId: string, title: string) => Promise<void>;
    closeQuickAdd: (lane: string) => Promise<void>;
    keepNewDraft: (
      lane: string | null,
      draft: { name: string; requestId: string | null } | undefined,
    ) => void;
  }
  let {
    boardElement = $bindable(),
    adding = $bindable(),
    statusNames,
    definitions,
    board,
    laneEstimateText,
    titleOf,
    filtered,
    drag,
    dropMark,
    pending,
    cards,
    quickInitial,
    quickAdd,
    closeQuickAdd,
    keepNewDraft,
  }: Props = $props();
</script>

<div
  bind:this={boardElement}
  class={[
    'grid min-w-0 auto-cols-[100%] grid-flow-col gap-3 overflow-x-auto @2xl:snap-none @2xl:auto-cols-auto @2xl:grid-flow-row @2xl:grid-cols-3',
    drag?.active === true ? 'snap-none' : 'snap-x snap-mandatory',
  ]}
  data-testid="task-board"
>
  {#each statusNames as lane (lane)}
    <BoardLane
      {lane}
      color={definitions.find((s) => s.name === lane)?.color}
      count={board[lane]?.length ?? 0}
      estimate={laneEstimateText(lane)}
      {filtered}
      dragActive={drag?.active === true && drag.slot?.status === lane}
    >
      <ul class="flex min-h-12 flex-col gap-2">
        {#each board[lane] as t (t.id)}
          {#if dropMark(lane, t.id)}
            <li aria-hidden="true" class="h-1 rounded bg-primary"></li>
          {/if}
          <BoardCard
            {t}
            {...cards}
            dragging={drag?.active === true && drag.id === t.id}
            save={pending[t.id]}
          />
        {/each}
        {#if dropMark(lane, null)}
          <li aria-hidden="true" class="h-1 rounded bg-primary"></li>
        {/if}
      </ul>
      {#if cards.canWrite}<BoardQuickAdd
          {lane}
          adding={adding === lane}
          initial={quickInitial?.lane === lane ? quickInitial : undefined}
          create={(id: string, title: string) => quickAdd(lane, id, title)}
          cancel={() => closeQuickAdd(lane)}
          onchange={(draft: { name: string; requestId: string | null } | undefined) => {
            keepNewDraft(lane, draft);
          }}
          onopen={() => {
            adding = lane;
          }}
        />{/if}
    </BoardLane>
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
{#if cards.canWrite}
  <p class="text-xs text-muted-foreground">
    {m.tasks_board_hint()}{cards.ordered ? '' : ` ${m.tasks_board_hint_unordered()}`}
  </p>
{/if}
<!-- 열 머리 예상 시간의 뜻. 업무보다 먼저 보이지 않게 보드 아래에 둔다. -->
<p class="text-xs text-muted-foreground">{m.tasks_lane_estimate_hint()}</p>
