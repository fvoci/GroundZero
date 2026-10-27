<script lang="ts">
  // 업무 행·카드 안의 시간 기록 시작·멈춤(D-117). 목록마다 합계를 읽지 않으려고 서버 조회 없이 타이머 상태만 본다.
  // 합계와 상세 조작은 업무 상세의 TaskTimePanel이 맡는다. 다른 대상에서 시작하면 서버가 이전 기록을 저장하고 새로
  // 시작하며 timer 저장소가 "이전 기록 저장됨"을 알린다.
  import PlayIcon from '@lucide/svelte/icons/play';
  import SquareIcon from '@lucide/svelte/icons/square';

  import { errorText } from '$lib/api/errors';
  import { notifyDone } from '$lib/features/feedback/feedback.svelte';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';

  import { durationText } from './format';
  import { timer } from './timer.svelte';

  interface Props {
    taskId: string;
    /** 접근성 이름에 넣을 업무 제목(목록에는 같은 단추가 여럿이다). */
    title: string;
  }

  const { taskId, title }: Props = $props();

  let busy = $state(false);
  let error = $state<string | undefined>(undefined);

  const runningHere = $derived(
    timer.entry !== null && timer.entry.target?.kind === 'task' && timer.entry.target.id === taskId,
  );

  async function toggle(): Promise<void> {
    if (busy) return;
    busy = true;
    error = undefined;
    try {
      if (runningHere) {
        const stopped = await timer.act('stop');
        if (stopped !== undefined)
          notifyDone(m.time_timer_stopped({ duration: durationText(stopped.seconds) }));
      } else {
        await timer.start('work', { kind: 'task', id: taskId });
      }
    } catch (e: unknown) {
      error = errorText(e, runningHere ? m.time_stop_failed() : m.time_start_failed());
    } finally {
      busy = false;
    }
  }

  /** 카드의 끌기 시작과 열기 클릭이 이 단추까지 번지지 않게 한다. */
  function isolate(e: Event): void {
    e.stopPropagation();
  }
</script>

<!-- 행·카드 안이라 글자 대신 아이콘만 둔다(제목 칸을 밀어내지 않게). 이름은 aria-label과 title로 준다. -->
<span class="inline-flex shrink-0 items-center gap-1">
  <Button
    size="icon-sm"
    variant="ghost"
    disabled={busy}
    aria-label={`${runningHere ? m.time_stop_timer() : m.time_track_task()}: ${title}`}
    title={runningHere ? m.time_stop_timer() : m.time_track_task()}
    onpointerdown={isolate}
    onclick={(e: MouseEvent) => {
      isolate(e);
      void toggle();
    }}
  >
    {#if runningHere}<SquareIcon aria-hidden="true" />{:else}<PlayIcon aria-hidden="true" />{/if}
  </Button>
  {#if error !== undefined}
    <span role="alert" class="text-xs text-destructive">{error}</span>
  {/if}
</span>
