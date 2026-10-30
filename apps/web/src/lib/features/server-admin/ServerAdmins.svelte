<script lang="ts">
  import { onMount } from 'svelte';
  import { formatInstant } from '$lib/display';
  import { organizationError, organizationUnavailable } from '$lib/features/organizations/feedback';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';
  import { serverAdminApi } from './api';
  import type { ServerAdmin } from './model';

  const { onunavailable }: { onunavailable: () => void } = $props();
  let admins = $state<ServerAdmin[]>();
  let error = $state('');
  let disposed = false;
  onMount(() => {
    void load();
    return () => {
      disposed = true;
    };
  });
  async function load(): Promise<void> {
    error = '';
    try {
      const list = await serverAdminApi.admins();
      if (!disposed) admins = list;
    } catch (e: unknown) {
      if (disposed) return;
      if (organizationUnavailable(e)) onunavailable();
      else error = organizationError(e);
    }
  }
</script>

<p role="alert" class="text-sm text-destructive empty:hidden">{error}</p>
{#if error !== ''}
  <Button variant="outline" onclick={() => void load()}>{m.common_retry()}</Button>
{:else if admins === undefined}
  <div class="animate-pulse space-y-3" aria-label={m.app_loading()} aria-busy="true">
    {#each [1, 2, 3] as n (n)}<div class="h-16 rounded bg-muted"></div>{/each}
  </div>
{:else if admins.length === 0}
  <p>{m.organizations_server_admins_empty()}</p>
{:else}
  <ul class="divide-y" aria-label={m.organizations_server_admins()}>
    {#each admins as admin (admin.user_id)}
      <li class="min-w-0 space-y-2 py-4 wrap-anywhere">
        <p class="font-medium">{admin.display_name}</p>
        <p class="text-sm text-muted-foreground">
          {m.organizations_appointed({ when: formatInstant(admin.created_at) })}
        </p>
      </li>
    {/each}
  </ul>
{/if}
