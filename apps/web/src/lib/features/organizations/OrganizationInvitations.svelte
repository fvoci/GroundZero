<script lang="ts">
  import type { OrgRole, Role } from '@fvoci/api-client';
  import { onMount, tick } from 'svelte';
  import { SvelteDate } from 'svelte/reactivity';
  import { api } from '$lib/api/client';
  import { formatInstant } from '$lib/display';
  import { createFormDraft } from '$lib/features/common/form-draft.svelte';
  import DiscardChangesConfirm from '$lib/features/common/DiscardChangesConfirm.svelte';
  import FormDraftOffer from '$lib/features/common/FormDraftOffer.svelte';
  import { notifyDone } from '$lib/features/feedback/feedback.svelte';
  import RoleSelect from '$lib/features/members/RoleSelect.svelte';
  import { roleLabel } from '$lib/features/members/roles';
  import { m } from '$lib/paraglide/messages.js';
  import { ApiError } from '$lib/shared/api';
  import { isRecord } from '$lib/shared/api/parse';
  import { Button } from '$lib/ui/button';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';
  import { organizationsApi } from './api';
  import { invitationEmailError } from './fields';
  import { organizationError, organizationUnavailable } from './feedback';
  import type { OrganizationInvitation } from './model';

  const { organizationId, onunavailable }: { organizationId: string; onunavailable: () => void } =
    $props();
  let invitations = $state<OrganizationInvitation[]>();
  let email = $state('');
  let role = $state<OrgRole>('member');
  let error = $state('');
  let loadError = $state('');
  let emailError = $state('');
  let roleError = $state('');
  let busy = $state(false);
  let revoking = $state<string>();
  let disposed = false;
  const current = (): boolean => !disposed;
  let seq = 0;
  const now = new SvelteDate();
  const draft = createFormDraft<{ email: string; role: OrgRole }>({
    name: () => `organization-invitation:${organizationId}`,
    snapshot: () => ({ email, role }),
    parse: (value) =>
      isRecord(value) &&
      typeof value.email === 'string' &&
      (value.role === 'admin' || value.role === 'member')
        ? { email: value.email, role: value.role }
        : undefined,
    restore: (value) => {
      email = value.email;
      role = value.role;
    },
    reset: () => {
      email = '';
      role = 'member';
    },
    dirty: () => email !== '' || role !== 'member',
    busy: () => busy,
  });
  onMount(() => {
    const detach = draft.attach();
    void load();
    return () => {
      disposed = true;
      seq += 1;
      detach();
    };
  });

  async function load(): Promise<OrganizationInvitation[] | undefined> {
    const mine = ++seq;
    loadError = '';
    try {
      const list = await organizationsApi.invitations(organizationId);
      if (!disposed && mine === seq) {
        invitations = list;
        return list;
      }
    } catch (e: unknown) {
      if (!disposed && mine === seq) {
        if (organizationUnavailable(e)) onunavailable();
        else loadError = organizationError(e);
      }
    }
    return undefined;
  }
  async function invite(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (busy) return;
    const attempt = draft.capture();
    if (attempt === undefined) return;
    busy = true;
    error = '';
    emailError = '';
    roleError = '';
    try {
      const sent = await organizationsApi.inviteOrganization(organizationId, email.trim(), role);
      if (!draft.isOwner(attempt)) return;
      if (draft.complete(attempt)) notifyDone(m.organizations_invited({ email: sent.email }));
      await load();
    } catch (e: unknown) {
      if (!draft.isCurrent(attempt)) return;
      if (organizationUnavailable(e)) onunavailable();
      else if (e instanceof ApiError && e.status === 400 && e.field === 'email')
        emailError = invitationEmailError(e) ?? organizationError(e);
      else if (e instanceof ApiError && e.status === 400 && e.field === 'role')
        roleError = organizationError(e);
      else error = organizationError(e);
    } finally {
      if (draft.isOwner(attempt)) busy = false;
    }
  }
  async function revoke(id: string): Promise<void> {
    if (revoking !== undefined) return;
    revoking = id;
    error = '';
    try {
      await api.revokeInvitation(id);
      if (disposed) return;
      invitations = invitations?.filter((invitation) => invitation.id !== id);
      notifyDone(m.organizations_revoked());
      await tick();
      document.getElementById('organization-invitations-heading')?.focus();
    } catch (e: unknown) {
      if (!disposed) {
        if (e instanceof ApiError && e.status === 404) {
          invitations = invitations?.filter((invitation) => invitation.id !== id);
          await tick();
          if (!current()) return;
          document.getElementById('organization-invitations-heading')?.focus();
          const refreshed = await load();
          if (refreshed !== undefined && current()) {
            if (refreshed.some((invitation) => invitation.id === id)) error = organizationError(e);
            else notifyDone(m.organizations_revoked());
          }
        } else if (organizationUnavailable(e)) onunavailable();
        else error = organizationError(e);
      }
    } finally {
      if (!disposed) revoking = undefined;
    }
  }
</script>

<form class="flex min-w-0 flex-wrap items-end gap-3" onsubmit={(event) => void invite(event)}>
  <div class="min-w-0 flex-1 basis-48 space-y-1">
    <Label for="organization-invite-email">{m.organizations_email()}</Label>
    <Input
      id="organization-invite-email"
      type="email"
      required
      bind:value={email}
      disabled={busy || draft.blocked}
      aria-invalid={emailError !== ''}
      aria-describedby={emailError !== '' ? 'organization-email-error' : undefined}
    />
    <p id="organization-email-error" class="text-sm text-destructive empty:hidden">{emailError}</p>
  </div>
  <div class="space-y-1">
    <RoleSelect
      value={role}
      roles={['admin', 'member']}
      label={m.organizations_role()}
      disabled={busy || draft.blocked}
      describedBy="organization-invite-role-error"
      invalid={roleError !== ''}
      onchange={(value: Role) => {
        if (value === 'admin' || value === 'member') role = value;
      }}
    />
    <p id="organization-invite-role-error" class="text-sm text-destructive empty:hidden">
      {roleError}
    </p>
  </div>
  <Button type="submit" disabled={busy || draft.blocked}>{m.organizations_send_invite()}</Button>
</form>
{#each draft.offers as offer (offer.key)}<FormDraftOffer
    onrestore={() => {
      draft.restore(offer);
    }}
    ondiscard={() => {
      draft.discard(offer);
    }}
  />{/each}
<p role="alert" class="mt-3 text-sm text-destructive empty:hidden">{error}</p>
<h2 id="organization-invitations-heading" tabindex="-1" class="mt-5 font-semibold">
  {m.organizations_invitations()}
</h2>
<p role="alert" class="text-sm text-destructive empty:hidden">{loadError}</p>
{#if loadError !== ''}<Button variant="outline" onclick={() => void load()}
    >{m.common_retry()}</Button
  >{/if}
{#if invitations === undefined && loadError === ''}
  <div class="mt-3 animate-pulse space-y-3" aria-label={m.app_loading()} aria-busy="true">
    {#each [1, 2, 3] as n (n)}<div class="h-16 rounded bg-muted"></div>{/each}
  </div>
{:else if invitations !== undefined}
  {#if invitations.length === 0}<div class="my-3 flex flex-wrap items-center gap-3">
      <p>{m.organizations_invitations_empty()}</p>
      <Button
        variant="outline"
        onclick={() => document.getElementById('organization-invite-email')?.focus()}
        >{m.organizations_write_invite()}</Button
      >
    </div>{/if}
  <ul class="divide-y" aria-label={m.organizations_invitations()}>
    {#each invitations as invitation (invitation.id)}
      {@const expired = new SvelteDate(invitation.expires_at).getTime() <= now.getTime()}
      <li class="flex min-w-0 flex-wrap items-center gap-3 py-3">
        <div
          class={`min-w-0 flex-1 basis-48 wrap-anywhere ${expired ? 'text-muted-foreground' : ''}`}
        >
          <p>{invitation.email}</p>
          <p class="text-sm">
            {roleLabel(invitation.role)} · {m.organizations_expires({
              when: formatInstant(invitation.expires_at),
            })}
          </p>
          {#if expired}<span class="text-sm">{m.organizations_expired()}</span>{/if}
        </div>
        <Button
          variant="outline"
          size="sm"
          disabled={revoking !== undefined}
          aria-label={m.organizations_revoke_named({ email: invitation.email })}
          onclick={() => void revoke(invitation.id)}>{m.organizations_revoke()}</Button
        >
      </li>
    {/each}
  </ul>
{/if}
<DiscardChangesConfirm
  open={draft.leave.open}
  onkeep={draft.leave.keep}
  ondiscard={draft.discardAndLeave}
/>
