<script lang="ts">
  // 내가 받은 초대(제공자가 확인한 이메일 기준). 수락하면 워크스페이스로 이동한다 (D-51).
  import type { ReceivedInvitation } from '@fvoci/api-client';

  import { api } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import { org } from '$lib/org.svelte';
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
  // 답하는 중인 초대. 그동안 수락·거절을 막는다(두 번 누르면 두 번째가 409로 실패했다).
  let busy = $state<string | undefined>(undefined);

  async function load(): Promise<void> {
    try {
      items = await api.myInvitations();
    } catch (e: unknown) {
      error = errorText(e, m.members_invitations_load_failed());
    }
  }

  async function respond(inv: ReceivedInvitation, accept: boolean): Promise<void> {
    if (busy !== undefined) return;
    busy = inv.id;
    error = undefined;
    try {
      if (accept) {
        // 조직 초대는 워크스페이스가 없다(D-112·D-118): 목록을 다시 읽은 뒤 그 조직을 현재 조직으로 고른다.
        const accepted = await api.acceptInvitation(inv.id);
        await onaccepted();
        if (accepted.kind === 'workspace' && typeof accepted.workspace_id === 'string') {
          router.navigate(workspacePath(accepted.workspace_id));
        } else {
          org.select(accepted.organization_id);
          router.navigate('/');
        }
      } else {
        await api.declineInvitation(inv.id);
        await load();
      }
    } catch (e: unknown) {
      error = errorText(e, m.members_invitation_failed());
      await load();
    } finally {
      busy = undefined;
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
          <span class="font-medium"
            >{inv.kind === 'workspace' ? (inv.workspace_name ?? '') : inv.organization_name}</span
          >
          <span class="text-sm text-muted-foreground">{inv.invited_by} · {roleLabel(inv.role)}</span
          >
          <span class="ml-auto flex gap-2">
            <Button size="sm" disabled={busy !== undefined} onclick={() => void respond(inv, true)}
              >{m.members_accept()}</Button
            >
            <Button
              size="sm"
              variant="ghost"
              disabled={busy !== undefined}
              onclick={() => void respond(inv, false)}>{m.members_decline()}</Button
            >
          </span>
        </li>
      {/each}
    </ul>
  </section>
{/if}
