<script lang="ts">
  // 머리글은 기록 중 표시와 멈춤만 둔다(D-117). 실행 타이머는 사용자당 모든 조직에서 하나라(D-32) 조직을 고르기 전에도
  // 읽고 보인다. 타이머가 없을 때만 조직을 고르라고 한다(R5 최종 A 확인 필요 1).
  // 길이 글자는 1초마다 바뀌므로 화면 낭독기에는 읽히지 않게 하고, 상태가 바뀔 때만 알린다.
  import { errorText } from '$lib/api/errors';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';
  import { org } from '$lib/org.svelte';
  import { notifyDone } from '$lib/features/feedback/feedback.svelte';

  import { clockText, TIME_KIND_LABEL } from './format';
  import { timer } from './timer.svelte';

  let status = $state('');
  let error = $state<string | undefined>(undefined);
  let busy = $state(false);

  $effect(() => {
    void timer.load();
    return timer.watchVisibility();
  });

  async function run(action: () => Promise<unknown>, done: string): Promise<void> {
    if (busy) return;
    busy = true;
    error = undefined;
    try {
      await action();
      status = done;
      notifyDone(done);
    } catch (e: unknown) {
      error = errorText(e, m.time_timer_change_failed());
    } finally {
      busy = false;
    }
  }

  const label = $derived.by(() => {
    if (timer.entry === null) return '';
    const name = timer.entry.target?.label ?? TIME_KIND_LABEL[timer.entry.kind];
    return timer.entry.paused_at === null ? name : m.time_timer_paused_label({ label: name });
  });
</script>

<div class="flex items-center gap-1 text-sm" role="group" aria-label={m.time_timer()}>
  {#if timer.entry === null}
    {#if org.currentId === undefined}<span>{m.app_organization_required()}</span>{/if}
  {:else}
    <span>{m.time_recording()}</span>
    <span class="hidden max-w-40 truncate text-muted-foreground sm:inline" title={label}
      >{label}</span
    >
    <span class="font-mono tabular-nums" aria-hidden="true">{clockText(timer.seconds)}</span>
    <Button
      size="sm"
      variant="outline"
      disabled={busy}
      onclick={() =>
        void run(
          () => timer.act('stop'),
          m.time_timer_stopped({ duration: clockText(timer.seconds) }),
        )}>{m.time_stop()}</Button
    >
  {/if}
  <span class="sr-only" role="status">{status}</span>
  {#if error !== undefined}
    <span role="alert" class="text-destructive">{error}</span>
  {/if}
</div>
