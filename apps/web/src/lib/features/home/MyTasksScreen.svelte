<script lang="ts">
  // 내 업무(D-120): 담당자가 나인 업무를 현재 조직 전체에서 워크스페이스별로 묶어 보인다. 기한 없는 업무는 "날짜 없음"
  // 묶음이다. 범주 기준 집계(`counts`)는 서버가 센다.
  import type { MyTasksView, MyTaskView } from '$lib/api/client';
  import { api } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import { displaySettings } from '$lib/display';
  import { dayIn } from '$lib/features/events/time';
  import { org } from '$lib/org.svelte';
  import { m } from '$lib/paraglide/messages.js';
  import { itemTaskPath, router } from '$lib/router.svelte';
  import { Button } from '$lib/ui/button';
  import { formatDay } from '$lib/display';

  const { summary = false, version = 0 }: { summary?: boolean; version?: number } = $props();

  let data = $state<MyTasksView | undefined>(undefined);
  let error = $state<string | undefined>(undefined);
  let retry = $state(0);

  let loadSeq = 0;
  const request = $derived({ retry, version, organizationId: org.currentId });
  $effect(() => {
    const { organizationId } = request;
    const mine = ++loadSeq;
    data = undefined;
    error = undefined;
    if (organizationId === undefined) {
      data = undefined;
      error = m.app_organization_required();
      return;
    }
    const today = dayIn(new Date().toISOString(), displaySettings().timeZone);
    void api
      .myTasks(organizationId, today)
      .then((d) => {
        if (mine !== loadSeq) return;
        data = d;
        error = undefined;
      })
      .catch((e: unknown) => {
        if (mine !== loadSeq) return;
        error = errorText(e, m.home_load_failed());
      });
    return () => {
      loadSeq += 1;
    };
  });

  const BUCKETS = ['overdue', 'today', 'upcoming', 'none', 'done'] as const;
  function bucketLabel(b: (typeof BUCKETS)[number]): string {
    switch (b) {
      case 'overdue':
        return m.home_bucket_overdue();
      case 'today':
        return m.home_bucket_today();
      case 'upcoming':
        return m.home_bucket_upcoming();
      case 'none':
        return m.home_my_tasks_undated();
      case 'done':
        return m.home_my_tasks_done();
    }
  }

  function go(e: MouseEvent, path: string): void {
    if (e.metaKey || e.ctrlKey || e.shiftKey || e.button !== 0) return;
    e.preventDefault();
    router.navigate(path);
  }

  // 마감 없는 업무는 뒤로, 같은 마감은 최근 갱신부터.
  function compareTasks(a: MyTaskView, b: MyTaskView): number {
    return (
      (a.due_date ?? '9999').localeCompare(b.due_date ?? '9999') ||
      b.updated_at.localeCompare(a.updated_at)
    );
  }
</script>

<section class="flex flex-col gap-6" aria-labelledby="my-tasks-heading">
  {#if summary}
    <h2 id="my-tasks-heading" class="text-lg font-semibold">{m.app_nav_my_tasks()}</h2>
  {:else}
    <h1 id="my-tasks-heading" class="text-xl font-semibold">{m.app_nav_my_tasks()}</h1>
    {#if org.current}<p class="text-sm text-muted-foreground">{org.current.name}</p>{/if}
  {/if}
  <p role="status" aria-live="polite" class="text-sm text-muted-foreground empty:hidden">
    {error ?? (data === undefined && org.currentId !== undefined ? m.app_loading() : '')}
  </p>
  {#if error !== undefined && org.currentId !== undefined}
    <Button variant="outline" onclick={() => (retry += 1)}>{m.common_retry()}</Button>
  {/if}
  {#if data !== undefined}
    <dl class="flex flex-wrap gap-x-6 gap-y-2 text-sm" data-testid="my-tasks-counts">
      {#each BUCKETS.filter((b) => !summary || b !== 'done') as b (b)}
        <div class="flex gap-1">
          <dt class="text-muted-foreground">{bucketLabel(b)}</dt>
          <dd class="font-medium">
            {b === 'none' ? data.counts.undated : data.counts[b]}
          </dd>
        </div>
      {/each}
    </dl>
    {#if data.workspaces.length === 0}
      <p class="text-sm text-muted-foreground">{m.home_my_tasks_empty()}</p>
    {/if}
    {#if summary}
      {#each BUCKETS.filter((b) => b !== 'done') as b (b)}
        {@const tasks = data.workspaces
          .flatMap((w) =>
            w.tasks
              .filter((t) => t.bucket === b)
              .map((t) => ({ ...t, workspace_id: w.workspace_id })),
          )
          .sort(compareTasks)}
        {@const count = b === 'none' ? data.counts.undated : data.counts[b]}
        {#if count > 0}
          <section class="flex flex-col gap-1">
            <h3 class="text-sm font-semibold">{bucketLabel(b)} {count}</h3>
            {#each tasks.slice(0, 5) as t (t.id)}
              {@const path = itemTaskPath(t.workspace_id, t.item_id, t.id)}
              <a
                class="rounded px-2 py-1 text-sm break-words hover:bg-accent"
                href={path}
                onclick={(e) => {
                  go(e, path);
                }}>{t.title}</a
              >
            {/each}
          </section>
        {/if}
      {/each}
      <a
        class="text-sm underline"
        href="/tasks"
        onclick={(e) => {
          go(e, '/tasks');
        }}>{m.home_all_my_tasks()}</a
      >
    {:else}
      {#each data.workspaces as w (w.workspace_id)}
        <section class="flex flex-col gap-2" aria-labelledby={`my-tasks-${w.workspace_id}`}>
          <h2 id={`my-tasks-${w.workspace_id}`} class="font-semibold">
            {w.workspace_name} · {w.tasks.length}
          </h2>
          <!-- 워크스페이스 안에서도 요약과 같은 날짜 묶음(제목과 수)으로 나눈다. 기한 없는 업무는 "날짜 없음" 묶음이다. -->
          {#each BUCKETS as b (b)}
            {@const tasks = w.tasks.filter((t) => t.bucket === b).sort(compareTasks)}
            {#if tasks.length > 0}
              <section class="flex flex-col gap-1">
                <h3 class="text-sm font-semibold">{bucketLabel(b)} {tasks.length}</h3>
                <ul class="flex flex-col divide-y rounded border">
                  {#each tasks as t (t.id)}
                    {@const path = itemTaskPath(w.workspace_id, t.item_id, t.id)}
                    <li>
                      <a
                        href={path}
                        class="flex flex-wrap items-center gap-2 px-3 py-2 hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
                        onclick={(e) => {
                          go(e, path);
                        }}
                      >
                        {#if t.reference !== null}
                          <span class="font-mono text-xs text-muted-foreground">{t.reference}</span>
                        {/if}
                        <span class="min-w-0 flex-1 truncate">{t.title}</span>
                        {#if t.due_date != null}
                          <time datetime={t.due_date} class="text-xs text-muted-foreground"
                            >{formatDay(t.due_date)}</time
                          >
                        {/if}
                      </a>
                    </li>
                  {/each}
                </ul>
              </section>
            {/if}
          {/each}
        </section>
      {/each}
    {/if}
  {/if}
</section>
