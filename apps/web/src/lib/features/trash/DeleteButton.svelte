<script lang="ts">
  // 휴지통으로 옮기기 단추(한 번 더 묻는다). 지운 뒤 할 일은 부르는 쪽이 정한다.
  import { ApiError } from '$lib/api/client';
  import { Button } from '$lib/ui/button';

  interface Props {
    /** 예: "문서", "업무", "항목". */
    what: string;
    remove: () => Promise<unknown>;
    ondeleted: () => void;
  }

  const { what, remove, ondeleted }: Props = $props();

  let confirming = $state(false);
  let error = $state<string | undefined>(undefined);

  async function go(): Promise<void> {
    error = undefined;
    try {
      await remove();
      confirming = false;
      ondeleted();
    } catch (e: unknown) {
      error = e instanceof ApiError ? e.message : '지우지 못했습니다.';
    }
  }
</script>

<span class="flex flex-wrap items-center gap-1">
  {#if confirming}
    <span class="text-sm" role="status">{what}: 휴지통으로 옮길까요?</span>
    <Button size="sm" variant="destructive" onclick={() => void go()}>휴지통으로</Button>
    <Button size="sm" variant="ghost" onclick={() => (confirming = false)}>취소</Button>
  {:else}
    <Button size="sm" variant="ghost" onclick={() => (confirming = true)}>{what} 지우기</Button>
  {/if}
  {#if error !== undefined}<span role="alert" class="text-sm text-destructive">{error}</span>{/if}
</span>
