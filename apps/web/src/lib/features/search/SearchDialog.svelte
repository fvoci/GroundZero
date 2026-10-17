<script lang="ts">
  // 통합 검색 창(D-39). 사이드바 단추나 Ctrl/⌘+K로 연다. 볼 수 있는 것만 서버가 돌려준다.
  import { onMount } from 'svelte';

  import type { SearchResultView } from '$lib/api/client';
  import { api } from '$lib/api/client';
  import { router } from '$lib/router.svelte';
  import * as Dialog from '$lib/ui/dialog';
  import { Input } from '$lib/ui/input';

  import { SEARCH_KIND_LABEL, searchResultPath } from './search';

  let { open = $bindable(false), memberOf }: { open?: boolean; memberOf: (ws: string) => boolean } =
    $props();

  let query = $state('');
  let results = $state<SearchResultView[]>([]);
  let status = $state<'idle' | 'loading' | 'done' | 'error'>('idle');
  let seq = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;

  async function run(q: string, mine: number): Promise<void> {
    try {
      const found = await api.search(q, null);
      if (mine !== seq) return;
      results = found;
      status = 'done';
    } catch {
      if (mine !== seq) return;
      results = [];
      status = 'error';
    }
  }

  function changed(value: string): void {
    query = value;
    seq += 1;
    const mine = seq;
    if (timer !== undefined) clearTimeout(timer);
    const q = value.trim();
    if (q === '') {
      results = [];
      status = 'idle';
      return;
    }
    status = 'loading';
    timer = setTimeout(() => {
      run(q, mine).catch(() => undefined);
    }, 250);
  }

  let list = $state<HTMLUListElement | undefined>();

  /** 위·아래 화살표로 검색어 칸과 결과 사이를 옮겨 다닌다. */
  function move(e: KeyboardEvent, from: number): void {
    if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
    const buttons = list === undefined ? [] : Array.from(list.querySelectorAll('button'));
    if (buttons.length === 0) return;
    e.preventDefault();
    const next = from + (e.key === 'ArrowDown' ? 1 : -1);
    if (next < 0) {
      document.querySelector<HTMLInputElement>('[data-testid="search-input"]')?.focus();
      return;
    }
    buttons[Math.min(next, buttons.length - 1)]?.focus();
  }

  function choose(r: SearchResultView): void {
    const path = searchResultPath(r, memberOf);
    if (path === undefined) return;
    open = false;
    router.navigate(path);
  }

  onMount(() => {
    const onKey = (e: KeyboardEvent): void => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault();
        open = true;
      }
    };
    window.addEventListener('keydown', onKey);
    return () => {
      window.removeEventListener('keydown', onKey);
      if (timer !== undefined) clearTimeout(timer);
      seq += 1;
    };
  });
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="max-h-[80vh] overflow-y-auto sm:max-w-xl">
    <Dialog.Header>
      <Dialog.Title>검색</Dialog.Title>
      <Dialog.Description
        >문서·업무·항목·일정에서 찾습니다. 볼 수 있는 것만 나옵니다.</Dialog.Description
      >
    </Dialog.Header>
    <Input
      type="search"
      aria-label="검색어"
      placeholder="검색어"
      maxlength={200}
      value={query}
      oninput={(e: Event) => {
        if (e.currentTarget instanceof HTMLInputElement) changed(e.currentTarget.value);
      }}
      onkeydown={(e: KeyboardEvent) => {
        move(e, -1);
      }}
      data-testid="search-input"
    />
    <div aria-live="polite" class="text-sm text-muted-foreground">
      {#if status === 'loading'}
        찾는 중…
      {:else if status === 'error'}
        <span class="text-destructive">검색하지 못했습니다. 잠시 뒤 다시 시도해 주세요.</span>
      {:else if status === 'done' && results.length === 0}
        찾은 것이 없습니다.
      {:else if status === 'done'}
        {results.length}건
      {/if}
    </div>
    {#if results.length > 0}
      <ul bind:this={list} class="flex flex-col gap-1" data-testid="search-results">
        {#each results as r, index (`${r.kind}:${r.id}`)}
          <li>
            <button
              type="button"
              class="w-full rounded-md px-2 py-1.5 text-left hover:bg-accent focus-visible:bg-accent focus-visible:outline-none"
              onclick={() => {
                choose(r);
              }}
              onkeydown={(e: KeyboardEvent) => {
                move(e, index);
              }}
            >
              <span class="flex items-baseline gap-2">
                <span class="shrink-0 text-xs text-muted-foreground"
                  >{SEARCH_KIND_LABEL[r.kind]}</span
                >
                {#if r.reference !== null}
                  <span class="shrink-0 font-mono text-xs text-muted-foreground">{r.reference}</span
                  >
                {/if}
                <span class="truncate font-medium">{r.title}</span>
              </span>
              {#if r.snippet !== null}
                <span class="mt-0.5 block text-sm text-muted-foreground">{r.snippet}</span>
              {/if}
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </Dialog.Content>
</Dialog.Root>
