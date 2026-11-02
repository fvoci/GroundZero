<script lang="ts">
  import type { Workspace } from '@fvoci/api-client';

  const {
    workspaces,
    onjoined,
  }: {
    workspaces: Workspace[];
    onjoined: () => Promise<void>;
  } = $props();
  let value = $state('');
  let failed = $state(false);
  async function refresh(): Promise<void> {
    try {
      await onjoined();
    } catch {
      // The host owns ordinary refresh failures, like the invitation flow.
      failed = true;
    }
  }
</script>

<section data-testid="refresh-private-frame">
  {#each workspaces as workspace (workspace.id)}
    <p>{workspace.name}</p>
  {/each}
  <label>Retained input <input bind:value /></label>
  <button onclick={() => void refresh()}>First membership refresh</button>
  <button onclick={() => void refresh()}>Second membership refresh</button>
  {#if failed}<p role="alert">Caller handled the ordinary failure</p>{/if}
</section>
