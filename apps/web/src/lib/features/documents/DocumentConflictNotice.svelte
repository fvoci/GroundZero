<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { DocNode, DocumentView } from '$lib/api/client';
  import { m } from '$lib/paraglide/messages.js';
  import { plainText } from './drafts';

  interface Props {
    current: Pick<DocumentView, 'title' | 'revision'>;
    title: string;
    serverBody?: DocNode;
    mineBody?: DocNode;
    children: Snippet;
  }
  const { current, title, serverBody, mineBody, children }: Props = $props();
  const heading = $props.id();
</script>

<section
  class="flex flex-col gap-3 rounded border border-amber-500/60 px-3 py-3"
  aria-labelledby={heading}
>
  <h2 id={heading} class="font-semibold">{m.documents_conflict_heading()}</h2>
  <p class="text-sm">{m.documents_conflict_body({ revision: current.revision })}</p>
  <div class="grid gap-3 @2xl:grid-cols-2">
    <div>
      <h3 class="text-sm font-semibold">{m.documents_conflict_server({ title: current.title })}</h3>
      {#if serverBody !== undefined}
        <pre
          class="max-h-60 overflow-auto rounded bg-muted p-2 text-sm whitespace-pre-wrap"
          data-testid="conflict-server">{plainText(serverBody)}</pre>
      {/if}
    </div>
    <div>
      <h3 class="text-sm font-semibold">{m.documents_conflict_mine({ title })}</h3>
      {#if mineBody !== undefined}
        <pre
          class="max-h-60 overflow-auto rounded bg-muted p-2 text-sm whitespace-pre-wrap"
          data-testid="conflict-mine">{plainText(mineBody)}</pre>
      {/if}
    </div>
  </div>
  <div class="flex flex-wrap gap-2">{@render children()}</div>
</section>
