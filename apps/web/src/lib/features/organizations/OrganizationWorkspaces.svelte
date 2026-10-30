<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { formatInstant } from '$lib/display';
  import { createFormDraft } from '$lib/features/common/form-draft.svelte';
  import DiscardChangesConfirm from '$lib/features/common/DiscardChangesConfirm.svelte';
  import { notifyDone } from '$lib/features/feedback/feedback.svelte';
  import { roleLabel } from '$lib/features/members/roles';
  import { m } from '$lib/paraglide/messages.js';
  import { router, workspacePath } from '$lib/router.svelte';
  import { ApiError } from '$lib/shared/api';
  import { Button } from '$lib/ui/button';
  import * as Dialog from '$lib/ui/dialog';
  import { Label } from '$lib/ui/label';
  import { NativeSelect } from '$lib/ui/native-select';
  import { organizationsApi } from './api';
  import { organizationError, organizationUnavailable } from './feedback';
  import type { OrganizationMember, OrganizationWorkspace } from './model';

  interface Props {
    organizationId: string;
    meId: string;
    onchanged: () => void;
    onunavailable: () => void;
  }
  const { organizationId, meId, onchanged, onunavailable }: Props = $props();
  let workspaces = $state<OrganizationWorkspace[]>();
  let error = $state('');
  let actionError = $state('');
  let dialog = $state<{ kind: 'join' | 'offer'; workspace: OrganizationWorkspace }>();
  let candidates = $state<OrganizationMember[]>();
  let candidateError = $state('');
  let dialogError = $state('');
  let recipientError = $state('');
  let toUser = $state('');
  let busy = $state(false);
  let disposed = false;
  let returnFocus: HTMLElement | null = null;
  let returnWorkspace = '';
  let candidateSeq = 0;
  const draft = createFormDraft<string>({
    preserve: false,
    name: () =>
      dialog?.kind === 'offer'
        ? `organization-ownership:${organizationId}:${dialog.workspace.id}`
        : undefined,
    snapshot: () => toUser,
    parse: (value) => (typeof value === 'string' ? value : undefined),
    restore: (value) => {
      toUser = value;
    },
    reset: () => {
      toUser = '';
    },
    dirty: () => dialog?.kind === 'offer' && toUser !== '',
    busy: () => busy,
  });
  onMount(() => {
    void load();
    const detach = draft.attach();
    return () => {
      disposed = true;
      candidateSeq += 1;
      detach();
    };
  });
  async function load(): Promise<void> {
    error = '';
    try {
      const list = await organizationsApi.workspaces(organizationId);
      if (!disposed) workspaces = list;
    } catch (e: unknown) {
      if (!disposed) {
        if (organizationUnavailable(e)) onunavailable();
        else error = organizationError(e);
      }
    }
  }
  async function loadCandidates(): Promise<void> {
    const mine = ++candidateSeq;
    candidateError = '';
    candidates = undefined;
    try {
      const list = await organizationsApi.members(organizationId);
      if (!disposed && mine === candidateSeq)
        candidates = list.filter((member) => member.user_id !== meId);
    } catch (e: unknown) {
      if (!disposed && mine === candidateSeq) {
        if (organizationUnavailable(e)) onunavailable();
        else candidateError = organizationError(e);
      }
    }
  }
  function open(
    kind: 'join' | 'offer',
    workspace: OrganizationWorkspace,
    trigger: HTMLElement,
  ): void {
    if (busy) return;
    returnFocus = trigger;
    returnWorkspace = workspace.id;
    toUser = '';
    dialogError = '';
    recipientError = '';
    candidates = undefined;
    dialog = { kind, workspace };
    if (kind === 'offer') void loadCandidates();
  }
  function close(): void {
    draft.leave.request(() => {
      dialog = undefined;
      candidateSeq += 1;
    });
  }
  async function submit(): Promise<void> {
    const pending = dialog;
    if (pending === undefined || busy) return;
    if (pending.kind === 'offer' && toUser === '') {
      recipientError = m.organizations_choose_recipient();
      return;
    }
    busy = true;
    dialogError = '';
    recipientError = '';
    try {
      if (pending.kind === 'join')
        await organizationsApi.joinAsAdmin(organizationId, pending.workspace.id);
      else await organizationsApi.offerOwnership(organizationId, pending.workspace.id, toUser);
      if (disposed) return;
      toUser = '';
      dialog = undefined;
      if (pending.kind === 'join') {
        workspaces = workspaces?.map((workspace) =>
          workspace.id === pending.workspace.id
            ? { ...workspace, my_role: 'admin', member_count: workspace.member_count + 1 }
            : workspace,
        );
        notifyDone(m.organizations_joined_done(), `organization-join:${pending.workspace.id}`, {
          action: {
            label: m.organizations_open_workspace(),
            onClick: () => {
              router.navigate(workspacePath(pending.workspace.id));
            },
          },
        });
        onchanged();
      } else notifyDone(m.organizations_ownership_sent());
    } catch (e: unknown) {
      if (disposed) return;
      if (organizationUnavailable(e)) onunavailable();
      else if (
        pending.kind === 'offer' &&
        e instanceof ApiError &&
        (e.status === 400 || e.status === 409)
      )
        recipientError = m.organizations_recipient_must_be_member();
      else dialogError = organizationError(e);
    } finally {
      if (!disposed) busy = false;
    }
  }
  async function restore(workspace: OrganizationWorkspace): Promise<void> {
    if (busy) return;
    busy = true;
    actionError = '';
    try {
      await organizationsApi.restoreWorkspace(organizationId, workspace.id);
      if (disposed) return;
      workspaces = workspaces?.map((row) =>
        row.id === workspace.id ? { ...row, deleted_at: null } : row,
      );
      notifyDone(m.organizations_restored({ name: workspace.name }));
      onchanged();
      await tick();
      document.getElementById('organization-workspaces-heading')?.focus();
    } catch (e: unknown) {
      if (!disposed) {
        if (organizationUnavailable(e)) onunavailable();
        else actionError = organizationError(e);
      }
    } finally {
      if (!disposed) busy = false;
    }
  }
</script>

<p role="alert" class="text-sm text-destructive empty:hidden">{error || actionError}</p>
{#if error !== ''}<Button variant="outline" onclick={() => void load()}>{m.common_retry()}</Button
  >{/if}
{#if workspaces === undefined && error === ''}
  <div class="animate-pulse space-y-3" aria-label={m.app_loading()} aria-busy="true">
    {#each [1, 2, 3] as n (n)}<div class="h-20 rounded bg-muted"></div>{/each}
  </div>
{:else if workspaces !== undefined}
  {#each [false, true] as deleted (deleted)}
    {@const list = workspaces.filter((workspace) => Boolean(workspace.deleted_at) === deleted)}
    <section
      class="mb-6 min-w-0"
      aria-labelledby={deleted ? 'organization-deleted-heading' : 'organization-workspaces-heading'}
    >
      <h2
        id={deleted ? 'organization-deleted-heading' : 'organization-workspaces-heading'}
        class="font-semibold"
        tabindex="-1"
      >
        {deleted ? m.organizations_deleted_workspaces() : m.organizations_workspaces()}
      </h2>
      {#if list.length === 0}<p class="my-3 text-sm text-muted-foreground">
          {deleted ? m.organizations_deleted_empty() : m.organizations_workspaces_empty()}
        </p>{/if}
      <ul class="divide-y">
        {#each list as workspace (workspace.id)}
          <li
            class="flex min-w-0 flex-wrap items-center gap-3 py-4"
            data-organization-workspace={workspace.id}
          >
            <div class="min-w-0 flex-1 basis-56 wrap-anywhere">
              <p class="font-medium">{workspace.name}</p>
              <p class="text-sm text-muted-foreground">
                {m.organizations_workspace_owner({ name: workspace.owner_name })} · {m.organizations_member_count(
                  { count: workspace.member_count },
                )}
              </p>
              <p class="text-sm text-muted-foreground">
                {m.organizations_created({ when: formatInstant(workspace.created_at) })}
              </p>
              {#if workspace.my_role}<p class="text-sm">
                  {m.organizations_my_role({ role: roleLabel(workspace.my_role) })}
                </p>{/if}
              {#if workspace.deleted_at}<p class="text-sm text-muted-foreground">
                  {m.organizations_deleted({ when: formatInstant(workspace.deleted_at) })}
                </p>{/if}
            </div>
            <div class="flex min-w-0 flex-wrap gap-2">
              {#if deleted}
                <Button
                  variant="outline"
                  size="sm"
                  disabled={busy}
                  onclick={() => void restore(workspace)}>{m.organizations_restore()}</Button
                >
              {:else}
                {#if workspace.my_role}
                  <Button
                    id={`org-workspace-open-${workspace.id}`}
                    variant="outline"
                    size="sm"
                    href={workspacePath(workspace.id)}
                    onclick={(event: MouseEvent) => {
                      if (
                        !event.ctrlKey &&
                        !event.metaKey &&
                        !event.shiftKey &&
                        event.button === 0
                      ) {
                        event.preventDefault();
                        router.navigate(workspacePath(workspace.id));
                      }
                    }}>{m.organizations_open()}</Button
                  >
                {:else}
                  <Button
                    variant="outline"
                    size="sm"
                    disabled={busy}
                    onclick={(event: MouseEvent) => {
                      if (event.currentTarget instanceof HTMLElement)
                        open('join', workspace, event.currentTarget);
                    }}>{m.organizations_join_as_admin()}</Button
                  >
                {/if}
                <Button
                  variant="outline"
                  size="sm"
                  disabled={busy}
                  onclick={(event: MouseEvent) => {
                    if (event.currentTarget instanceof HTMLElement)
                      open('offer', workspace, event.currentTarget);
                  }}>{m.organizations_offer_ownership()}</Button
                >
              {/if}
            </div>
          </li>
        {/each}
      </ul>
    </section>
  {/each}
{/if}

<Dialog.Root
  open={dialog !== undefined}
  onOpenChange={(open) => {
    if (!open) close();
  }}
>
  <Dialog.Content
    class="max-h-[90vh] overflow-y-auto"
    onCloseAutoFocus={(event: Event) => {
      event.preventDefault();
      if (returnFocus?.isConnected) returnFocus.focus();
      else
        (
          document.getElementById(`org-workspace-open-${returnWorkspace}`) ??
          document.getElementById('organization-workspaces-heading')
        )?.focus();
    }}
  >
    <Dialog.Header>
      <Dialog.Title
        >{dialog?.kind === 'join'
          ? m.organizations_join_question({ name: dialog.workspace.name })
          : m.organizations_offer_ownership()}</Dialog.Title
      >
      <Dialog.Description
        >{dialog?.kind === 'join'
          ? m.organizations_join_read_all()
          : m.organizations_offer_hint()}</Dialog.Description
      >
    </Dialog.Header>
    {#if dialog?.kind === 'join'}
      <p>{m.organizations_join_audit()}</p>
      <p>{m.organizations_join_revert()}</p>
    {:else}
      <div class="space-y-2">
        <Label for="organization-ownership-recipient">{m.organizations_recipient()}</Label>
        <NativeSelect
          id="organization-ownership-recipient"
          class="w-full"
          bind:value={toUser}
          disabled={busy || candidates === undefined}
          aria-invalid={recipientError !== ''}
          aria-describedby="organization-recipient-error"
        >
          <option value="">{m.organizations_choose_recipient()}</option>
          {#each candidates ?? [] as candidate (candidate.user_id)}<option value={candidate.user_id}
              >{candidate.display_name}{candidate.email ? ` (${candidate.email})` : ''}</option
            >{/each}
        </NativeSelect>
        {#if candidates?.length === 0}<p>{m.organizations_recipients_empty()}</p>{/if}
        <p id="organization-recipient-error" class="text-sm text-destructive empty:hidden">
          {recipientError}
        </p>
        <p role="alert" class="text-sm text-destructive empty:hidden">{candidateError}</p>
        {#if candidateError !== ''}<Button variant="outline" onclick={() => void loadCandidates()}
            >{m.common_retry()}</Button
          >{/if}
      </div>
    {/if}
    <p role="alert" class="text-sm text-destructive empty:hidden">{dialogError}</p>
    <Dialog.Footer
      ><Button variant="ghost" disabled={busy} onclick={close}>{m.common_cancel()}</Button><Button
        disabled={busy || (dialog?.kind === 'offer' && candidates === undefined)}
        onclick={() => void submit()}
        >{dialog?.kind === 'join'
          ? m.organizations_join_as_admin()
          : m.organizations_send_offer()}</Button
      ></Dialog.Footer
    >
  </Dialog.Content>
</Dialog.Root>
<DiscardChangesConfirm
  open={draft.leave.open}
  onkeep={draft.leave.keep}
  ondiscard={draft.discardAndLeave}
/>
