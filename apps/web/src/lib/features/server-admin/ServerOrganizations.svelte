<script lang="ts">
  import { onMount } from 'svelte';
  import { formatInstant } from '$lib/display';
  import { organizationError, organizationUnavailable } from '$lib/features/organizations/feedback';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';
  import { serverAdminApi } from './api';
  import CreateOrganizationDialog from './CreateOrganizationDialog.svelte';
  import type { ServerOrganization } from './model';

  const {
    meId,
    oncreated,
    onunavailable,
  }: { meId: string; oncreated: () => void; onunavailable: () => void } = $props();
  let organizations = $state<ServerOrganization[]>();
  let error = $state('');
  let disposed = false;
  let loadSequence = 0;
  onMount(() => {
    void load();
    return () => {
      disposed = true;
    };
  });
  async function load(): Promise<void> {
    const sequence = ++loadSequence;
    error = '';
    try {
      const list = await serverAdminApi.organizations();
      if (!disposed && sequence === loadSequence) organizations = list;
    } catch (e: unknown) {
      if (disposed || sequence !== loadSequence) return;
      if (organizationUnavailable(e)) onunavailable();
      else error = organizationError(e);
    }
  }
  function created(): void {
    oncreated();
    void load();
  }
</script>

<CreateOrganizationDialog {meId} oncreated={created} {onunavailable} />
<p role="alert" class="mt-3 text-sm text-destructive empty:hidden">{error}</p>
{#if error !== ''}
  <Button variant="outline" onclick={() => void load()}>{m.common_retry()}</Button>
{:else if organizations === undefined}
  <div class="mt-3 animate-pulse space-y-3" aria-label={m.app_loading()} aria-busy="true">
    {#each [1, 2, 3] as n (n)}<div class="h-16 rounded bg-muted"></div>{/each}
  </div>
{:else if organizations.length === 0}
  <p class="my-3">{m.organizations_server_empty()}</p>
{:else}
  <ul class="divide-y" aria-label={m.organizations_server_organizations()}>
    {#each organizations as organization (organization.id)}
      <li class="min-w-0 space-y-2 py-4 wrap-anywhere">
        <p class="font-medium">{organization.name}</p>
        <div class="flex flex-wrap gap-x-4 gap-y-2 text-sm text-muted-foreground">
          <span>{m.organizations_member_count({ count: organization.member_count })}</span>
          <span>{m.organizations_workspace_count({ count: organization.workspace_count })}</span>
          <span>{m.organizations_created({ when: formatInstant(organization.created_at) })}</span>
        </div>
      </li>
    {/each}
  </ul>
{/if}
