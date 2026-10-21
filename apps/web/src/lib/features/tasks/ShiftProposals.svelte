<script lang="ts">
  // 일정이나 선행 관계를 바꾸면 영향을 받는 후속 업무(D-31, D-91) 미리보기와 승인. 업무 상세의 저장·관계 고치기와
  // 간트 끌기가 같이 쓴다. 업무마다 지금 → 제안 날짜와 옮기게 한 관계(선행 업무, 종류, 지연일)를 글로 보인다.
  import type { ShiftProposalView, UndatedTaskView } from '$lib/api/client';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';

  import { linkText } from './labels';

  interface Props {
    proposals: ShiftProposalView[];
    /** 바뀐 관계가 닿지만 날짜가 없어 옮기지 않는 업무(날짜를 지어내지 않는다). */
    undated?: UndatedTaskView[];
    /** 승인하려는 사이 후속 업무가 바뀌어 다시 계산한 것인지. */
    retried: boolean;
    busy: boolean;
    /** 제목 요소 id(같은 화면에 둘이 있어도 겹치지 않게). */
    headingId?: string;
    /** 관계 고치기에서 띄웠는지(단추 글이 "관계 저장…"이 된다). */
    forLink?: boolean;
    ondecide: (withShifts: boolean) => void;
    oncancel: () => void;
  }

  const {
    proposals,
    undated = [],
    retried,
    busy,
    headingId = 'shift-heading',
    forLink = false,
    ondecide,
    oncancel,
  }: Props = $props();

  const range = (start: string | null, due: string | null): string =>
    start !== null && due !== null && start !== due
      ? `${start}~${due}`
      : (due ?? start ?? m.tasks_no_dates());
  const named = (t: { reference: string | null; title: string }): string =>
    `${t.reference ?? ''} ${t.title}`.trim();

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
  <ul class="flex flex-col gap-1.5">
    {#each proposals as s (s.task_id)}
      <li data-shift-task={s.task_id}>
        <span class="font-mono text-xs text-muted-foreground">{s.reference ?? ''}</span>
        {s.title}: {range(s.from_start_date, s.from_due_date)} → {range(s.start_date, s.due_date)}
        {#each s.causes as c (c.predecessor_id)}
          <span class="block text-xs text-muted-foreground" data-shift-cause={c.predecessor_id}>
            {m.tasks_shift_cause({
              predecessor: named({
                reference: c.predecessor_reference,
                title: c.predecessor_title,
              }),
              relation: linkText(c),
            })}
          </span>
        {/each}
      </li>
    {/each}
  </ul>
  {#if undated.length > 0}
    <p class="text-muted-foreground" data-testid="shift-undated">
      {m.tasks_shift_undated({ tasks: undated.map(named).join(', ') })}
    </p>
  {/if}
  <div class="flex flex-wrap gap-2">
    <Button
      size="sm"
      disabled={busy}
      onclick={() => {
        ondecide(true);
      }}>{forLink ? m.tasks_shift_link_save_all() : m.tasks_shift_save_all()}</Button
    >
    <Button
      size="sm"
      variant="outline"
      disabled={busy}
      onclick={() => {
        ondecide(false);
      }}>{forLink ? m.tasks_shift_link_save_this() : m.tasks_shift_save_this()}</Button
    >
    <Button size="sm" variant="ghost" disabled={busy} onclick={oncancel}>{m.common_cancel()}</Button
    >
  </div>
</section>
