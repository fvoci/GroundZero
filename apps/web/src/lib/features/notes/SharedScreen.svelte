<script lang="ts">
  // 나에게 공유된 문서 (D-23). 제공자가 확인한 이메일 기준이다.
  import type { SharedView } from '$lib/api/client';
  import { api } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import { m } from '$lib/paraglide/messages.js';
  import { router, standalonePath } from '$lib/router.svelte';

  let docs = $state<SharedView[] | undefined>(undefined);
  let error = $state<string | undefined>(undefined);

  api
    .sharedWithMe()
    .then((list) => {
      docs = list;
    })
    .catch((e: unknown) => {
      error = errorText(e, m.notes_shared_load_failed());
    });
</script>

<section class="flex flex-col gap-6" aria-labelledby="shared-heading">
  <h1 id="shared-heading" class="text-xl font-semibold">{m.notes_shared_heading()}</h1>
  {#if error !== undefined}
    <p role="alert" class="text-destructive">{error}</p>
  {:else if docs === undefined}
    <p aria-live="polite">{m.notes_loading()}</p>
  {:else if docs.length === 0}
    <p class="text-muted-foreground">{m.notes_shared_empty()}</p>
  {:else}
    <ul class="flex flex-col gap-1" data-testid="shared-list">
      {#each docs as doc (doc.id)}
        <li>
          <a
            class="flex flex-wrap items-baseline gap-2 rounded px-2 py-1.5 hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
            href={standalonePath(doc.id)}
            onclick={(e: MouseEvent) => {
              e.preventDefault();
              router.navigate(standalonePath(doc.id));
            }}
          >
            <span>{doc.title}</span>
            <span class="text-sm text-muted-foreground"
              >{doc.permission === 'write'
                ? m.notes_shared_by_write({ name: doc.shared_by })
                : m.notes_shared_by_read({ name: doc.shared_by })}</span
            >
          </a>
        </li>
      {/each}
    </ul>
  {/if}
</section>
