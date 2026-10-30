<script lang="ts">
  import { onMount, tick } from 'svelte';
  import EllipsisIcon from '@lucide/svelte/icons/ellipsis';
  import { formatInstant } from '$lib/display';
  import { roleLabel } from '$lib/features/members/roles';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';
  import * as Dialog from '$lib/ui/dialog';
  import * as DropdownMenu from '$lib/ui/dropdown-menu';
  import { organizationsApi } from './api';
  import type { OrganizationMember, OrganizationRemovalPreview } from './model';
  import { errorText } from '$lib/api/errors';
  import { organizationError, organizationUnavailable } from './feedback';
  import { notifyDone } from '$lib/features/feedback/feedback.svelte';

  interface Props {
    organizationId: string;
    isAdmin: boolean;
    meId: string;
    onchanged: () => void;
    onunavailable: () => void;
    oncountchange: (count: number) => void;
  }
  const { organizationId, isAdmin, meId, onchanged, onunavailable, oncountchange }: Props =
    $props();
  let members = $state<OrganizationMember[]>();
  let filter = $state('');
  const query = $derived(filter.trim().toLocaleLowerCase());
  const filteredMembers = $derived(
    members?.filter(
      (member) =>
        member.display_name.toLocaleLowerCase().includes(query) ||
        (isAdmin && (member.email?.toLocaleLowerCase().includes(query) ?? false)),
    ) ?? [],
  );
  let error = $state('');
  let busy = $state(false);
  let confirm = $state<{ kind: 'demote' | 'remove'; member: OrganizationMember }>();
  let preview = $state<OrganizationRemovalPreview>();
  let previewError = $state('');
  let dialogError = $state('');
  let returnFocus: HTMLElement | null = null;
  let previewSeq = 0;
  let disposed = false;
  const current = (): boolean => !disposed;
  const adminCount = $derived(members?.filter((member) => member.role === 'admin').length ?? 0);
  const lastAdmin = (member: OrganizationMember): boolean =>
    member.role === 'admin' && adminCount === 1;
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
      if (!disposed) {
        members = list;
        if (list.length <= 10) filter = '';
        oncountchange(list.length);
      }
    } catch (e: unknown) {
      if (disposed) return;
      if (organizationUnavailable(e)) onunavailable();
      else error = errorText(e, m.organizations_load_failed());
    }
  }

  async function setRole(member: OrganizationMember): Promise<void> {
    if (busy || !isAdmin || lastAdmin(member)) return;
    if (member.role === 'admin') {
      confirm = { kind: 'demote', member };
      dialogError = '';
      return;
    }
    await run(() => organizationsApi.setRole(organizationId, member.user_id, 'admin'));
  }

  async function readPreview(): Promise<void> {
    if (confirm?.kind !== 'remove') return;
    const userId = confirm.member.user_id;
    const seq = ++previewSeq;
    previewError = '';
    preview = undefined;
    try {
      const found = await organizationsApi.removalPreview(organizationId, userId);
      if (!disposed && seq === previewSeq) preview = found;
    } catch (e: unknown) {
      if (!disposed && seq === previewSeq) {
        if (organizationUnavailable(e)) onunavailable();
        else previewError = organizationError(e);
      }
    }
  }

  function askRemove(member: OrganizationMember): void {
    if (busy || !isAdmin || lastAdmin(member)) return;
    confirm = { kind: 'remove', member };
    dialogError = '';
    void readPreview();
  }

  function closeConfirm(): void {
    confirm = undefined;
    preview = undefined;
    previewError = '';
    dialogError = '';
    previewSeq += 1;
  }

  function removalText(name: string, impact: OrganizationRemovalPreview): string {
    const parts = [
      impact.workspaces > 0
        ? impact.shares > 0 || impact.assignments > 0
          ? m.organizations_removal_workspaces({ count: impact.workspaces })
          : m.organizations_removal_workspaces_end({ count: impact.workspaces })
        : '',
      impact.shares > 0
        ? impact.assignments > 0
          ? m.organizations_removal_shares({ count: impact.shares })
          : m.organizations_removal_shares_end({ count: impact.shares })
        : '',
      impact.assignments > 0
        ? m.organizations_removal_assignments({ count: impact.assignments })
        : '',
    ].filter((part) => part !== '');
    return parts.length === 0
      ? m.organizations_removal_none()
      : m.organizations_removal_body({ name, impact: parts.join(', ') });
  }

  async function run(action: () => Promise<void>): Promise<void> {
    if (busy || !isAdmin) return;
    busy = true;
    error = '';
    dialogError = '';
    try {
      await action();
      if (disposed) return;
      closeConfirm();
      notifyDone(m.organizations_member_updated());
      await load();
      if (current()) {
        onchanged();
        busy = false;
        await tick();
        if (returnFocus?.isConnected) returnFocus.focus();
        else document.getElementById('organization-members-heading')?.focus();
      }
    } catch (e: unknown) {
      if (disposed) return;
      if (organizationUnavailable(e)) onunavailable();
      else if (confirm !== undefined) dialogError = organizationError(e);
      else error = organizationError(e);
    } finally {
      if (!disposed) busy = false;
    }
  }

  function confirmAction(): void {
    const pending = confirm;
    if (pending === undefined || (pending.kind === 'remove' && preview === undefined)) return;
    void run(() =>
      pending.kind === 'remove'
        ? organizationsApi.removeMember(organizationId, pending.member.user_id)
        : organizationsApi.setRole(organizationId, pending.member.user_id, 'member'),
    );
  }
</script>

<h2 id="organization-members-heading" tabindex="-1" class="font-semibold">
  {members === undefined
    ? m.organizations_members()
    : m.organizations_member_count({ count: members.length })}
</h2>
<p role="alert" class="text-sm text-destructive empty:hidden">{error}</p>
{#if error !== ''}<Button variant="outline" onclick={() => void load()}>{m.common_retry()}</Button
  >{/if}
{#if members === undefined && error === ''}
  <div class="animate-pulse space-y-3" aria-label={m.app_loading()} aria-busy="true">
    {#each [1, 2, 3] as n (n)}<div class="h-16 rounded bg-muted"></div>{/each}
  </div>
{:else if members !== undefined}
  {#if members.length > 10}
    <div class="flex min-w-0 flex-col gap-1">
      <Label for="organization-member-filter"
        >{isAdmin ? m.members_filter() : m.members_filter_name()}</Label
      >
      <Input id="organization-member-filter" type="search" bind:value={filter} />
    </div>
  {/if}
  {#if members.length === 0}<p>{m.organizations_members_empty()}</p>{/if}
  <p role="status" class="text-sm text-muted-foreground empty:hidden">
    {members.length > 0 && filteredMembers.length === 0 ? m.members_filter_empty() : ''}
  </p>
  <ul class="divide-y" aria-label={m.organizations_members()}>
    {#each filteredMembers as member (member.user_id)}
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
        {#if isAdmin}
          <div>
            <DropdownMenu.Root
              onOpenChange={(open) => {
                if (open)
                  returnFocus = document.getElementById(`org-member-menu-${member.user_id}`);
              }}
            >
              <DropdownMenu.Trigger>
                {#snippet child({ props })}
                  <Button
                    {...props}
                    id={`org-member-menu-${member.user_id}`}
                    variant="outline"
                    size="icon"
                    disabled={busy}
                    aria-label={m.organizations_member_actions({ name: member.display_name })}
                    ><EllipsisIcon class="size-4" aria-hidden="true" /></Button
                  >
                {/snippet}
              </DropdownMenu.Trigger>
              <DropdownMenu.Content>
                <DropdownMenu.Item
                  disabled={busy || lastAdmin(member)}
                  aria-describedby={lastAdmin(member) ? `last-admin-${member.user_id}` : undefined}
                  onSelect={() => void setRole(member)}
                >
                  {member.role === 'admin' ? m.organizations_demote() : m.organizations_promote()}
                </DropdownMenu.Item>
                <DropdownMenu.Item
                  disabled={busy || lastAdmin(member)}
                  aria-describedby={lastAdmin(member) ? `last-admin-${member.user_id}` : undefined}
                  onSelect={() => {
                    askRemove(member);
                  }}>{m.organizations_remove()}</DropdownMenu.Item
                >
              </DropdownMenu.Content>
            </DropdownMenu.Root>
            {#if lastAdmin(member)}<p
                id={`last-admin-${member.user_id}`}
                class="mt-1 text-xs text-muted-foreground"
              >
                {m.organizations_last_admin()}
              </p>{/if}
          </div>
        {/if}
      </li>
    {/each}
  </ul>
{/if}

<Dialog.Root
  open={confirm !== undefined}
  onOpenChange={(open) => {
    if (!open && !busy) closeConfirm();
  }}
>
  <Dialog.Content
    class="max-h-[90vh] overflow-y-auto"
    onCloseAutoFocus={(event: Event) => {
      event.preventDefault();
      if (returnFocus?.isConnected) returnFocus.focus();
      else document.getElementById('organization-members-heading')?.focus();
    }}
  >
    <Dialog.Header>
      <Dialog.Title
        >{confirm?.kind === 'demote'
          ? confirm.member.user_id === meId
            ? m.organizations_demote_question()
            : m.organizations_demote_other_question({ name: confirm.member.display_name })
          : m.organizations_remove_question({
              name: confirm?.member.display_name ?? '',
            })}</Dialog.Title
      >
      <Dialog.Description>
        {#if confirm?.kind === 'demote'}
          {confirm.member.user_id === meId
            ? m.organizations_demote_body()
            : m.organizations_demote_other_body({ name: confirm.member.display_name })}
        {:else if preview !== undefined}{removalText(confirm?.member.display_name ?? '', preview)}
        {:else}{m.app_loading()}{/if}
      </Dialog.Description>
    </Dialog.Header>
    <p role="alert" class="text-sm text-destructive empty:hidden">
      {dialogError || (confirm?.kind === 'remove' ? previewError : '')}
    </p>
    {#if confirm?.kind === 'remove' && previewError !== ''}<Button
        variant="outline"
        onclick={() => void readPreview()}>{m.common_retry()}</Button
      >{/if}
    <Dialog.Footer>
      <Button variant="ghost" disabled={busy} onclick={closeConfirm}>{m.common_cancel()}</Button>
      <Button
        variant="destructive"
        disabled={busy || (confirm?.kind === 'remove' && preview === undefined)}
        onclick={confirmAction}
        >{confirm?.kind === 'demote' ? m.organizations_demote() : m.organizations_remove()}</Button
      >
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
