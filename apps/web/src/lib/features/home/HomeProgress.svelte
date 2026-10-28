<script lang="ts">
  import type { HomeProgress } from './api';
  import { unitAmount } from '$lib/features/study/progress';
  import { m } from '$lib/paraglide/messages.js';

  const { progress }: { progress: HomeProgress } = $props();
  const rows = $derived.by(() => {
    if (progress.kind === 'study') {
      return progress.amounts.map((amount) => ({
        done: amount.done,
        total: amount.target,
        label: m.home_study_done({
          done: unitAmount(amount.unit, amount.done),
          target: unitAmount(amount.unit, amount.target),
        }),
      }));
    }
    return [
      {
        done: progress.done,
        total: progress.total,
        label:
          progress.kind === 'course'
            ? m.home_course_done({ done: progress.done, total: progress.total })
            : m.home_tasks_done({ done: progress.done, total: progress.total }),
      },
    ];
  });
</script>

{#each rows as row}
  {#if row.total > 0}
    <span class="flex items-center gap-2 text-xs text-muted-foreground">
      <span class="h-1.5 w-32 overflow-hidden rounded bg-muted" aria-hidden="true">
        <span
          class="block h-full bg-primary"
          style:width={`${String(Math.round((row.done / row.total) * 100))}%`}
        ></span>
      </span>
      {row.label}
    </span>
  {/if}
{/each}
