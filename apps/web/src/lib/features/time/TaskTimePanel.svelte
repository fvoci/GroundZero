<script lang="ts">
  // 업무의 예상·실제 시간(D-32): 내 시간, 공유를 켠 사람들의 합계, 이 업무로 타이머 시작.
  import type { TaskTimeView } from '$lib/api/client';
  import { api } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';

  import { durationText } from './format';
  import { timer } from './timer.svelte';

  interface Props {
    taskId: string;
  }

  const { taskId }: Props = $props();

  let time = $state<TaskTimeView | undefined>(undefined);
  // 합계를 못 읽었을 때. 조용히 비우지 않고 알리며, 이미 보이던 합계는 그대로 둔다.
  let loadError = $state<string | undefined>(undefined);
  let error = $state<string | undefined>(undefined);
  let confirmSwitch = $state(false);

  // 읽기가 겹치면(처음 읽기·타이머 종료 뒤·다시 시도) 마지막으로 보낸 읽기의 결과만 남긴다. 화면이 사라지면 늘려
  // 늦은 응답을 버린다.
  let readSeq = 0;

  async function reload(id: string = taskId): Promise<void> {
    const mine = ++readSeq;
    loadError = undefined;
    try {
      const next = await api.taskTime(id);
      if (mine !== readSeq) return;
      time = next;
    } catch (e: unknown) {
      if (mine === readSeq) loadError = errorText(e, m.time_load_failed());
    }
  }

  $effect(() => {
    const id = taskId;
    time = undefined;
    void reload(id);
    return () => {
      readSeq += 1;
    };
  });

  const runningHere = $derived(
    timer.entry !== null && timer.entry.target?.kind === 'task' && timer.entry.target.id === taskId,
  );

  // 이 업무의 타이머가 다른 곳(머리글)에서 끝나면 끝난 기록이 합계에 들도록 다시 불러온다.
  let wasRunning = false;
  $effect(() => {
    const now = runningHere;
    if (wasRunning && !now) void reload();
    wasRunning = now;
  });

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
      error = errorText(e, m.time_start_failed());
    }
  }

  async function stopHere(): Promise<void> {
    error = undefined;
    try {
      await timer.act('stop');
      await reload();
    } catch (e: unknown) {
      error = errorText(e, m.time_stop_failed());
    }
  }
</script>

<section class="flex flex-col gap-2" aria-labelledby="task-time-heading">
  <h3 id="task-time-heading" class="text-sm font-semibold">{m.time_heading()}</h3>
  {#if time !== undefined}
    <p class="text-sm">
      {m.time_task_summary({
        estimate:
          time.estimate_minutes === null ? m.time_none() : durationText(time.estimate_minutes * 60),
        mine: durationText(time.mine_seconds + (runningHere ? timer.seconds : 0)),
      })}
      {#if time.shared_people > 0}
        {m.time_task_shared({
          total: m.time_with_people({
            total: durationText(time.shared_seconds),
            count: time.shared_people,
          }),
        })}
      {/if}
    </p>
  {/if}
  {#if loadError !== undefined}
    <p role="alert" class="text-sm text-destructive">
      {loadError}
      <Button size="sm" variant="outline" class="ml-2" onclick={() => void reload()}
        >{m.common_retry()}</Button
      >
    </p>
  {/if}
  <div class="flex flex-wrap items-center gap-2">
    <!-- 상태 글은 처음부터 있는 알림 영역 안에서만 바뀐다(새로 만든 영역은 낭독기가 읽지 않을 수 있다). -->
    <span class="text-sm empty:sr-only" role="status"
      >{runningHere
        ? m.time_task_running()
        : confirmSwitch
          ? m.time_task_switch_confirm()
          : ''}</span
    >
    {#if runningHere}
      <Button size="sm" variant="outline" onclick={() => void stopHere()}
        >{m.time_stop_timer()}</Button
      >
    {:else if confirmSwitch}
      <Button size="sm" onclick={() => void startHere()}>{m.time_stop_and_start()}</Button>
      <Button size="sm" variant="ghost" onclick={() => (confirmSwitch = false)}
        >{m.common_cancel()}</Button
      >
    {:else}
      <Button size="sm" variant="outline" onclick={() => void startHere()}
        >{m.time_track_task()}</Button
      >
    {/if}
  </div>
  {#if error !== undefined}
    <p role="alert" class="text-sm text-destructive">{error}</p>
  {/if}
</section>
