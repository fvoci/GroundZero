<script lang="ts">
  import type { Workspace } from '@fvoci/api-client';

  import { api } from '$lib/api/client';
  import type { CreateFormContext } from '$lib/features/common/form-draft';
  import { org } from '$lib/org.svelte';
  import { m } from '$lib/paraglide/messages.js';
  import { router, workspacePath } from '$lib/router.svelte';

  import ReceivedInvitations from '$lib/features/members/ReceivedInvitations.svelte';
  import { roleLabel } from '$lib/features/members/roles';

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
            <span class="text-sm text-muted-foreground">({roleLabel(ws.role)})</span>
          </a>
        </li>
      {/each}
    </ul>
  {/if}
  <!-- 워크스페이스는 현재 조직 안에 만든다(D-112). 조직을 고르기 전에는 만들 수 없다. -->
  {#if org.currentId !== undefined}
    {@const organizationId = org.currentId}
    <CreateForm
      id="new-workspace"
      draftName={`workspace-new:${organizationId}`}
      label={m.workspaces_new_name()}
      placeholder={m.workspaces_new_placeholder()}
      submitLabel={m.workspaces_create()}
      create={async (requestId: string, name: string, context: CreateFormContext) => {
        const ws = await api.createWorkspace(requestId, organizationId, name);
        if (!context.saved()) return;
        oncreated(ws);
        router.navigate(workspacePath(ws.id));
      }}
    />
  {:else}
    <p class="text-sm text-muted-foreground">{m.app_organization_required()}</p>
  {/if}
</section>
