<script lang="ts">
  // 이 대상을 가리키는 문서(내가 볼 수 있는 것만, 서버가 거른다).
  import type { Workspace } from '@fvoci/api-client';

  import type { BacklinkView } from '$lib/api/client';
  import { api } from '$lib/api/client';
  import { pathFor, router } from '$lib/router.svelte';

  interface Props {
    /** 대상 종류와 id. */
    kind: 'document' | 'item';
    id: string;
    workspaces: Workspace[];
  }

  const { kind, id, workspaces }: Props = $props();

  let links = $state<BacklinkView[]>([]);
  let failed = $state(false);

  $effect(() => {
    const run = kind === 'document' ? api.documentBacklinks : api.itemBacklinks;
    const target = id;
    let cancelled = false;
    failed = false;
    run(target)
      .then((list) => {
        if (!cancelled) links = list;
      })
      .catch(() => {
        if (!cancelled) failed = true;
      });
    return () => {
      cancelled = true;
    };
  });

  const memberOf = (ws: string): boolean => workspaces.some((w) => w.id === ws);
</script>

<section class="flex flex-col gap-2 border-t pt-4" aria-labelledby="backlinks-heading">
  <h2 id="backlinks-heading" class="text-sm font-semibold">이곳을 가리키는 문서</h2>
  {#if failed}
    <p class="text-sm text-muted-foreground">불러오지 못했습니다.</p>
  {:else if links.length === 0}
    <p class="text-sm text-muted-foreground">없습니다.</p>
  {:else}
    <ul class="flex flex-col gap-1" data-testid="backlinks">
      {#each links as link (link.id)}
        {@const path = pathFor(link, memberOf)}
        <li>
          <a
            class="flex items-baseline gap-2 rounded px-2 py-1 text-sm hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
            href={path}
            onclick={(e: MouseEvent) => {
              e.preventDefault();
              router.navigate(path);
            }}
          >
            <span class="font-mono text-xs text-muted-foreground">{link.reference ?? ''}</span>
            <span>{link.title}</span>
          </a>
        </li>
      {/each}
    </ul>
  {/if}
</section>
