<script lang="ts">
  import type { Snippet } from 'svelte';
  import { m } from '$lib/paraglide/messages.js';
  const {
    lane,
    color,
    count,
    estimate,
    filtered,
    dragActive,
    children,
  }: {
    lane: string;
    color: string | undefined;
    count: number;
    estimate: string;
    filtered: boolean;
    dragActive: boolean;
    children: Snippet;
  } = $props();
</script>

<section
  data-lane={lane}
  class={[
    'flex min-w-0 snap-start snap-always flex-col gap-2 rounded-lg border bg-muted/40 p-2',
    dragActive && 'ring-2 ring-ring',
  ]}
  aria-label={m.tasks_lane_aria({ status: lane })}
>
  <!-- 열 이름 옆에 카드 수를 칩으로(참고 제품의 열 머리글 배치), 그 아래 예상 시간 합계. -->
  <h3 class="flex items-center gap-2 px-1 text-sm font-semibold">
    <span
      class="size-2 rounded-full bg-muted-foreground"
      style:background-color={color}
      aria-hidden="true"
    ></span>
    {lane}
    <span
      class="rounded-full bg-background px-2 py-0.5 text-xs font-normal text-muted-foreground tabular-nums"
      >{count}</span
    >
  </h3>
  {#if estimate !== ''}
    <p class="px-1 text-xs text-muted-foreground" data-lane-estimate={lane}>
      {estimate}
    </p>
  {/if}
  {#if count === 0}
    <p class="px-1 text-xs text-muted-foreground" data-lane-empty={lane}>
      {filtered ? m.tasks_lane_empty_filtered() : m.tasks_lane_empty()}
    </p>
  {/if}
  {@render children()}
</section>
