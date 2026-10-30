<script lang="ts">
  import ManagementFrame from '$lib/features/common/ManagementFrame.svelte';
  import { m } from '$lib/paraglide/messages.js';
  import { router, serverAdminPath, type ServerAdminTab } from '$lib/router.svelte';
  import { Button } from '$lib/ui/button';
  import ServerOrganizations from './ServerOrganizations.svelte';
  import ServerAdmins from './ServerAdmins.svelte';

  interface Props {
    isServerAdmin: boolean;
    meId: string;
    tab: ServerAdminTab;
    oncreated: () => void;
  }
  const { isServerAdmin, meId, tab, oncreated }: Props = $props();
  let unavailable = $state(false);
  const tabs = $derived([
    { value: 'organizations', label: m.organizations_server_organizations() },
    { value: 'admins', label: m.organizations_server_admins() },
  ]);
  $effect(() => {
    if (router.path === '/admin' || router.path === '/admin/') router.replace(serverAdminPath());
  });
  function change(value: string): void {
    if (value === 'organizations' || value === 'admins') router.navigate(serverAdminPath(value));
  }
  function noAccess(): void {
    unavailable = true;
  }
</script>

<p role="alert" class="text-destructive empty:hidden">
  {unavailable || !isServerAdmin ? m.organizations_unavailable() : ''}
</p>
{#if unavailable || !isServerAdmin}
  <Button
    href="/"
    onclick={(event: MouseEvent) => {
      event.preventDefault();
      router.navigate('/');
    }}>{m.organizations_go_home()}</Button
  >
{:else}
  <ManagementFrame title={m.organizations_server_manage()} {tabs} value={tab} onchange={change}>
    {#if tab === 'organizations'}
      <ServerOrganizations {meId} {oncreated} onunavailable={noAccess} />
    {:else}
      <ServerAdmins onunavailable={noAccess} />
    {/if}
  </ManagementFrame>
{/if}
