<script lang="ts">
  // 항목의 시간 합계(D-78·D-32·D-37): 항목과 그 아래 업무·문서·강의 회차·스터디 단위의 기록을 종류별로.
  // 내 시간은 내 기록만, 공유된 합계는 이 워크스페이스에서 공유를 켠 사람들의 것만 보인다. 강의 시청과 순공부는
  // 종류가 달라 따로 보인다. 이 항목으로 타이머를 시작할 수 있다(종류는 모듈에 맞춤).
  import type { ModuleKind } from '@fvoci/api-client';

  import type { ItemTimeView, TimeKind } from '$lib/api/client';
  import { api } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';

  import { durationText, TIME_KIND_LABEL } from './format';
  import { timer } from './timer.svelte';

  interface Props {
    itemId: string;
    moduleKind: ModuleKind;
    canTrack: boolean;
  }

  const { itemId, moduleKind, canTrack }: Props = $props();

  /** 항목에서 바로 시작하는 타이머의 종류. 강의 회차·스터디 단위의 "측정"은 각 패널이 정한다. */
  const START_KIND: Record<ModuleKind, TimeKind> = {
    project: 'work',
    research: 'work',
    study: 'study',
    course: 'study',
    hobby: 'hobby',
  };

  let time = $state<ItemTimeView | undefined>(undefined);
  let error = $state<string | undefined>(undefined);
  let confirmSwitch = $state(false);

  async function reload(id: string = itemId): Promise<void> {
    try {
      time = await api.itemTime(id);
    } catch {
      time = undefined;
    }
  }

  $effect(() => {
    void reload(itemId);
  });

  // 이 항목 아래 무엇이든 타이머가 끝나면 합계를 다시 읽는다.
  const running = $derived(timer.entry !== null);
  let wasRunning = false;
  $effect(() => {
    const now = running;
    if (wasRunning && !now) void reload();
    wasRunning = now;
  });

  const runningHere = $derived(
    timer.entry !== null && timer.entry.target?.kind === 'item' && timer.entry.target.id === itemId,
  );

  async function startHere(): Promise<void> {
    error = undefined;
    try {
      if (timer.entry !== null) {
        if (!confirmSwitch) {
          confirmSwitch = true;
          return;
        }
        await timer.act('stop');
      }
      confirmSwitch = false;
      await timer.start(START_KIND[moduleKind], { kind: 'item', id: itemId });
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

  const mineTotal = $derived(
    time === undefined ? 0 : time.kinds.reduce((s, k) => s + k.mine_seconds, 0),
  );
</script>

<section class="flex flex-col gap-2" aria-labelledby="item-time-heading">
  <h2 id="item-time-heading" class="text-lg font-semibold">{m.time_heading()}</h2>
  {#if time !== undefined}
    <p class="text-sm">
      {m.time_item_summary({
        estimate:
          time.estimate_minutes > 0 ? durationText(time.estimate_minutes * 60) : m.time_none(),
        mine: durationText(mineTotal),
      })}
    </p>
    {#if time.kinds.length > 0}
      <table class="text-sm" data-testid="item-time">
        <thead class="text-left text-muted-foreground">
          <tr
            ><th class="pr-4 font-medium">{m.time_kind()}</th><th class="pr-4 font-medium"
              >{m.time_my_time()}</th
            ><th class="font-medium">{m.time_shared_total()}</th></tr
          >
        </thead>
        <tbody>
          {#each time.kinds as k (k.kind)}
            <tr class="border-t">
              <td class="pr-4">{TIME_KIND_LABEL[k.kind]}</td>
              <td class="pr-4">{durationText(k.mine_seconds)}</td>
              <td
                >{k.shared_people > 0
                  ? m.time_with_people({
                      total: durationText(k.shared_seconds),
                      count: k.shared_people,
                    })
                  : '—'}</td
              >
            </tr>
          {/each}
        </tbody>
      </table>
    {:else}
      <p class="text-sm text-muted-foreground">{m.time_item_empty()}</p>
    {/if}
  {/if}
  {#if canTrack}
    <div class="flex flex-wrap items-center gap-2">
      {#if runningHere}
        <span class="text-sm" role="status">{m.time_item_running()}</span>
        <Button size="sm" variant="outline" onclick={() => void stopHere()}
          >{m.time_stop_timer()}</Button
        >
      {:else if confirmSwitch}
        <span class="text-sm" role="status">{m.time_item_switch_confirm()}</span>
        <Button size="sm" onclick={() => void startHere()}>{m.time_stop_and_start()}</Button>
        <Button size="sm" variant="ghost" onclick={() => (confirmSwitch = false)}
          >{m.common_cancel()}</Button
        >
      {:else}
        <Button size="sm" variant="outline" onclick={() => void startHere()}
          >{m.time_track_item({ kind: TIME_KIND_LABEL[START_KIND[moduleKind]] })}</Button
        >
      {/if}
    </div>
  {/if}
  {#if error !== undefined}
    <p role="alert" class="text-sm text-destructive">{error}</p>
  {/if}
</section>
