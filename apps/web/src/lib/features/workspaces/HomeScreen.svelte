<script lang="ts">
  import type { Workspace } from '@fvoci/api-client';

  import { api } from '$lib/api/client';
  import { router, workspacePath } from '$lib/router.svelte';

  import ReceivedInvitations from '$lib/features/members/ReceivedInvitations.svelte';

  import CreateForm from './CreateForm.svelte';
  import WorkspaceBadge from './WorkspaceBadge.svelte';

  interface Props {
    workspaces: Workspace[];
    oncreated: (ws: Workspace) => void;
    onjoined: () => Promise<void>;
  }

  const { workspaces, oncreated, onjoined }: Props = $props();
</script>

<section class="flex flex-col gap-6" aria-labelledby="home-heading">
  <ReceivedInvitations onaccepted={onjoined} />
  <h1 id="home-heading" class="text-xl font-semibold">워크스페이스</h1>
  {#if workspaces.length === 0}
    <p class="text-muted-foreground">아직 워크스페이스가 없습니다. 첫 워크스페이스를 만드세요.</p>
  {:else}
    <ul class="flex flex-col gap-1" data-testid="workspace-list">
      {#each workspaces as ws (ws.id)}
        <li>
          <a
            class="flex items-baseline gap-2 rounded px-2 py-1.5 hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
            href={workspacePath(ws.id)}
            onclick={(e) => {
              e.preventDefault();
              router.navigate(workspacePath(ws.id));
            }}
          >
            <WorkspaceBadge workspace={ws} all={workspaces} />
            <span class="text-sm text-muted-foreground">({ws.role})</span>
          </a>
        </li>
      {/each}
    </ul>
  {/if}
  <CreateForm
    id="new-workspace"
    label="새 워크스페이스 이름"
    placeholder="예: 연구실"
    submitLabel="만들기"
    create={async (requestId: string, name: string) => {
      const ws = await api.createWorkspace(requestId, name);
      oncreated(ws);
      router.navigate(workspacePath(ws.id));
    }}
  />
</section>
