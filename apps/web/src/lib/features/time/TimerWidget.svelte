<script lang="ts">
  // 머리글의 타이머(D-19 "실행 중인 스톱워치", D-32). 진행 중이면 길이·대상과 일시정지·재개·종료, 아니면 시작.
  // 길이 글자는 1초마다 바뀌므로 화면 낭독기에는 읽히지 않게 하고, 상태가 바뀔 때만 알린다.
  import { errorText } from '$lib/api/errors';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';

  import { clockText, TIME_KIND_LABEL } from './format';
  import { timer } from './timer.svelte';

  let status = $state('');
  let error = $state<string | undefined>(undefined);

  $effect(() => {
    void timer.load();
    return timer.watchVisibility();
  });

  async function run(action: () => Promise<unknown>, done: string): Promise<void> {
    error = undefined;
    try {
      await action();
      status = done;
    } catch (e: unknown) {
      error = errorText(e, m.time_timer_change_failed());
    }
  }

  const label = $derived.by(() => {
    if (timer.entry === null) return '';
    const name = timer.entry.target?.label ?? TIME_KIND_LABEL[timer.entry.kind];
    return timer.entry.paused_at === null ? name : m.time_timer_paused_label({ label: name });
  });
</script>

<div class="flex items-center gap-1 text-sm" role="group" aria-label={m.time_timer()}>
  {#if timer.entry !== null}
    {@const entry = timer.entry}
    <span class="hidden max-w-40 truncate text-muted-foreground sm:inline" title={label}
      >{label}</span
    >
    <span class="font-mono tabular-nums" aria-hidden="true">{clockText(timer.seconds)}</span>
    {#if entry.paused_at === null}
      <Button
        size="sm"
        variant="ghost"
        onclick={() => void run(() => timer.act('pause'), m.time_timer_paused())}
        >{m.time_pause()}</Button
      >
    {:else}
      <Button
        size="sm"
        variant="ghost"
        onclick={() => void run(() => timer.act('resume'), m.time_timer_resumed())}
        >{m.time_resume()}</Button
      >
    {/if}
    <Button
      size="sm"
      variant="outline"
      onclick={() =>
        void run(
          () => timer.act('stop'),
          m.time_timer_stopped({ duration: clockText(timer.seconds) }),
        )}>{m.time_stop()}</Button
    >
  {:else}
    <Button
      size="sm"
      variant="outline"
      onclick={() => void run(() => timer.start('work'), m.time_timer_started())}
      >{m.time_timer_start()}</Button
    >
  {/if}
  <span class="sr-only" aria-live="polite">{status}</span>
  {#if error !== undefined}
    <span role="alert" class="text-destructive">{error}</span>
  {/if}
</div>
