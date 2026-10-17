<script lang="ts">
  // 업무의 예상·실제 시간(D-32): 내 시간, 공유를 켠 사람들의 합계, 이 업무로 타이머 시작.
  import type { TaskTimeView } from '$lib/api/client';
  import { api, ApiError } from '$lib/api/client';
  import { Button } from '$lib/ui/button';

  import { durationText } from './format';
  import { timer } from './timer.svelte';

  interface Props {
    taskId: string;
  }

  const { taskId }: Props = $props();

  let time = $state<TaskTimeView | undefined>(undefined);
  let error = $state<string | undefined>(undefined);
  let confirmSwitch = $state(false);

  async function reload(id: string = taskId): Promise<void> {
    try {
      time = await api.taskTime(id);
    } catch {
      time = undefined;
    }
  }

  $effect(() => {
    void reload(taskId);
  });

  const runningHere = $derived(
    timer.entry !== null && timer.entry.target?.kind === 'task' && timer.entry.target.id === taskId,
  );

  async function startHere(): Promise<void> {
    error = undefined;
    try {
      if (timer.entry !== null) {
        // 사용자당 타이머 하나: 다른 타이머를 끝낼지 먼저 묻는다.
        if (!confirmSwitch) {
          confirmSwitch = true;
          return;
        }
        await timer.act('stop');
      }
      confirmSwitch = false;
      await timer.start('work', { kind: 'task', id: taskId });
    } catch (e: unknown) {
      error = e instanceof ApiError ? e.message : '타이머를 시작하지 못했습니다.';
    }
  }

  async function stopHere(): Promise<void> {
    await timer.act('stop');
    await reload();
  }
</script>

<section class="flex flex-col gap-2" aria-labelledby="task-time-heading">
  <h3 id="task-time-heading" class="text-sm font-semibold">시간</h3>
  {#if time !== undefined}
    <p class="text-sm">
      예상 {time.estimate_minutes === null ? '없음' : durationText(time.estimate_minutes * 60)} · 내 시간
      {durationText(time.mine_seconds + (runningHere ? timer.seconds : 0))}
      {#if time.shared_people > 0}
        · 공유된 합계 {durationText(time.shared_seconds)}({time.shared_people}명)
      {/if}
    </p>
  {/if}
  <div class="flex flex-wrap items-center gap-2">
    {#if runningHere}
      <span class="text-sm" role="status">이 업무의 타이머가 돌고 있습니다.</span>
      <Button size="sm" variant="outline" onclick={() => void stopHere()}>타이머 끝내기</Button>
    {:else if confirmSwitch}
      <span class="text-sm" role="status"
        >다른 타이머가 돌고 있습니다. 끝내고 이 업무로 시작할까요?</span
      >
      <Button size="sm" onclick={() => void startHere()}>끝내고 시작</Button>
      <Button size="sm" variant="ghost" onclick={() => (confirmSwitch = false)}>취소</Button>
    {:else}
      <Button size="sm" variant="outline" onclick={() => void startHere()}>이 업무 시간 재기</Button
      >
    {/if}
  </div>
  {#if error !== undefined}
    <p role="alert" class="text-sm text-destructive">{error}</p>
  {/if}
</section>
