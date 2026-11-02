<script lang="ts">
  // 이 대상을 가리키는 문서(내가 볼 수 있는 것만, 서버가 거른다).
  import type { Workspace } from '@fvoci/api-client';
  import { untrack } from 'svelte';

  import type { BacklinkView } from '$lib/api/client';
  import { api } from '$lib/api/client';
  import { accountSafety } from '$lib/features/feedback/feedback.svelte';
  import { me } from '$lib/me.svelte';
  import { org } from '$lib/org.svelte';
  import { m } from '$lib/paraglide/messages.js';
  import { pathFor, router } from '$lib/router.svelte';

  import { referenceHash } from './focus';
  import { settleBacklinks, type BacklinksRead } from './backlinks-read';

  interface Props {
    /** 대상 종류와 id. */
    kind: 'document' | 'item' | 'task';
    id: string;
    workspaces: Workspace[];
    takeInitialRead?: ((id: string) => BacklinksRead | undefined) | undefined;
  }

  const { kind, id, workspaces, takeInitialRead }: Props = $props();

  let links = $state<BacklinkView[]>([]);
  let failed = $state(false);

  $effect(() => {
    const run =
      kind === 'document'
        ? api.documentBacklinks
        : kind === 'item'
          ? api.itemBacklinks
          : api.taskBacklinks;
    const target = id;
    const context = untrack(() => ({
      userId: me.userId,
      organizationId: org.currentId,
      generation: accountSafety.generation,
    }));
    let cancelled = false;
    const isCurrent = (): boolean =>
      !cancelled &&
      context.userId === me.userId &&
      context.organizationId === org.currentId &&
      context.generation === accountSafety.generation;
    links = [];
    failed = false;
    const read = untrack(() => takeInitialRead?.(target)) ?? settleBacklinks(run(target));
    void read.then((result) => {
      if (!isCurrent()) return;
      if (result.kind === 'loaded') links = result.links;
      else failed = true;
    });
    return () => {
      cancelled = true;
    };
  });

  const memberOf = (ws: string): boolean => workspaces.some((w) => w.id === ws);
</script>

<section class="flex flex-col gap-2 border-t pt-4" aria-labelledby="backlinks-heading">
  <h2 id="backlinks-heading" class="text-sm font-semibold">
    {m.references_backlinks_heading()}
  </h2>
  {#if failed}
    <p class="text-sm text-muted-foreground">{m.references_backlinks_failed()}</p>
  {:else if links.length === 0}
    <p class="text-sm text-muted-foreground">{m.references_backlinks_empty()}</p>
  {:else}
    <ul class="flex flex-col gap-1" data-testid="backlinks">
      {#each links as link (link.id)}
        {@const path = `${pathFor(link, memberOf)}${referenceHash({ kind, id })}`}
        <li>
          <a
            class="flex items-baseline gap-2 rounded px-2 py-1 text-sm hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
            href={path}
            onclick={(e: MouseEvent) => {
              e.preventDefault();
              router.navigate(path);
            }}
          >
            <span class="shrink-0 font-mono text-xs whitespace-nowrap text-muted-foreground"
              >{link.reference ?? ''}</span
            >
            <span class="min-w-0 [overflow-wrap:anywhere]">{link.title}</span>
          </a>
        </li>
      {/each}
    </ul>
  {/if}
</section>
