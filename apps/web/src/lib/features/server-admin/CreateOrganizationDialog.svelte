<script lang="ts">
  import { onMount } from 'svelte';
  import { createFormDraft } from '$lib/features/common/form-draft.svelte';
  import DiscardChangesConfirm from '$lib/features/common/DiscardChangesConfirm.svelte';
  import { notifyDone } from '$lib/features/feedback/feedback.svelte';
  import { organizationError, organizationUnavailable } from '$lib/features/organizations/feedback';
  import { m } from '$lib/paraglide/messages.js';
  import { ApiError } from '$lib/shared/api';
  import { isRecord } from '$lib/shared/api/parse';
  import { Button } from '$lib/ui/button';
  import * as Dialog from '$lib/ui/dialog';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';
  import { NativeSelect } from '$lib/ui/native-select';
  import { serverAdminApi } from './api';
  import type { ServerAdmin } from './model';

  const {
    meId,
    oncreated,
    onunavailable,
  }: { meId: string; oncreated: () => void; onunavailable: () => void } = $props();
  let dialogId = $state<string>();
  let requestId: string | undefined;
  let submitted: { name: string; firstAdmin: string } | undefined;
  let name = $state('');
  let firstAdmin = $state('');
  let admins = $state<ServerAdmin[]>();
  let candidateError = $state('');
  let error = $state('');
  let nameError = $state('');
  let adminError = $state('');
  let busy = $state(false);
  let disposed = false;
  let candidateSeq = 0;
  const draft = createFormDraft<{ name: string; firstAdmin: string }>({
    preserve: false,
    name: () => (dialogId === undefined ? undefined : `server-organization:${dialogId}`),
    snapshot: () => ({ name, firstAdmin }),
    parse: (v) =>
      isRecord(v) && typeof v.name === 'string' && typeof v.firstAdmin === 'string'
        ? { name: v.name, firstAdmin: v.firstAdmin }
        : undefined,
    restore: (v) => {
      name = v.name;
      firstAdmin = v.firstAdmin;
    },
    reset: () => {
      name = '';
      firstAdmin = meId;
    },
    dirty: () =>
      dialogId !== undefined && (name !== '' || (firstAdmin !== '' && firstAdmin !== meId)),
    busy: () => busy,
  });
  onMount(() => {
    const detach = draft.attach();
    return () => {
      disposed = true;
      candidateSeq += 1;
      detach();
    };
  });
  async function loadAdmins(): Promise<void> {
    const mine = ++candidateSeq;
    candidateError = '';
    admins = undefined;
    try {
      const list = await serverAdminApi.admins();
      if (disposed || mine !== candidateSeq) return;
      admins = list;
      if (!list.some((admin) => admin.user_id === firstAdmin)) firstAdmin = '';
    } catch (e: unknown) {
      if (disposed || mine !== candidateSeq) return;
      if (organizationUnavailable(e)) onunavailable();
      else candidateError = organizationError(e);
    }
  }
  function open(): void {
    // Keep the form session stable when a changed payload needs a new request ID.
    dialogId = crypto.randomUUID();
    requestId = dialogId;
    submitted = undefined;
    name = '';
    firstAdmin = meId;
    error = '';
    nameError = '';
    adminError = '';
    void loadAdmins();
  }
  function close(): void {
    draft.leave.request(() => {
      dialogId = undefined;
      requestId = undefined;
      submitted = undefined;
      candidateSeq += 1;
    });
  }
  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (busy || requestId === undefined || admins === undefined || admins.length === 0) return;
    nameError = name.trim() === '' ? m.organizations_name_required() : '';
    adminError = admins.some((admin) => admin.user_id === firstAdmin)
      ? ''
      : m.organizations_choose_first_admin();
    if (nameError !== '' || adminError !== '') return;
    const attempt = draft.capture();
    if (attempt === undefined) return;
    const input = { name: name.trim(), firstAdmin };
    if (
      submitted !== undefined &&
      (submitted.name !== input.name || submitted.firstAdmin !== input.firstAdmin)
    )
      requestId = crypto.randomUUID();
    submitted = input;
    busy = true;
    error = '';
    try {
      await serverAdminApi.createOrganization({
        request_id: requestId,
        name: input.name,
        first_admin_user_id: input.firstAdmin,
      });
      if (!draft.isOwner(attempt)) return;
      if (draft.complete(attempt)) {
        dialogId = undefined;
        requestId = undefined;
        submitted = undefined;
        candidateSeq += 1;
        notifyDone(m.organizations_created_done());
        oncreated();
      }
    } catch (e: unknown) {
      if (!draft.isCurrent(attempt)) return;
      if (organizationUnavailable(e)) onunavailable();
      else if (e instanceof ApiError && e.status === 400 && e.field === 'name')
        nameError = organizationError(e);
      else if (e instanceof ApiError && e.status === 400 && e.field === 'first_admin_user_id')
        adminError = organizationError(e);
      else error = organizationError(e);
    } finally {
      if (!disposed) busy = false;
    }
  }
  const blockedReason = $derived(
    busy
      ? m.organizations_creating()
      : admins === undefined
        ? candidateError || m.app_loading()
        : admins.length === 0
          ? m.organizations_server_admins_empty()
          : '',
  );
</script>

<Button id="server-create-organization" onclick={open}>{m.organizations_create()}</Button>
<Dialog.Root
  bind:open={
    () => dialogId !== undefined,
    (open: boolean) => {
      if (!open) close();
    }
  }
>
  <Dialog.Content
    class="max-h-[90vh] overflow-y-auto"
    escapeKeydownBehavior={busy || draft.leave.open ? 'ignore' : 'close'}
    interactOutsideBehavior={busy || draft.leave.open ? 'ignore' : 'close'}
    showCloseButton={!busy}
    onCloseAutoFocus={(event: Event) => {
      event.preventDefault();
      document.getElementById('server-create-organization')?.focus();
    }}
  >
    <Dialog.Header>
      <Dialog.Title>{m.organizations_create()}</Dialog.Title>
      <Dialog.Description id="server-first-admin-help"
        >{m.organizations_first_admin_help()}</Dialog.Description
      >
    </Dialog.Header>
    <form
      aria-label={m.organizations_create()}
      class="min-w-0 space-y-4"
      onsubmit={(event) => void submit(event)}
    >
      <div class="space-y-2">
        <Label for="server-organization-name">{m.organizations_name()}</Label>
        <Input
          id="server-organization-name"
          bind:value={name}
          disabled={busy}
          aria-invalid={nameError !== ''}
          aria-describedby={nameError !== ''
            ? `server-name-error${busy ? ' server-create-state' : ''}`
            : busy
              ? 'server-create-state'
              : undefined}
        />
        <p id="server-name-error" role="alert" class="text-sm text-destructive empty:hidden">
          {nameError}
        </p>
      </div>
      <div class="min-w-0 space-y-2">
        <Label for="server-first-admin">{m.organizations_first_admin()}</Label>
        <NativeSelect
          id="server-first-admin"
          class="w-full"
          bind:value={firstAdmin}
          disabled={busy || admins === undefined || admins.length === 0}
          aria-invalid={adminError !== ''}
          aria-describedby={`server-first-admin-help${adminError !== '' ? ' server-admin-error' : ''}${blockedReason !== '' ? ' server-create-state' : ''}`}
        >
          <option value="">{m.organizations_choose_first_admin()}</option>
          {#each admins ?? [] as admin (admin.user_id)}<option value={admin.user_id}
              >{admin.display_name}</option
            >{/each}
        </NativeSelect>
        <p id="server-admin-error" role="alert" class="text-sm text-destructive empty:hidden">
          {adminError}
        </p>
      </div>
      {#if admins === undefined && candidateError === ''}
        <div
          class="h-12 animate-pulse rounded bg-muted"
          aria-label={m.app_loading()}
          aria-busy="true"
        ></div>
      {/if}
      <p role="alert" class="text-sm text-destructive empty:hidden">{candidateError}</p>
      {#if candidateError !== ''}<Button variant="outline" onclick={() => void loadAdmins()}
          >{m.common_retry()}</Button
        >{/if}
      <p role="alert" class="text-sm text-destructive empty:hidden">{error}</p>
      <p id="server-create-state" role="status" class="text-sm text-muted-foreground empty:hidden">
        {blockedReason}
      </p>
      <Dialog.Footer>
        <Button
          variant="ghost"
          disabled={busy}
          aria-describedby={busy ? 'server-create-state' : undefined}
          onclick={close}>{m.common_cancel()}</Button
        >
        <Button
          type="submit"
          disabled={busy || admins === undefined || admins.length === 0}
          aria-describedby={blockedReason !== '' ? 'server-create-state' : undefined}
          >{m.organizations_create()}</Button
        >
      </Dialog.Footer>
    </form>
    <DiscardChangesConfirm
      open={draft.leave.open}
      onkeep={draft.leave.keep}
      ondiscard={draft.discardAndLeave}
    />
  </Dialog.Content>
</Dialog.Root>
