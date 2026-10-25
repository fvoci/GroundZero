<script lang="ts">
  // 항목의 일정(회의·발표·행사, D-31): 목록, 추가, 상세 대화상자.
  import { tick } from 'svelte';

  import type { EventInput, EventView, ItemView } from '$lib/api/client';
  import { api } from '$lib/api/client';
  import { RetryKey } from '$lib/api/retry-key';
  import { errorText } from '$lib/api/errors';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';

  import EventDialog from './EventDialog.svelte';
  import EventForm from './EventForm.svelte';
  import { describeWhen, KIND_LABEL } from './labels';
  import { localTimeZone } from './time';

  interface Props {
    item: ItemView;
    /** 같은 화면의 다른 곳(달력)에서 일정이 바뀔 때마다 늘어난다(다시 읽기). */
    version?: number;
    /** 여기서 일정을 만들거나 고치거나 지웠다(같은 화면의 달력이 다시 읽게). */
    onchanged?: () => void;
  }

  const { item, version = 0, onchanged }: Props = $props();

  let events = $state<EventView[]>([]);
  // 목록을 한 번이라도 받았는지. 받기 전에는 "일정이 없습니다" 대신 불러오는 중이라고 보인다.
  let loaded = $state(false);
  let error = $state<string | undefined>(undefined);
  let adding = $state(false);
  let openId = $state<string | undefined>(undefined);
  // 같은 입력의 재시도는 같은 요청 id로(중복 생성 방지), 고친 입력이나 새로 연 폼은 새 id.
  const createKey = new RetryKey();
  const zone = localTimeZone();
  const canWrite = $derived(item.access !== 'read' && !item.archived);

  // 읽기가 겹치면(달력 변경·여기서의 저장) 마지막으로 보낸 읽기의 결과만 남긴다(늦게 온 옛 결과가 새 목록을 덮지 않게).
  let readSeq = 0;

  async function reload(itemId: string = item.id): Promise<void> {
    const mine = ++readSeq;
    try {
      const next = await api.itemEvents(itemId);
      if (mine !== readSeq) return;
      events = next;
      loaded = true;
      error = undefined;
      // 지운 일정의 단추처럼 초점이 있던 요소가 사라졌으면 이 영역 제목으로 돌린다.
      await tick();
      if (changed && document.activeElement === document.body)
        document.getElementById('events-heading')?.focus();
      changed = false;
    } catch (e: unknown) {
      if (mine !== readSeq) return;
      error = errorText(e, m.events_list_load_failed());
    }
  }

  $effect(() => {
    if (version < 0) return;
    void reload(item.id);
  });

  async function create(input: EventInput): Promise<void> {
    const requestId = createKey.for({ item: item.id, input });
    await api.createEvent(item.id, requestId, input);
    createKey.done(requestId);
    adding = false;
    onchanged?.();
    await reload();
  }

  let returnTo: string | undefined;
  let changed = false;
</script>

<section class="flex flex-col gap-3" aria-labelledby="events-heading">
  <div class="flex items-center gap-2">
    <h2 id="events-heading" class="text-lg font-semibold" tabindex="-1">{m.events_heading()}</h2>
    {#if canWrite && !adding}
      <Button
        size="sm"
        variant="outline"
        class="ml-auto"
        onclick={() => {
          createKey.done();
          adding = true;
        }}>{m.events_add()}</Button
      >
    {/if}
  </div>
  {#if error !== undefined}
    <p role="alert" class="text-sm text-destructive">
      {error}
      {#if !loaded}
        <Button
          size="sm"
          variant="outline"
          class="ml-2"
          onclick={() => {
            error = undefined;
            void reload();
          }}>{m.common_retry()}</Button
        >
      {/if}
    </p>
  {/if}
  {#if adding}
    <div class="rounded-md border p-3">
      <EventForm
        idPrefix="event-new"
        initial={undefined}
        submitLabel={m.events_create()}
        onsubmit={create}
        oncancel={() => (adding = false)}
      />
    </div>
  {/if}
  <ul class="flex flex-col gap-1">
    {#each events as e (e.id)}
      <li>
        <button
          type="button"
          data-event-id={e.id}
          class="flex w-full flex-wrap items-baseline gap-x-2 rounded px-2 py-1 text-left text-sm hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
          onclick={() => {
            returnTo = e.id;
            openId = e.id;
          }}
        >
          <span class="text-xs text-muted-foreground">{KIND_LABEL[e.kind]}</span>
          <span class="font-medium">{e.title}</span>
          <span class="text-muted-foreground">{describeWhen(e, zone)}</span>
        </button>
      </li>
    {:else}
      {#if loaded}
        <li class="text-sm text-muted-foreground">{m.events_list_empty()}</li>
      {:else if error === undefined}
        <li class="text-sm text-muted-foreground">{m.app_loading()}</li>
      {/if}
    {/each}
  </ul>
</section>

<EventDialog
  eventId={openId}
  onclose={() => (openId = undefined)}
  returnFocus={() =>
    returnTo === undefined
      ? null
      : document.querySelector<HTMLElement>(`[data-event-id="${CSS.escape(returnTo)}"]`)}
  onchanged={() => {
    changed = true;
    onchanged?.();
    void reload();
  }}
/>
