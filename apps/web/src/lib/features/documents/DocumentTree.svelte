<script lang="ts">
  // 문서 트리 (D-27). WAI-ARIA 트리 패턴: 위·아래로 항목 이동, 오른쪽·왼쪽으로 펼치기·접기와 부모·자식 이동,
  // Enter로 열기, Home·End. 쓰기 권한이 있으면 Alt+위·아래로 순서, Alt+오른쪽·왼쪽으로 들여쓰기·내어쓰기,
  // Alt+Enter로 하위 문서 만들기. 같은 동작을 항목 메뉴로도 할 수 있다(마우스).
  import { tick } from 'svelte';
  import { SvelteSet } from 'svelte/reactivity';

  import type { SummaryView } from '$lib/api/client';
  import { Button } from '$lib/ui/button';
  import * as DropdownMenu from '$lib/ui/dropdown-menu';

  import type { TreeAction, TreeMove } from './tree';
  import { planMove, visibleRows } from './tree';

  interface Props {
    docs: SummaryView[];
    label: string;
    hrefFor: (id: string) => string;
    navigate: (path: string) => void;
    canWrite: boolean;
    onmove: (id: string, move: TreeMove) => Promise<void>;
    oncreatechild: (parentId: string) => Promise<void>;
  }

  const { docs, label, hrefFor, navigate, canWrite, onmove, oncreatechild }: Props = $props();

  // 접은 항목. 기본은 모두 펼침.
  const collapsed = new SvelteSet<string>();
  let focusId = $state<string | undefined>(undefined);
  let root: HTMLUListElement | undefined = $state();
  let busy = $state(false);
  let status = $state('');

  const rows = $derived(visibleRows(docs, collapsed));
  const current = $derived(rows.find((r) => r.doc.id === focusId) ?? rows[0]);

  async function focusRow(id: string | undefined): Promise<void> {
    if (id === undefined) return;
    focusId = id;
    await tick();
    root?.querySelector<HTMLElement>(`[data-tree-id="${CSS.escape(id)}"]`)?.focus();
  }

  function toggle(id: string, open?: boolean): void {
    const shouldOpen = open ?? collapsed.has(id);
    if (shouldOpen) collapsed.delete(id);
    else collapsed.add(id);
  }

  const ACTION_LABEL: Record<TreeAction, string> = {
    up: '위로 옮겼습니다',
    down: '아래로 옮겼습니다',
    indent: '들여썼습니다',
    outdent: '내어썼습니다',
  };

  async function act(id: string, action: TreeAction): Promise<void> {
    const move = planMove(docs, id, action);
    if (move === undefined || busy) return;
    busy = true;
    try {
      if (action === 'indent' && move.parentId !== null) toggle(move.parentId, true);
      await onmove(id, move);
      status = ACTION_LABEL[action];
      await focusRow(id);
    } finally {
      busy = false;
    }
  }

  async function createChild(id: string): Promise<void> {
    if (busy) return;
    busy = true;
    try {
      toggle(id, true);
      await oncreatechild(id);
    } finally {
      busy = false;
    }
  }

  function onkeydown(event: KeyboardEvent, id: string): void {
    const index = rows.findIndex((r) => r.doc.id === id);
    const row = rows[index];
    if (row === undefined) return;
    if (event.altKey && canWrite) {
      const map: Partial<Record<string, TreeAction>> = {
        ArrowUp: 'up',
        ArrowDown: 'down',
        ArrowRight: 'indent',
        ArrowLeft: 'outdent',
      };
      const action = map[event.key];
      if (action !== undefined) {
        event.preventDefault();
        void act(id, action);
        return;
      }
      if (event.key === 'Enter') {
        event.preventDefault();
        void createChild(id);
        return;
      }
    }
    switch (event.key) {
      case 'ArrowDown':
        event.preventDefault();
        void focusRow(rows[index + 1]?.doc.id);
        break;
      case 'ArrowUp':
        event.preventDefault();
        void focusRow(rows[index - 1]?.doc.id);
        break;
      case 'Home':
        event.preventDefault();
        void focusRow(rows[0]?.doc.id);
        break;
      case 'End':
        event.preventDefault();
        void focusRow(rows.at(-1)?.doc.id);
        break;
      case 'ArrowRight':
        event.preventDefault();
        if (row.hasChildren && collapsed.has(id)) toggle(id, true);
        else if (row.hasChildren) void focusRow(rows[index + 1]?.doc.id);
        break;
      case 'ArrowLeft': {
        event.preventDefault();
        if (row.hasChildren && !collapsed.has(id)) {
          toggle(id, false);
          break;
        }
        const parent = rows
          .slice(0, index)
          .reverse()
          .find((r) => r.depth === row.depth - 1);
        void focusRow(parent?.doc.id);
        break;
      }
      case 'Enter':
        event.preventDefault();
        navigate(hrefFor(id));
        break;
    }
  }
</script>

{#if rows.length > 0}
  <ul
    bind:this={root}
    role="tree"
    aria-label={label}
    class="flex flex-col"
    data-testid="document-tree"
  >
    {#each rows as row (row.doc.id)}
      {@const id = row.doc.id}
      <li
        role="treeitem"
        data-tree-id={id}
        aria-label={row.doc.reference === null
          ? row.doc.title
          : `${row.doc.reference} ${row.doc.title}`}
        aria-level={row.depth}
        aria-setsize={row.setSize}
        aria-posinset={row.position}
        aria-expanded={row.hasChildren ? !collapsed.has(id) : undefined}
        aria-selected={current?.doc.id === id}
        aria-keyshortcuts={canWrite
          ? 'Alt+ArrowUp Alt+ArrowDown Alt+ArrowRight Alt+ArrowLeft Alt+Enter'
          : undefined}
        tabindex={current?.doc.id === id ? 0 : -1}
        class="group flex items-center gap-1 rounded py-0.5 pr-1 focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
        style:padding-left={`${String((row.depth - 1) * 1.25)}rem`}
        onkeydown={(e) => {
          onkeydown(e, id);
        }}
        onfocus={() => (focusId = id)}
      >
        {#if row.hasChildren}
          <button
            type="button"
            tabindex="-1"
            aria-hidden="true"
            class="flex size-6 shrink-0 items-center justify-center rounded text-muted-foreground hover:bg-accent"
            onclick={() => {
              toggle(id);
            }}>{collapsed.has(id) ? '▸' : '▾'}</button
          >
        {:else}
          <span class="size-6 shrink-0" aria-hidden="true"></span>
        {/if}
        <a
          tabindex="-1"
          class="flex min-w-0 flex-1 items-baseline gap-2 rounded px-1 py-1 hover:bg-accent"
          href={hrefFor(id)}
          onclick={(e: MouseEvent) => {
            e.preventDefault();
            navigate(hrefFor(id));
          }}
        >
          <span class="shrink-0 font-mono text-sm text-muted-foreground"
            >{row.doc.reference ?? ''}</span
          >
          <span class="truncate">{row.doc.title}</span>
        </a>
        {#if canWrite}
          <DropdownMenu.Root>
            <DropdownMenu.Trigger>
              {#snippet child({ props })}
                <Button
                  {...props}
                  tabindex={-1}
                  size="sm"
                  variant="ghost"
                  aria-label={`${row.doc.title} 문서 동작`}
                  class="opacity-60 group-focus-within:opacity-100 group-hover:opacity-100"
                  >⋯</Button
                >
              {/snippet}
            </DropdownMenu.Trigger>
            <DropdownMenu.Content align="end">
              <DropdownMenu.Item onSelect={() => void createChild(id)}
                >하위 문서 만들기</DropdownMenu.Item
              >
              <DropdownMenu.Item
                disabled={planMove(docs, id, 'up') === undefined}
                onSelect={() => void act(id, 'up')}>위로</DropdownMenu.Item
              >
              <DropdownMenu.Item
                disabled={planMove(docs, id, 'down') === undefined}
                onSelect={() => void act(id, 'down')}>아래로</DropdownMenu.Item
              >
              <DropdownMenu.Item
                disabled={planMove(docs, id, 'indent') === undefined}
                onSelect={() => void act(id, 'indent')}
                >들여쓰기(앞 문서의 하위로)</DropdownMenu.Item
              >
              <DropdownMenu.Item
                disabled={planMove(docs, id, 'outdent') === undefined}
                onSelect={() => void act(id, 'outdent')}
                >내어쓰기(상위 문서 다음으로)</DropdownMenu.Item
              >
            </DropdownMenu.Content>
          </DropdownMenu.Root>
        {/if}
      </li>
    {/each}
  </ul>
  {#if canWrite}
    <p class="text-xs text-muted-foreground">
      키보드: Alt+위·아래 순서, Alt+오른쪽·왼쪽 들여쓰기·내어쓰기, Alt+Enter 하위 문서.
    </p>
  {/if}
  <p class="sr-only" aria-live="polite">{status}</p>
{/if}
