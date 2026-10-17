<script lang="ts">
  import type { Workspace } from '@fvoci/api-client';

  import { api } from '$lib/api/client';
  import { m } from '$lib/paraglide/messages.js';
  import { router, workspacePath } from '$lib/router.svelte';

  import ReceivedInvitations from '$lib/features/members/ReceivedInvitations.svelte';

  import HomeDashboard from '$lib/features/home/HomeDashboard.svelte';

  import CreateForm from './CreateForm.svelte';
  import WorkspaceBadge from './WorkspaceBadge.svelte';

  interface Props {
    workspaces: Workspace[];
    oncreated: (ws: Workspace) => void;
    onjoined: () => Promise<void>;
  }

  const { workspaces, oncreated, onjoined }: Props = $props();
</script>

<HomeDashboard {workspaces} />

<section class="flex flex-col gap-6" aria-labelledby="home-heading">
  <ReceivedInvitations onaccepted={onjoined} />
  <h2 id="home-heading" class="text-xl font-semibold">{m.workspaces_heading()}</h2>
  {#if workspaces.length === 0}
    <p class="text-muted-foreground">{m.workspaces_empty()}</p>
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
    label={m.workspaces_new_name()}
    placeholder={m.workspaces_new_placeholder()}
    submitLabel={m.workspaces_create()}
    create={async (requestId: string, name: string) => {
      const ws = await api.createWorkspace(requestId, name);
      oncreated(ws);
      router.navigate(workspacePath(ws.id));
    }}
  />
</section>
