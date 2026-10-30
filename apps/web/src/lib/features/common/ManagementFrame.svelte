<script lang="ts">
  import type { Snippet } from 'svelte';
  import * as Tabs from '$lib/ui/tabs';

  interface Props {
    title: string;
    tabs: { value: string; label: string }[];
    value: string;
    onchange: (value: string) => void;
    children: Snippet;
  }
  const { title, tabs, value, onchange, children }: Props = $props();
  let scroller = $state<HTMLDivElement>();
  let tabList = $state<HTMLDivElement | null>(null);
  let overflowLeft = $state(false);
  let overflowRight = $state(false);

  function updateOverflow(): void {
    if (scroller === undefined) return;
    overflowLeft = scroller.scrollLeft > 1;
    overflowRight = scroller.scrollLeft + scroller.clientWidth < scroller.scrollWidth - 1;
  }

  $effect(() => {
    if (scroller === undefined || tabList === null || typeof ResizeObserver === 'undefined') return;
    const observer = new ResizeObserver(updateOverflow);
    observer.observe(scroller);
    observer.observe(tabList);
    updateOverflow();
    return () => {
      observer.disconnect();
    };
  });
</script>

<section class="flex min-w-0 flex-col gap-5" data-testid="management-screen">
  <h1 class="text-2xl font-semibold wrap-anywhere">{title}</h1>
  <Tabs.Root bind:value={() => value, onchange} activationMode="manual" class="min-w-0">
    <div class="relative min-w-0">
      <div
        bind:this={scroller}
        class="max-w-full overflow-x-auto"
        data-testid="management-tabs-scroll"
        onscroll={updateOverflow}
      >
        <Tabs.List
          bind:ref={tabList}
          aria-label={title}
          class="flex w-max min-w-full gap-1 border-b p-1"
        >
          {#each tabs as tab (tab.value)}
            <Tabs.Trigger
              value={tab.value}
              class="shrink-0 rounded-md px-3 py-2 text-sm font-medium whitespace-nowrap outline-none hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring data-[state=active]:bg-muted"
              >{tab.label}</Tabs.Trigger
            >
          {/each}
        </Tabs.List>
      </div>
      {#if overflowLeft}
        <div
          aria-hidden="true"
          data-testid="management-tabs-fade-left"
          class="pointer-events-none absolute inset-y-0 left-0 w-6 bg-linear-to-r from-muted to-transparent"
        ></div>
      {/if}
      {#if overflowRight}
        <div
          aria-hidden="true"
          data-testid="management-tabs-fade-right"
          class="pointer-events-none absolute inset-y-0 right-0 w-6 bg-linear-to-l from-muted to-transparent"
        ></div>
      {/if}
    </div>
    <Tabs.Content
      {value}
      class="min-w-0 pt-5 outline-none focus-visible:ring-2 focus-visible:ring-ring"
    >
      {@render children()}
    </Tabs.Content>
  </Tabs.Root>
</section>
