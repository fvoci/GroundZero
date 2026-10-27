<script lang="ts">
  // 저장하지 않은 입력이 있는 화면에서 DiscardChangesConfirm을 부르는 합성 예. 업무·일정 대화상자처럼 입력이 바뀌었을
  // 때만 묻고, 계속 편집하면 입력과 초점을 그대로 돌려준다.
  import DiscardChangesConfirm from '$lib/features/common/DiscardChangesConfirm.svelte';
  import { createLeaveGuard } from '$lib/features/common/leave-guard.svelte';
  import { Button } from '$lib/ui/button';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';

  interface Props {
    initial: string;
    description?: string | undefined;
    confirmLabel?: string | undefined;
    ondiscard: () => void;
  }

  const { initial, description, confirmLabel, ondiscard }: Props = $props();

  let value = $state('');
  let saved = $state('');
  const leaveGuard = createLeaveGuard({ dirty: () => value !== saved });
  let box = $state<HTMLInputElement | null>(null);

  $effect.pre(() => {
    value = initial;
    saved = initial;
  });
</script>

<div class="flex max-w-sm flex-col gap-2">
  <Label for="story-discard-title">업무 제목</Label>
  <Input id="story-discard-title" bind:ref={box} bind:value />
  <div>
    <Button
      variant="outline"
      onclick={() => {
        leaveGuard.request(() => {
          value = saved;
          ondiscard();
        });
      }}>닫기</Button
    >
  </div>
</div>

<DiscardChangesConfirm
  open={leaveGuard.open}
  onkeep={leaveGuard.keep}
  ondiscard={leaveGuard.discard}
  returnFocus={() => box}
  {description}
  {confirmLabel}
/>
