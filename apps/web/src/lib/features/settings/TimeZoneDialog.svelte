<script lang="ts">
  // 내 시간대(D-50). 개인 시간 집계·홈·달력의 날짜를 이 시간대로 본다. 비우면 브라우저 시간대.
  import { api, ApiError } from '$lib/api/client';
  import { me } from '$lib/me.svelte';
  import { Button } from '$lib/ui/button';
  import * as Dialog from '$lib/ui/dialog';

  let { open = $bindable(false) }: { open?: boolean } = $props();

  const browserZone = Intl.DateTimeFormat().resolvedOptions().timeZone;
  const zones: string[] =
    typeof Intl.supportedValuesOf === 'function'
      ? Intl.supportedValuesOf('timeZone')
      : [browserZone];
  let choice = $state('');
  let error = $state<string | undefined>(undefined);

  $effect(() => {
    if (open) {
      choice = me.timeZone;
      error = undefined;
    }
  });

  async function save(): Promise<void> {
    try {
      await api.setTimeZone(choice);
      me.timeZone = choice;
      open = false;
      // 날짜 계산을 새 시간대로 다시 하도록 지금 화면을 새로 읽는다.
      window.location.reload();
    } catch (e: unknown) {
      error = e instanceof ApiError ? e.message : '저장하지 못했습니다.';
    }
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="sm:max-w-md">
    <Dialog.Header>
      <Dialog.Title>내 시간대</Dialog.Title>
      <Dialog.Description
        >개인 시간 집계·홈·달력의 날짜를 이 시간대로 봅니다. 다른 사람에게는 보이지 않습니다.</Dialog.Description
      >
    </Dialog.Header>
    <label class="flex flex-col gap-1 text-sm">
      <span class="font-medium">시간대</span>
      <select
        class="h-9 rounded-md border border-input bg-transparent px-3 text-base"
        bind:value={choice}
      >
        <option value="">브라우저 시간대 사용({browserZone})</option>
        {#each zones as z (z)}<option value={z}>{z}</option>{/each}
      </select>
    </label>
    {#if error !== undefined}<p role="alert" class="text-sm text-destructive">{error}</p>{/if}
    <div class="flex justify-end gap-2">
      <Button variant="ghost" onclick={() => (open = false)}>취소</Button>
      <Button onclick={() => void save()}>저장</Button>
    </div>
  </Dialog.Content>
</Dialog.Root>
