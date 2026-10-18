<script lang="ts">
  // 사이드바의 위키 문서 트리. 펼친 문서의 하위만 그리고, 지금 연 문서의 상위는 자동으로 펼친다.
  // 접고 펼치기는 버튼(aria-expanded), 이동은 링크(aria-current)로 키보드·화면 낭독기에서 같은 동작을 한다.
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import { SvelteSet } from 'svelte/reactivity';

  import type { SummaryView } from '$lib/api/client';

  import { childrenMap } from '$lib/features/documents/tree';
  import { m } from '$lib/paraglide/messages.js';
  import { documentPath, router } from '$lib/router.svelte';

  interface Props {
    workspaceId: string;
    docs: readonly SummaryView[];
    activeId: string | undefined;
  }

  const { workspaceId, docs, activeId }: Props = $props();

  const expanded = new SvelteSet<string>();

  // 지금 연 문서의 상위를 모두 펼친다(사용자가 펼치거나 접은 다른 문서는 그대로 둔다).
  $effect(() => {
    if (activeId === undefined) return;
    const byId = new Map(docs.map((d) => [d.id, d]));
    let parent = byId.get(activeId)?.parent_id ?? null;
    while (parent !== null && byId.has(parent)) {
      expanded.add(parent);
      parent = byId.get(parent)?.parent_id ?? null;
    }
  });

  // 펼친 문서만 깊이 우선으로 편 줄 목록.
  const rows = $derived.by(() => {
    const map = childrenMap(docs);
    const out: { doc: SummaryView; depth: number; hasChildren: boolean; open: boolean }[] = [];
    const walk = (parent: string | null, depth: number): void => {
      for (const doc of map.get(parent) ?? []) {
        const hasChildren = (map.get(doc.id)?.length ?? 0) > 0;
        const open = hasChildren && expanded.has(doc.id);
        out.push({ doc, depth, hasChildren, open });
        if (open) walk(doc.id, depth + 1);
      }
    };
    walk(null, 0);
    return out;
  });

  function toggle(id: string): void {
    if (expanded.has(id)) expanded.delete(id);
    else expanded.add(id);
  }

  function go(e: MouseEvent, id: string): void {
    if (e.metaKey || e.ctrlKey || e.shiftKey || e.button !== 0) return;
    e.preventDefault();
    router.navigate(documentPath(workspaceId, id));
  }
</script>

<ul class="flex min-w-0 flex-col gap-0.5">
  {#each rows as row (row.doc.id)}
    {@const active = row.doc.id === activeId}
    {@const title = row.doc.title === '' ? m.workspaces_untitled_document() : row.doc.title}
    <li
      class="flex h-8 min-w-0 items-center rounded-md text-sm hover:bg-sidebar-accent hover:text-sidebar-accent-foreground data-active:bg-sidebar-accent data-active:font-medium data-active:text-sidebar-accent-foreground"
      style:padding-left={`${String(row.depth * 0.75)}rem`}
      data-active={active ? '' : undefined}
    >
      {#if row.hasChildren}
        <button
          type="button"
          class="flex size-6 shrink-0 items-center justify-center rounded-sm text-sidebar-foreground/70 hover:text-sidebar-foreground focus-visible:ring-2 focus-visible:ring-sidebar-ring focus-visible:outline-none"
          aria-expanded={row.open}
          aria-label={row.open ? m.app_nav_collapse({ title }) : m.app_nav_expand({ title })}
          onclick={() => {
            toggle(row.doc.id);
          }}
        >
          <ChevronRight
            class={`size-3.5 transition-transform ${row.open ? 'rotate-90' : ''}`}
            aria-hidden="true"
          />
        </button>
      {:else}
        <span class="size-6 shrink-0" aria-hidden="true"></span>
      {/if}
      <a
        class="flex h-full min-w-0 flex-1 items-center rounded-md pr-2 focus-visible:ring-2 focus-visible:ring-sidebar-ring focus-visible:outline-none"
        href={documentPath(workspaceId, row.doc.id)}
        aria-current={active ? 'page' : undefined}
        {title}
        onclick={(e: MouseEvent) => {
          go(e, row.doc.id);
        }}
      >
        <span class="truncate">{title}</span>
      </a>
    </li>
  {/each}
</ul>
