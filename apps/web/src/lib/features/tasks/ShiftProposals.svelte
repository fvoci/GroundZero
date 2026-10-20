<script lang="ts">
  // 일정을 바꾸면 영향을 받는 후속 업무(D-31) 미리보기와 승인. 업무 상세의 저장과 간트 끌기가 같이 쓴다.
  import type { ShiftProposalView } from '$lib/api/client';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';

  interface Props {
    proposals: ShiftProposalView[];
    /** 승인하려는 사이 후속 업무가 바뀌어 다시 계산한 것인지. */
    retried: boolean;
    busy: boolean;
    /** 제목 요소 id(같은 화면에 둘이 있어도 겹치지 않게). */
    headingId?: string;
    ondecide: (withShifts: boolean) => void;
    oncancel: () => void;
  }

  const {
    proposals,
    retried,
    busy,
    headingId = 'shift-heading',
    ondecide,
    oncancel,
  }: Props = $props();

  const range = (start: string | null, due: string | null): string =>
    start !== null && due !== null && start !== due
      ? `${start}~${due}`
      : (due ?? start ?? m.tasks_no_dates());

  let heading = $state<HTMLElement | undefined>(undefined);
  $effect(() => {
    heading?.focus();
  });
</script>

<section
  class="flex flex-col gap-2 rounded-md border border-amber-500/50 p-3 text-sm"
  aria-labelledby={headingId}
>
  <h3 id={headingId} class="font-semibold" tabindex="-1" bind:this={heading}>
    {m.tasks_shift_heading({ count: proposals.length })}
  </h3>
  {#if retried}
    <p role="alert">{m.tasks_shift_retried()}</p>
  {/if}
  <p class="text-muted-foreground">{m.tasks_shift_hint()}</p>
  <ul class="flex flex-col gap-1">
    {#each proposals as s (s.task_id)}
      <li>
        <span class="font-mono text-xs text-muted-foreground">{s.reference ?? ''}</span>
        {s.title}: {range(s.from_start_date, s.from_due_date)} → {range(s.start_date, s.due_date)}
      </li>
    {/each}
  </ul>
  <div class="flex flex-wrap gap-2">
    <Button
      size="sm"
      disabled={busy}
      onclick={() => {
        ondecide(true);
      }}>{m.tasks_shift_save_all()}</Button
    >
    <Button
      size="sm"
      variant="outline"
      disabled={busy}
      onclick={() => {
        ondecide(false);
      }}>{m.tasks_shift_save_this()}</Button
    >
    <Button size="sm" variant="ghost" onclick={oncancel}>{m.common_cancel()}</Button>
  </div>
</section>
