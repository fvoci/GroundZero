<script lang="ts">
  import * as Collapsible from '$lib/ui/collapsible';
  import { Button } from '$lib/ui/button';
  import { m } from '$lib/paraglide/messages.js';
  import ActivityList from './ActivityList.svelte';
  const { itemId, workspaceId }: { itemId: string; workspaceId: string } = $props();
</script>

<section class="flex flex-col gap-3" aria-labelledby={`item-activity-${itemId}`}>
  <Collapsible.Root>
    <h2 id={`item-activity-${itemId}`} class="text-lg font-semibold">
      <Collapsible.Trigger
        >{#snippet child({ props })}<Button {...props} variant="outline"
            >{m.home_activity_tab()}</Button
          >{/snippet}</Collapsible.Trigger
      >
    </h2>
    <Collapsible.Content
      ><ActivityList kind="item" targetId={itemId} {workspaceId} /></Collapsible.Content
    >
  </Collapsible.Root>
</section>
