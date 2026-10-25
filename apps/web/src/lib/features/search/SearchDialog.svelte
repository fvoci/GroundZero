<script lang="ts">
  // 통합 검색 창(D-39). 사이드바 단추나 Ctrl/⌘+K로 연다. 볼 수 있는 것만 서버가 돌려준다.
  import { onMount, untrack } from 'svelte';

  import type { Module } from '@fvoci/api-client';

  import type { SearchFilters, SearchKind, SearchResultView } from '$lib/api/client';
  import { api } from '$lib/api/client';
  import { displaySettings } from '$lib/display';
  import { org } from '$lib/org.svelte';
  import { m } from '$lib/paraglide/messages.js';
  import { router } from '$lib/router.svelte';
  import { Checkbox } from '$lib/ui/checkbox';
  import * as Dialog from '$lib/ui/dialog';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';
  import * as Select from '$lib/ui/select';

  import { readableSnippet, SEARCH_KINDS, searchKindLabel, searchResultPath } from './search';

  let {
    open = $bindable(false),
    memberOf,
    workspace,
  }: {
    open?: boolean;
    memberOf: (ws: string) => boolean;
    /** 지금 보는 워크스페이스. 있으면 작성자로 그 멤버를 고를 수 있다. */
    workspace?: { id: string; name: string } | undefined;
  } = $props();

  let query = $state('');
  let results = $state<SearchResultView[]>([]);
  let status = $state<'idle' | 'loading' | 'done' | 'error'>('idle');
  let seq = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;

  // 필터(D-39). 상태·내 담당을 고르면 업무만 나온다.
  let kind = $state<SearchKind | 'all'>('all');
  // 현재 서버 계약은 범주가 아니라 상태 이름이다. D-116과의 충돌은 코디네이터에게 보고했다.
  let taskStatus = $state('');
  let mine = $state(false);
  let from = $state('');
  let to = $state('');
  // 작성자(만든 사람)와 제목만. 작성자 거르기는 좁히기만 한다(볼 수 없는 것은 그대로 안 나온다).
  let author = $state<string>('any');
  let titleOnly = $state(false);
  let workspaceOnly = $state(false);
  let people = $state<{ user_id: string; display_name: string }[]>([]);
  // 고를 때의 사람 이름. 대화상자를 다시 열어 멤버 목록을 다시 읽는 동안에도 작성자 거르기는 남으므로, 목록이 오기 전에는
  // 이 이름을 보인다(표시는 "누구나"인데 거르기는 남아 있는 일이 없게. 모듈도 같다).
  let authorName = $state('');
  // 모듈(D-39 모듈 필터). 기존 API의 `module_id`를 지금 보는 워크스페이스의 모듈 목록과 잇는다. 작성자와 같이 좁히기만
  // 한다. 고른 모듈이 목록에 없으면(다른 워크스페이스로 옮김·목록을 못 읽음) 그 선택을 거둔다.
  let moduleId = $state<string>('any');
  let modules = $state<Module[]>([]);
  // 고를 때의 모듈 이름. 대화상자를 다시 열어 목록을 다시 읽는 동안에도 선택(거르기)은 남으므로, 목록이 오기 전에는 이
  // 이름을 보인다(표시는 "모든 모듈"인데 거르기는 남아 있는 일이 없게).
  let moduleName = $state('');
  // 고른 모듈이 속한 워크스페이스. 워크스페이스가 바뀌면 새 목록이 오기 전에 바로 선택을 거둔다(옛 모듈 id로 검색하지 않게).
  let moduleWs: string | undefined;
  function dropMissingModule(list: readonly Module[]): void {
    untrack(() => {
      if (moduleId === 'any' || list.some((x) => x.id === moduleId)) return;
      moduleId = 'any';
      refilter();
    });
  }
  $effect(() => {
    const ws = workspace?.id;
    modules = [];
    if (moduleWs !== ws) dropMissingModule([]);
    if (!open) return;
    if (org.currentId === undefined) return;
    if (ws === undefined) return;
    let cancelled = false;
    api
      .modules(ws)
      .then((list) => {
        if (cancelled) return;
        modules = list.filter((x) => !x.archived);
        dropMissingModule(modules);
      })
      .catch(() => {
        if (!cancelled) dropMissingModule([]);
      });
    return () => {
      cancelled = true;
    };
  });
  const moduleLabel = $derived(
    moduleId === 'any'
      ? m.search_module_any()
      : (modules.find((x) => x.id === moduleId)?.name ??
          (moduleName === '' ? m.search_module_any() : moduleName)),
  );
  // 고른 사람이 지금 멤버 목록에 없으면(다른 워크스페이스로 옮김·목록을 못 읽음) 그 선택을 거둔다(표시는 "누구나"인데
  // 거르기는 남아 있는 일이 없게). "나"·"누구나"는 그대로.
  function dropMissingAuthor(list: readonly { user_id: string }[]): void {
    untrack(() => {
      if (author === 'any' || author === 'me' || list.some((p) => p.user_id === author)) return;
      author = 'any';
      refilter();
    });
  }
  $effect(() => {
    const ws = workspace?.id;
    people = [];
    if (!open) return;
    if (org.currentId === undefined) return;
    if (ws === undefined) {
      dropMissingAuthor([]);
      return;
    }
    let cancelled = false;
    api
      .members(ws)
      .then((v) => {
        if (cancelled) return;
        people = v.members;
        dropMissingAuthor(v.members);
      })
      .catch(() => {
        if (!cancelled) dropMissingAuthor([]);
      });
    return () => {
      cancelled = true;
    };
  });
  const authorLabel = $derived(
    author === 'any'
      ? m.search_author_any()
      : author === 'me'
        ? m.search_author_me()
        : (people.find((p) => p.user_id === author)?.display_name ??
          (authorName === '' ? m.search_author_any() : authorName)),
  );
  const taskOnly = $derived(taskStatus.trim() !== '' || mine);
  const reversed = $derived(from !== '' && to !== '' && from > to);

  function filters(): SearchFilters {
    const f: SearchFilters = {};
    if (taskOnly) f.kind = 'task';
    else if (kind !== 'all') f.kind = kind;
    if (taskStatus.trim() !== '') f.status = taskStatus.trim();
    if (mine) f.assignee = 'me';
    if (from !== '') f.from = from;
    if (to !== '') f.to = to;
    // 날짜의 하루는 설정의 시간대로 서버가 정한다(D-101). 화면은 시간대만 보내고 경계를 계산하지 않는다.
    if (from !== '' || to !== '') f.tz = displaySettings().timeZone;
    if (author !== 'any') f.author = author;
    if (moduleId !== 'any') f.moduleId = moduleId;
    if (titleOnly) f.titleOnly = true;
    return f;
  }

  /** 필터를 바꾸면 같은 검색어로 바로 다시 찾는다. */
  function refilter(): void {
    changed(query, 0);
  }

  async function run(q: string, ticket: number): Promise<void> {
    try {
      // 검색은 현재 조직 범위다(D-111). 조직을 고르기 전에는 찾지 않는다.
      const organizationId = org.currentId;
      if (organizationId === undefined) return;
      const found = await api.search(
        organizationId,
        q,
        workspaceOnly ? (workspace?.id ?? null) : null,
        filters(),
      );
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
    if (q === '' || org.currentId === undefined) {
      results = [];
      status = 'idle';
      return;
    }
    status = 'loading';
    timer = setTimeout(() => {
      run(q, ticket).catch(() => undefined);
    }, delay);
  }

  $effect(() => {
    const organizationId = org.currentId;
    untrack(() => {
      workspaceOnly = false;
      moduleId = 'any';
      author = 'any';
      results = [];
      if (organizationId !== undefined && open) changed(query, 0);
      else {
        seq += 1;
        if (timer !== undefined) clearTimeout(timer);
        status = 'idle';
      }
    });
  });

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
      <Dialog.Title>{m.search_title()}</Dialog.Title>
      <Dialog.Description>{m.search_description()}</Dialog.Description>
    </Dialog.Header>
    <p role="status" class="text-sm text-muted-foreground">
      {org.current === undefined
        ? m.app_organization_required()
        : m.search_organization_scope({ name: org.current.name })}
    </p>
    {#if workspace !== undefined && org.currentId !== undefined}
      <label class="flex items-center gap-2 text-sm"
        ><Checkbox
          checked={workspaceOnly}
          onCheckedChange={(value: boolean) => {
            workspaceOnly = value;
            refilter();
          }}
        />{m.search_workspace_scope({ name: workspace.name })}</label
      >
    {/if}
    <Input
      type="search"
      aria-label={m.search_query()}
      placeholder={m.search_query()}
      maxlength={200}
      disabled={org.currentId === undefined}
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
      <legend class="sr-only">{m.search_filters()}</legend>
      <div class="flex flex-col gap-1">
        <span id="search-kind-label" class="font-medium">{m.search_kind()}</span>
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
              ? searchKindLabel('task')
              : kind === 'all'
                ? m.search_all()
                : searchKindLabel(kind)}</Select.Trigger
          >
          <Select.Content>
            <Select.Item value="all" label={m.search_all()}>{m.search_all()}</Select.Item>
            {#each SEARCH_KINDS as k (k)}
              <Select.Item value={k} label={searchKindLabel(k)}>{searchKindLabel(k)}</Select.Item>
            {/each}
          </Select.Content>
        </Select.Root>
      </div>
      <div class="flex flex-col gap-1">
        <Label for="search-status">{m.search_task_status()}</Label>
        <Input
          id="search-status"
          class="w-40"
          value={taskStatus}
          maxlength={50}
          placeholder={m.search_status_name()}
          oninput={(e) => {
            if (e.currentTarget instanceof HTMLInputElement) {
              taskStatus = e.currentTarget.value;
              refilter();
            }
          }}
        />
      </div>
      {#if workspace !== undefined}
        <div class="flex flex-col gap-1">
          <span id="search-module-label" class="font-medium">{m.search_module()}</span>
          <Select.Root
            type="single"
            value={moduleId}
            onValueChange={(v: string) => {
              moduleId = v === '' ? 'any' : v;
              moduleWs = workspace.id;
              moduleName = modules.find((x) => x.id === v)?.name ?? '';
              refilter();
            }}
          >
            <Select.Trigger class="w-40" aria-labelledby="search-module-label"
              >{moduleLabel}</Select.Trigger
            >
            <Select.Content>
              <Select.Item value="any" label={m.search_module_any()}
                >{m.search_module_any()}</Select.Item
              >
              {#each modules as x (x.id)}
                <Select.Item value={x.id} label={x.name}>{x.name}</Select.Item>
              {/each}
            </Select.Content>
          </Select.Root>
        </div>
      {/if}
      <div class="flex flex-col gap-1">
        <span id="search-author-label" class="font-medium">{m.search_author()}</span>
        <Select.Root
          type="single"
          value={author}
          onValueChange={(v: string) => {
            author = v === '' ? 'any' : v;
            authorName = people.find((p) => p.user_id === v)?.display_name ?? '';
            refilter();
          }}
        >
          <Select.Trigger class="w-36" aria-labelledby="search-author-label"
            >{authorLabel}</Select.Trigger
          >
          <Select.Content>
            <Select.Item value="any" label={m.search_author_any()}
              >{m.search_author_any()}</Select.Item
            >
            <Select.Item value="me" label={m.search_author_me()}>{m.search_author_me()}</Select.Item
            >
            {#each people as p (p.user_id)}
              <Select.Item value={p.user_id} label={p.display_name}>{p.display_name}</Select.Item>
            {/each}
          </Select.Content>
        </Select.Root>
      </div>
      <div class="flex items-center gap-2 pb-2">
        <Checkbox
          id="search-title-only"
          bind:checked={titleOnly}
          onCheckedChange={() => {
            refilter();
          }}
        />
        <Label for="search-title-only">{m.search_title_only()}</Label>
      </div>
      <div class="flex items-center gap-2 pb-2">
        <Checkbox
          id="search-mine"
          bind:checked={mine}
          onCheckedChange={() => {
            refilter();
          }}
        />
        <Label for="search-mine">{m.search_mine()}</Label>
      </div>
      <div class="flex items-end gap-2">
        <div class="flex flex-col gap-1">
          <Label for="search-from">{m.search_from()}</Label>
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
          <Label for="search-to">{m.search_to()}</Label>
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
      </div>
    </fieldset>
    <!-- 조건 설명은 접어 둬서 결과가 첫 화면에 바로 보이게 한다(좁은 화면에서 결과가 아래로 밀리지 않게). -->
    <details class="text-xs text-muted-foreground" data-testid="search-filters-help">
      <summary
        class="w-fit cursor-pointer rounded select-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
        >{m.search_filters_help()}</summary
      >
      <p class="mt-1">{m.search_date_hint()}</p>
      <p class="mt-1">{m.search_author_hint()}</p>
    </details>
    <div aria-live="polite" class="text-sm text-muted-foreground">
      {#if reversed}
        <span class="text-destructive">{m.search_reversed()}</span>
      {:else if status === 'loading'}
        {m.search_searching()}
      {:else if status === 'error'}
        <span class="text-destructive">{m.search_failed()}</span>
      {:else if status === 'done' && results.length === 0}
        {m.search_no_results()}
      {:else if status === 'done'}
        {m.search_result_count({ count: results.length })}
      {/if}
    </div>
    {#if results.length > 0}
      <!-- 대화상자 본문은 격자라 칸 폭이 내용 폭을 따라간다. 줄바꿈하지 않는 긴 제목(truncate)이 칸을 넓혀 대화상자 전체가
           옆으로 넘치지 않게 목록의 최소 폭을 0으로 둔다(제목은 말줄임, 요약은 줄바꿈). -->
      <ul bind:this={list} class="flex min-w-0 flex-col gap-1" data-testid="search-results">
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
                <span class="shrink-0 text-xs text-muted-foreground">{searchKindLabel(r.kind)}</span
                >
                {#if r.reference !== null}
                  <span class="shrink-0 font-mono text-xs text-muted-foreground">{r.reference}</span
                  >
                {/if}
                <span class="truncate font-medium" title={r.title}>{r.title}</span>
              </span>
              {#if r.snippet !== null}
                <span class="mt-0.5 line-clamp-2 text-sm break-words text-muted-foreground"
                  >{readableSnippet(r.snippet)}</span
                >
              {/if}
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </Dialog.Content>
</Dialog.Root>
