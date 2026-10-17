<script lang="ts">
  // 휴지통으로 옮기기 단추(한 번 더 묻는다). 확인 단계로 바뀌면 초점이 확인 단추로 가고, 처리 중에는 다시 누를
  // 수 없다. 지운 뒤 할 일은 부르는 쪽이 정한다.
  import { tick } from 'svelte';

  import { ApiError } from '$lib/api/client';
  import { notifyDone } from '$lib/features/feedback/feedback.svelte';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';

  interface Props {
    /** 예: "문서", "업무", "항목". */
    what: string;
    remove: () => Promise<unknown>;
    ondeleted: () => void;
  }

  const { what, remove, ondeleted }: Props = $props();

  let confirming = $state(false);
  let busy = $state(false);
  let error = $state<string | undefined>(undefined);
  let confirmButton = $state<HTMLButtonElement | null>(null);
  let startButton = $state<HTMLButtonElement | null>(null);

  async function ask(): Promise<void> {
    confirming = true;
    await tick();
    confirmButton?.focus();
  }

  async function cancel(): Promise<void> {
    confirming = false;
    await tick();
    startButton?.focus();
  }

  async function go(): Promise<void> {
    if (busy) return;
    busy = true;
    error = undefined;
    try {
      await remove();
      confirming = false;
      notifyDone(m.common_moved_to_trash());
      ondeleted();
    } catch (e: unknown) {
      error = e instanceof ApiError ? e.message : '지우지 못했습니다.';
    } finally {
      busy = false;
    }
  }
</script>

<span class="flex flex-wrap items-center gap-1">
  {#if confirming}
    <span class="text-sm" role="status">{what}: 휴지통으로 옮길까요?</span>
    <Button
      size="sm"
      variant="destructive"
      bind:ref={confirmButton}
      disabled={busy}
      onclick={() => void go()}>휴지통으로</Button
    >
    <Button size="sm" variant="ghost" onclick={() => void cancel()}>취소</Button>
  {:else}
    <Button size="sm" variant="ghost" bind:ref={startButton} onclick={() => void ask()}
      >{what} 지우기</Button
    >
  {/if}
  {#if error !== undefined}<span role="alert" class="text-sm text-destructive">{error}</span>{/if}
</span>
