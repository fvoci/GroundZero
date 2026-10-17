<script lang="ts">
  // 내가 받은 초대(제공자가 확인한 이메일 기준). 수락하면 워크스페이스로 이동한다 (D-51).
  import type { ReceivedInvitation } from '@fvoci/api-client';

  import { api } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import { m } from '$lib/paraglide/messages.js';
  import { router, workspacePath } from '$lib/router.svelte';
  import { Button } from '$lib/ui/button';

  import { roleLabel } from './roles';

  interface Props {
    onaccepted: () => Promise<void>;
  }

  const { onaccepted }: Props = $props();

  let items = $state<ReceivedInvitation[]>([]);
  let error = $state<string | undefined>(undefined);

  async function load(): Promise<void> {
    try {
      items = await api.myInvitations();
    } catch (e: unknown) {
      error = errorText(e, m.members_invitations_load_failed());
    }
  }

  async function respond(inv: ReceivedInvitation, accept: boolean): Promise<void> {
    error = undefined;
    try {
      if (accept) {
        const ws = await api.acceptInvitation(inv.id);
        await onaccepted();
        router.navigate(workspacePath(ws));
      } else {
        await api.declineInvitation(inv.id);
        await load();
      }
    } catch (e: unknown) {
      error = errorText(e, m.members_invitation_failed());
      await load();
    }
  }

  void load();
</script>

{#if items.length > 0 || error !== undefined}
  <section class="flex flex-col gap-2" aria-labelledby="received-heading">
    <h2 id="received-heading" class="font-semibold">{m.members_received_heading()}</h2>
    {#if error !== undefined}<p role="alert" class="text-sm text-destructive">{error}</p>{/if}
    <ul class="flex flex-col gap-2" data-testid="received-invitations">
      {#each items as inv (inv.id)}
        <li class="flex flex-wrap items-center gap-2 rounded border px-3 py-2">
          <span class="font-medium">{inv.workspace_name}</span>
          <span class="text-sm text-muted-foreground">{inv.invited_by} · {roleLabel(inv.role)}</span
          >
          <span class="ml-auto flex gap-2">
            <Button size="sm" onclick={() => void respond(inv, true)}>{m.members_accept()}</Button>
            <Button size="sm" variant="ghost" onclick={() => void respond(inv, false)}
              >{m.members_decline()}</Button
            >
          </span>
        </li>
      {/each}
    </ul>
  </section>
{/if}
