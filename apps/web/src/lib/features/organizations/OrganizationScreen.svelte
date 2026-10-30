<script lang="ts">
  import type { Organization } from '@fvoci/api-client';
  import { onDestroy, untrack } from 'svelte';
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
  import { organizationsApi } from './api';
  import type { OrganizationMember } from './model';

  interface Props {
    organizationId: string;
    tab: OrganizationTab;
    meId: string;
    onchanged: () => void;
  }
  const { organizationId, tab, meId, onchanged }: Props = $props();
  let organization = $state<Organization>();
  let members = $state<OrganizationMember[]>();
  let memberError = $state('');
  const memberCount = $derived(members?.length);
  let error = $state('');
  let unavailable = $state(false);
  let disposed = false;
  let generation = 0;
  let memberGeneration = 0;
  onDestroy(() => {
    disposed = true;
  });
  const loadKey = $derived(JSON.stringify([organizationId, meId]));
  $effect(() => {
    if (loadKey !== '')
      untrack(() => {
        organization = undefined;
        members = undefined;
        void load();
      });
  });
  async function load(refreshMembers = true): Promise<void> {
    const sequence = ++generation;
    const id = organizationId;
    const user = meId;
    const current = (): boolean =>
      !disposed &&
      sequence === generation &&
      organizationId === id &&
      meId === user &&
      !unavailable;
    error = '';
    unavailable = false;
    if (refreshMembers) void loadMembers();
    try {
      const found = await api.organization(id);
      if (!current()) return;
      organization = found;
    } catch (e: unknown) {
      if (!current()) return;
      unavailable = e instanceof ApiError && (e.status === 403 || e.status === 404);
      error = unavailable
        ? m.organizations_unavailable()
        : errorText(e, m.organizations_load_failed());
    }
  }
  async function loadMembers(): Promise<void> {
    const sequence = ++memberGeneration;
    const id = organizationId;
    const user = meId;
    const current = (): boolean =>
      !disposed && sequence === memberGeneration && organizationId === id && meId === user;
    memberError = '';
    try {
      const list = await organizationsApi.members(id);
      if (current()) members = list;
    } catch (e: unknown) {
      if (!current()) return;
      members = undefined;
      if (e instanceof ApiError && (e.status === 403 || e.status === 404)) noAccess();
      else memberError = errorText(e, m.organizations_load_failed());
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
    if (!unavailable) void load(false);
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
        {members}
        loadError={memberError}
        onreload={loadMembers}
        onchanged={changed}
        onunavailable={noAccess}
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
