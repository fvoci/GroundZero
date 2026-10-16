<script lang="ts">
  // 항목의 일정(회의·발표·행사, D-31): 목록, 추가, 상세 대화상자.
  import type { EventInput, EventView, ItemView } from '$lib/api/client';
  import { api, ApiError, newRequestId } from '$lib/api/client';
  import { Button } from '$lib/ui/button';

  import EventDialog from './EventDialog.svelte';
  import EventForm from './EventForm.svelte';
  import { describeWhen, KIND_LABEL } from './labels';
  import { localTimeZone } from './time';

  interface Props {
    item: ItemView;
  }

  const { item }: Props = $props();

  let events = $state<EventView[]>([]);
  let error = $state<string | undefined>(undefined);
  let adding = $state(false);
  let openId = $state<string | undefined>(undefined);
  // 같은 입력의 재시도는 같은 요청 id로(중복 생성 방지). 폼이 새로 열리면 새 id.
  let requestId = newRequestId();
  const zone = localTimeZone();
  const canWrite = $derived(item.access !== 'read' && !item.archived);

  async function reload(itemId: string = item.id): Promise<void> {
    try {
      events = await api.itemEvents(itemId);
      error = undefined;
    } catch (e: unknown) {
      error = e instanceof ApiError ? e.message : '일정을 불러오지 못했습니다.';
    }
  }

  $effect(() => {
    void reload(item.id);
  });

  async function create(input: EventInput): Promise<void> {
    await api.createEvent(item.id, requestId, input);
    requestId = newRequestId();
    adding = false;
    await reload();
  }

  let returnTo: string | undefined;
</script>

<section class="flex flex-col gap-3" aria-labelledby="events-heading">
  <div class="flex items-center gap-2">
    <h2 id="events-heading" class="text-lg font-semibold">일정</h2>
    {#if canWrite && !adding}
      <Button
        size="sm"
        variant="outline"
        class="ml-auto"
        onclick={() => {
          requestId = newRequestId();
          adding = true;
        }}>일정 추가</Button
      >
    {/if}
  </div>
  {#if error !== undefined}
    <p role="alert" class="text-sm text-destructive">{error}</p>
  {/if}
  {#if adding}
    <div class="rounded-md border p-3">
      <EventForm
        idPrefix="event-new"
        initial={undefined}
        submitLabel="일정 만들기"
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
      <li class="text-sm text-muted-foreground">일정이 없습니다.</li>
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
  onchanged={() => void reload()}
/>
