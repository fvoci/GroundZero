<script lang="ts">
  import type { ItemTimeView } from '$lib/api/client';
  import { api } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';
  import { durationText, TIME_KIND_LABEL } from './format';
  import { timer } from './timer.svelte';

  // 항목에서는 시작하지 않고 본인 합계만 본다(D-117).
  interface Props {
    itemId: string;
  }
  const { itemId }: Props = $props();
  const headingId = $props.id();
  let time = $state<ItemTimeView | undefined>(undefined);
  let loadError = $state<string | undefined>(undefined);
  let readSeq = 0;
  async function reload(id: string = itemId): Promise<void> {
    const mine = ++readSeq;
    loadError = undefined;
    try {
      const next = await api.itemTime(id);
      if (mine === readSeq) time = next;
    } catch (e: unknown) {
      if (mine === readSeq) loadError = errorText(e, m.time_load_failed());
    }
  }
  // 전환도 이전 기록을 저장하므로 타이머 id·판이 바뀔 때 합계를 다시 읽는다.
  const request = $derived({ id: itemId, timerId: timer.entry?.id, version: timer.entry?.version });
  $effect(() => {
    const { id } = request;
    time = undefined;
    void reload(id);
    return () => {
      readSeq += 1;
    };
  });
  const mineTotal = $derived(time?.kinds.reduce((s, k) => s + k.mine_seconds, 0) ?? 0);
</script>

<section class="flex flex-col gap-2" aria-labelledby={headingId}>
  <h2 id={headingId} class="text-lg font-semibold">{m.time_heading()}</h2>
  <span role="status" class="sr-only"
    >{time === undefined && loadError === undefined ? m.app_loading() : ''}</span
  >
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
        <thead class="text-left text-muted-foreground"
          ><tr
            ><th class="pr-4 font-medium">{m.time_kind()}</th><th class="font-medium"
              >{m.time_my_time()}</th
            ></tr
          ></thead
        >
        <tbody
          >{#each time.kinds as k (k.kind)}<tr class="border-t"
              ><td class="pr-4">{TIME_KIND_LABEL[k.kind]}</td><td>{durationText(k.mine_seconds)}</td
              ></tr
            >{/each}</tbody
        >
      </table>
    {:else}<p class="text-sm text-muted-foreground">{m.time_item_empty()}</p>{/if}
  {/if}
  {#if loadError !== undefined}
    <p role="alert" class="text-sm text-destructive">
      {loadError}<Button size="sm" variant="outline" class="ml-2" onclick={() => void reload()}
        >{m.common_retry()}</Button
      >
    </p>
  {/if}
</section>
