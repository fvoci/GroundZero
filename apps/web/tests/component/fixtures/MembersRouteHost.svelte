<script lang="ts">
  import type { ComponentProps } from 'svelte';
  import MembersScreen from '$lib/features/members/MembersScreen.svelte';
  import { router } from '$lib/router.svelte';

  const props: Omit<ComponentProps<typeof MembersScreen>, 'tab'> = $props();
  const route = $derived(router.route);
</script>

{#if route.name === 'members'}
  <MembersScreen {...props} tab={route.tab} />
{/if}
