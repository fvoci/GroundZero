<script lang="ts">
  // 통합 검색 창(D-39). 사이드바 단추나 Ctrl/⌘+K로 연다. 볼 수 있는 것만 서버가 돌려준다.
  import { onMount } from 'svelte';

  import type { SearchFilters, SearchKind, SearchResultView, TaskStatus } from '$lib/api/client';
  import { api } from '$lib/api/client';
  import { STATUS_LABEL } from '$lib/features/tasks/labels';
  import { router } from '$lib/router.svelte';
  import { Checkbox } from '$lib/ui/checkbox';
  import * as Dialog from '$lib/ui/dialog';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';
  import * as Select from '$lib/ui/select';

  import { SEARCH_KIND_LABEL, SEARCH_KINDS, searchResultPath } from './search';

  let { open = $bindable(false), memberOf }: { open?: boolean; memberOf: (ws: string) => boolean } =
    $props();

  const TASK_STATUSES: TaskStatus[] = ['todo', 'in_progress', 'done'];

  let query = $state('');
  let results = $state<SearchResultView[]>([]);
  let status = $state<'idle' | 'loading' | 'done' | 'error'>('idle');
  let seq = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;

  // 필터(D-39). 상태·내 담당을 고르면 업무만 나온다.
  let kind = $state<SearchKind | 'all'>('all');
  let taskStatus = $state<TaskStatus | 'any'>('any');
  let mine = $state(false);
  let from = $state('');
  let to = $state('');
  const taskOnly = $derived(taskStatus !== 'any' || mine);
  const reversed = $derived(from !== '' && to !== '' && from > to);

  function filters(): SearchFilters {
    const f: SearchFilters = {};
    if (taskOnly) f.kind = 'task';
    else if (kind !== 'all') f.kind = kind;
    if (taskStatus !== 'any') f.status = taskStatus;
    if (mine) f.assignee = 'me';
    if (from !== '') f.from = from;
    if (to !== '') f.to = to;
    return f;
  }

  /** 필터를 바꾸면 같은 검색어로 바로 다시 찾는다. */
  function refilter(): void {
    changed(query, 0);
  }

  async function run(q: string, ticket: number): Promise<void> {
    try {
      const found = await api.search(q, null, filters());
      if (ticket !== seq) return;
      results = found;
      status = 'done';
    } catch {
      if (ticket !== seq) return;
      results = [];
      status = 'error';
    }
  }

  function changed(value: string, delay = 250): void {
    query = value;
    seq += 1;
    const ticket = seq;
    if (timer !== undefined) clearTimeout(timer);
    const q = value.trim();
    if (q === '') {
      results = [];
      status = 'idle';
      return;
    }
    status = 'loading';
    timer = setTimeout(() => {
      run(q, ticket).catch(() => undefined);
    }, delay);
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
        >문서·업무·항목·일정·파일(지원 형식은 본문까지)에서 찾습니다. 볼 수 있는 것만 나옵니다.</Dialog.Description
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
    <fieldset class="flex flex-wrap items-end gap-3 text-sm" data-testid="search-filters">
      <legend class="sr-only">필터</legend>
      <div class="flex flex-col gap-1">
        <span id="search-kind-label" class="font-medium">종류</span>
        <Select.Root
          type="single"
          value={taskOnly ? 'task' : kind}
          disabled={taskOnly}
          onValueChange={(v: string) => {
            kind = v === 'all' ? 'all' : (SEARCH_KINDS.find((k) => k === v) ?? 'all');
            refilter();
          }}
        >
          <Select.Trigger class="w-24" aria-labelledby="search-kind-label"
            >{taskOnly
              ? SEARCH_KIND_LABEL.task
              : kind === 'all'
                ? '전체'
                : SEARCH_KIND_LABEL[kind]}</Select.Trigger
          >
          <Select.Content>
            <Select.Item value="all" label="전체">전체</Select.Item>
            {#each SEARCH_KINDS as k (k)}
              <Select.Item value={k} label={SEARCH_KIND_LABEL[k]}
                >{SEARCH_KIND_LABEL[k]}</Select.Item
              >
            {/each}
          </Select.Content>
        </Select.Root>
      </div>
      <div class="flex flex-col gap-1">
        <span id="search-status-label" class="font-medium">업무 상태</span>
        <Select.Root
          type="single"
          value={taskStatus}
          onValueChange={(v: string) => {
            taskStatus = TASK_STATUSES.find((s) => s === v) ?? 'any';
            refilter();
          }}
        >
          <Select.Trigger class="w-24" aria-labelledby="search-status-label"
            >{taskStatus === 'any' ? '전체' : STATUS_LABEL[taskStatus]}</Select.Trigger
          >
          <Select.Content>
            <Select.Item value="any" label="전체">전체</Select.Item>
            {#each TASK_STATUSES as s (s)}
              <Select.Item value={s} label={STATUS_LABEL[s]}>{STATUS_LABEL[s]}</Select.Item>
            {/each}
          </Select.Content>
        </Select.Root>
      </div>
      <div class="flex items-center gap-2 pb-2">
        <Checkbox
          id="search-mine"
          bind:checked={mine}
          onCheckedChange={() => {
            refilter();
          }}
        />
        <Label for="search-mine">내 담당</Label>
      </div>
      <div class="flex flex-col gap-1">
        <Label for="search-from">날짜부터</Label>
        <Input
          id="search-from"
          type="date"
          class="w-36"
          bind:value={from}
          onchange={() => {
            refilter();
          }}
        />
      </div>
      <div class="flex flex-col gap-1">
        <Label for="search-to">까지</Label>
        <Input
          id="search-to"
          type="date"
          class="w-36"
          bind:value={to}
          onchange={() => {
            refilter();
          }}
        />
      </div>
    </fieldset>
    <p class="text-xs text-muted-foreground">
      날짜는 업무의 기한(없으면 시작일), 일정의 시작일, 그 밖에는 고친 날로 봅니다.
    </p>
    <div aria-live="polite" class="text-sm text-muted-foreground">
      {#if reversed}
        <span class="text-destructive">시작 날짜가 끝 날짜보다 늦습니다.</span>
      {:else if status === 'loading'}
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
