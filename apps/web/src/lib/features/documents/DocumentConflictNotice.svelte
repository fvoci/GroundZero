<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { DocumentView } from '$lib/api/client';
  import { m } from '$lib/paraglide/messages.js';

  interface Props {
    current: Pick<DocumentView, 'title' | 'revision'>;
    title: string;
    children: Snippet;
  }
  const { current, title, children }: Props = $props();
  const heading = $props.id();
</script>

<section
  class="flex flex-col gap-3 rounded border border-amber-500/60 px-3 py-3"
  aria-labelledby={heading}
>
  <h2 id={heading} class="font-semibold">{m.documents_conflict_heading()}</h2>
  <p class="text-sm">{m.documents_conflict_body({ revision: current.revision })}</p>
  <div class="grid gap-3">
    <div>
      <h3 class="text-sm font-semibold">{m.documents_conflict_server({ title: current.title })}</h3>
    </div>
    <div>
      <h3 class="text-sm font-semibold">{m.documents_conflict_mine({ title })}</h3>
    </div>
  </div>
  <div class="flex flex-wrap gap-2">{@render children()}</div>
</section>
