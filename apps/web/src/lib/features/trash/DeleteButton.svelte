<script lang="ts">
  // 휴지통으로 옮기기 단추(한 번 더 묻는다). 확인 단계로 바뀌면 초점이 확인 단추로 가고, 처리 중에는 다시 누를
  // 수 없다. 지운 뒤 할 일은 부르는 쪽이 정한다.
  import { tick } from 'svelte';

  import { errorText } from '$lib/api/errors';
  import { notifyDone } from '$lib/features/feedback/feedback.svelte';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';

  interface Props {
    /** 예: "문서", "업무", "항목". */
    what: string;
    remove: () => Promise<unknown>;
    ondeleted: () => void;
    /** 묻는 중인지. 다른 곳(문서 화면의 "더 보기" 메뉴)에서 묻기를 시작할 수 있다. */
    confirming?: boolean;
    /** 시작 단추를 그릴지. 끄면 묻는 동안만 보이고, 취소하면 `oncancel`로 초점을 돌려줄 곳을 부르는 쪽이 정한다. */
    trigger?: boolean;
    oncancel?: () => void;
  }

  let {
    what,
    remove,
    ondeleted,
    confirming = $bindable(false),
    trigger = true,
    oncancel,
  }: Props = $props();

  let busy = $state(false);
  let error = $state<string | undefined>(undefined);
  let confirmButton = $state<HTMLButtonElement | null>(null);
  let startButton = $state<HTMLButtonElement | null>(null);

  // 묻기로 바뀌면(이 단추로든 밖에서든) 초점을 확인 단추로 옮긴다.
  $effect(() => {
    if (confirming) confirmButton?.focus();
  });

  async function cancel(): Promise<void> {
    confirming = false;
    await tick();
    if (trigger) startButton?.focus();
    else oncancel?.();
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
      error = errorText(e, m.trash_delete_failed());
    } finally {
      busy = false;
    }
  }
</script>

<span class="flex flex-wrap items-center gap-1">
  <!-- 묻는 글은 늘 있는 상태 영역에서 바꾼다(글과 함께 영역을 새로 넣으면 화면 낭독기가 놓칠 수 있다). -->
  <span class="text-sm empty:hidden" role="status">
    {#if confirming}{m.trash_move_question({ what })}{/if}
  </span>
  {#if confirming}
    <Button
      size="sm"
      variant="destructive"
      bind:ref={confirmButton}
      disabled={busy}
      onclick={() => void go()}>{m.trash_move()}</Button
    >
    <Button size="sm" variant="ghost" onclick={() => void cancel()}>{m.common_cancel()}</Button>
  {:else if trigger}
    <Button size="sm" variant="ghost" bind:ref={startButton} onclick={() => (confirming = true)}
      >{m.trash_delete_what({ what })}</Button
    >
  {/if}
  {#if error !== undefined}<span role="alert" class="text-sm text-destructive">{error}</span>{/if}
</span>
