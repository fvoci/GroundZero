<script lang="ts">
  // 이 문서 바로 아래의 하위 문서 (D-27). 트리가 쓰는 같은 자리 목록(워크스페이스 위키·항목 문서·내 노트)을 부모로
  // 거른다. 목록은 서버가 볼 수 있는 것만 준다. 다른 워크스페이스에서 공유받아 연 문서처럼 그 목록을 읽을 수 없는
  // 자리면 보이지 않는다. 하위 문서가 없으면 아무것도 그리지 않는다.
  import type { Workspace } from '@fvoci/api-client';

  import type { DocumentView, SummaryView } from '$lib/api/client';
  import { api } from '$lib/api/client';
  import { m } from '$lib/paraglide/messages.js';
  import { pathFor, router } from '$lib/router.svelte';

  interface Props {
    doc: DocumentView;
    workspaces: Workspace[];
  }

  const { doc, workspaces }: Props = $props();

  const memberOf = (ws: string): boolean => workspaces.some((w) => w.id === ws);
  let children = $state<SummaryView[]>([]);
  let failed = $state(false);

  /** 같은 자리 목록을 읽는 방법. 읽을 수 없는 자리는 undefined. */
  function listFor(d: DocumentView): (() => Promise<SummaryView[]>) | undefined {
    if (d.item_id !== null) {
      const id = d.item_id;
      return () => api.itemDocuments(id);
    }
    if (d.workspace_id !== null) {
      const ws = d.workspace_id;
      return memberOf(ws) ? () => api.wikiDocuments(ws) : undefined;
    }
    return d.access === 'owner' ? () => api.myNotes() : undefined;
  }

  $effect(() => {
    const id = doc.id;
    const load = listFor(doc);
    let cancelled = false;
    children = [];
    failed = false;
    if (load === undefined) return;
    load()
      .then((list) => {
        if (!cancelled) children = list.filter((c) => c.parent_id === id);
      })
      .catch(() => {
        if (!cancelled) failed = true;
      });
    return () => {
      cancelled = true;
    };
  });
</script>

{#if failed}
  <p class="text-sm text-muted-foreground">{m.documents_children_failed()}</p>
{:else if children.length > 0}
  <nav class="flex flex-col gap-2 border-t pt-4" aria-labelledby="children-heading">
    <h2 id="children-heading" class="text-sm font-semibold">
      {m.documents_children_heading({ count: children.length })}
    </h2>
    <ul class="flex flex-col gap-1" data-testid="child-documents">
      {#each children as child (child.id)}
        {@const path = pathFor(
          { id: child.id, workspace_id: doc.workspace_id, item_id: doc.item_id },
          memberOf,
        )}
        <li>
          <a
            class="flex items-baseline gap-2 rounded px-2 py-1 text-sm hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
            href={path}
            onclick={(e: MouseEvent) => {
              e.preventDefault();
              router.navigate(path);
            }}
          >
            <span class="font-mono text-xs text-muted-foreground">{child.reference ?? ''}</span>
            <span>{child.title}</span>
          </a>
        </li>
      {/each}
    </ul>
  </nav>
{/if}
