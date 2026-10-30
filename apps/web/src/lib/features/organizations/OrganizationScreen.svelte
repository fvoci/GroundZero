<script lang="ts">
  import type { Organization } from '@fvoci/api-client';
  import { onMount } from 'svelte';
  import { api, ApiError } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import ManagementFrame from '$lib/features/common/ManagementFrame.svelte';
  import { m } from '$lib/paraglide/messages.js';
  import { organizationPath, router, type OrganizationTab } from '$lib/router.svelte';
  import { Button } from '$lib/ui/button';
  import OrganizationMembers from './OrganizationMembers.svelte';
  import OrganizationInvitations from './OrganizationInvitations.svelte';
  import OrganizationWorkspaces from './OrganizationWorkspaces.svelte';
  import OrganizationSettings from './OrganizationSettings.svelte';
  import { org } from '$lib/org.svelte';

  interface Props {
    organizationId: string;
    tab: OrganizationTab;
    meId: string;
    onchanged: () => void;
  }
  const { organizationId, tab, meId, onchanged }: Props = $props();
  let organization = $state<Organization>();
  let memberCount = $state<number>();
  let error = $state('');
  let unavailable = $state(false);
  let disposed = false;
  onMount(() => {
    void load();
    return () => {
      disposed = true;
    };
  });
  async function load(): Promise<void> {
    error = '';
    unavailable = false;
    try {
      const found = await api.organization(organizationId);
      if (!disposed) organization = found;
    } catch (e: unknown) {
      if (disposed) return;
      unavailable = e instanceof ApiError && (e.status === 403 || e.status === 404);
      error = unavailable
        ? m.organizations_unavailable()
        : errorText(e, m.organizations_load_failed());
    }
  }
  const isAdmin = $derived(organization?.role === 'admin');
  const tabs = $derived([
    {
      value: 'members',
      label:
        memberCount === undefined
          ? m.organizations_members()
          : m.organizations_member_count({ count: memberCount }),
    },
    ...(isAdmin
      ? [
          { value: 'invitations', label: m.organizations_invitations() },
          { value: 'workspaces', label: m.organizations_workspaces() },
          { value: 'settings', label: m.organizations_settings() },
        ]
      : []),
  ]);
  $effect(() => {
    if (organization === undefined) return;
    if (
      (!isAdmin && tab !== 'members') ||
      router.path === `/o/${organizationId}` ||
      router.path === `/o/${organizationId}/`
    )
      router.replace(organizationPath(organizationId));
  });
  function change(value: string): void {
    if (
      value === 'members' ||
      value === 'invitations' ||
      value === 'workspaces' ||
      value === 'settings'
    )
      router.navigate(organizationPath(organizationId, value));
  }
  function noAccess(): void {
    unavailable = true;
    error = m.organizations_unavailable();
  }
  function changed(): void {
    onchanged();
    void load();
  }
  function settingsChanged(saved: Organization): void {
    organization = saved;
    org.organizations = org.organizations.map((current) =>
      current.id === saved.id ? saved : current,
    );
    onchanged();
  }
</script>

<p role="alert" class="text-destructive empty:hidden">{error}</p>
{#if unavailable}
  <Button
    href="/"
    onclick={(event: MouseEvent) => {
      event.preventDefault();
      router.navigate('/');
    }}>{m.organizations_go_home()}</Button
  >
{:else if error !== ''}
  <Button variant="outline" onclick={() => void load()}>{m.common_retry()}</Button>
{:else if organization === undefined}
  <div class="animate-pulse space-y-4" aria-label={m.app_loading()} aria-busy="true">
    <div class="h-8 w-1/2 rounded bg-muted"></div>
    <div class="h-48 rounded bg-muted"></div>
  </div>
{:else}
  <ManagementFrame
    title={organization.name}
    {tabs}
    value={isAdmin ? tab : 'members'}
    onchange={change}
  >
    {#if tab === 'members' || !isAdmin}
      <OrganizationMembers
        {organizationId}
        {isAdmin}
        {meId}
        onchanged={changed}
        onunavailable={noAccess}
        oncountchange={(count: number) => {
          memberCount = count;
        }}
      />
    {:else if tab === 'invitations'}
      <OrganizationInvitations {organizationId} onunavailable={noAccess} />
    {:else if tab === 'workspaces'}
      <OrganizationWorkspaces {organizationId} {meId} {onchanged} onunavailable={noAccess} />
    {:else}
      <OrganizationSettings {organization} onchanged={settingsChanged} onunavailable={noAccess} />
    {/if}
  </ManagementFrame>
{/if}
