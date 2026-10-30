<script lang="ts">
  import { onMount } from 'svelte';
  import { formatInstant } from '$lib/display';
  import { roleLabel } from '$lib/features/members/roles';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';
  import { organizationsApi } from './api';
  import type { OrganizationMember } from './model';
  import { errorText } from '$lib/api/errors';

  const { organizationId, isAdmin }: { organizationId: string; isAdmin: boolean } = $props();
  let members = $state<OrganizationMember[]>();
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
      const list = await organizationsApi.members(organizationId);
      if (!disposed) members = list;
    } catch (e: unknown) {
      if (!disposed) error = errorText(e, m.organizations_load_failed());
    }
  }
</script>

<p role="alert" class="text-sm text-destructive empty:hidden">{error}</p>
{#if error !== ''}<Button variant="outline" onclick={() => void load()}>{m.common_retry()}</Button
  >{/if}
{#if members === undefined && error === ''}
  <div class="animate-pulse space-y-3" aria-label={m.app_loading()} aria-busy="true">
    {#each [1, 2, 3] as n (n)}<div class="h-16 rounded bg-muted"></div>{/each}
  </div>
{:else if members !== undefined}
  {#if members.length === 0}<p>{m.organizations_members_empty()}</p>{/if}
  <ul class="divide-y" aria-label={m.organizations_members()}>
    {#each members as member (member.user_id)}
      <li class="flex min-w-0 flex-wrap items-center gap-3 py-3">
        <div class="min-w-0 flex-1 basis-48 wrap-anywhere">
          <p class="font-medium">{member.display_name}</p>
          {#if isAdmin && member.email}<p class="text-sm text-muted-foreground">
              {member.email}
            </p>{/if}
        </div>
        <span class="rounded border px-2 py-1 text-sm">{roleLabel(member.role)}</span>
        <span class="text-sm text-muted-foreground"
          >{m.organizations_joined({ when: formatInstant(member.created_at) })}</span
        >
      </li>
    {/each}
  </ul>
{/if}
