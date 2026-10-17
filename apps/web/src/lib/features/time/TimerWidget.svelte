<script lang="ts">
  // 머리글의 타이머(D-19 "실행 중인 스톱워치", D-32). 진행 중이면 길이·대상과 일시정지·재개·종료, 아니면 시작.
  // 길이 글자는 1초마다 바뀌므로 화면 낭독기에는 읽히지 않게 하고, 상태가 바뀔 때만 알린다.
  import { ApiError } from '$lib/api/client';
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
      error = e instanceof ApiError ? e.message : '타이머를 바꾸지 못했습니다.';
    }
  }

  const label = $derived(
    timer.entry === null
      ? ''
      : (timer.entry.target?.label ?? TIME_KIND_LABEL[timer.entry.kind]) +
          (timer.entry.paused_at === null ? '' : ' (일시정지)'),
  );
</script>

<div class="flex items-center gap-1 text-sm" role="group" aria-label="타이머">
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
        onclick={() => void run(() => timer.act('pause'), '타이머를 일시정지했습니다.')}
        >일시정지</Button
      >
    {:else}
      <Button
        size="sm"
        variant="ghost"
        onclick={() => void run(() => timer.act('resume'), '타이머를 다시 시작했습니다.')}
        >재개</Button
      >
    {/if}
    <Button
      size="sm"
      variant="outline"
      onclick={() =>
        void run(
          () => timer.act('stop'),
          `타이머를 끝냈습니다(${clockText(timer.seconds)}). 내 시간에서 종류·대상을 정할 수 있습니다.`,
        )}>종료</Button
    >
  {:else}
    <Button
      size="sm"
      variant="outline"
      onclick={() => void run(() => timer.start('work'), '타이머를 시작했습니다.')}
      >타이머 시작</Button
    >
  {/if}
  <span class="sr-only" aria-live="polite">{status}</span>
  {#if error !== undefined}
    <span role="alert" class="text-destructive">{error}</span>
  {/if}
</div>
