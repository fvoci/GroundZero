<script lang="ts">
  // 내 업무(D-120): 담당자가 나인 업무를 현재 조직 전체에서 워크스페이스별로 묶어 보인다. 기한 없는 업무는 "날짜 없음"
  // 묶음이다. 집계(`counts`)는 서버가 센다. 3단계 토대 화면이며 묶음별 보기·정렬은 이어서 다듬는다.
  import type { MyTasksView } from '$lib/api/client';
  import { api } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import { displaySettings } from '$lib/display';
  import { dayIn } from '$lib/features/events/time';
  import { org } from '$lib/org.svelte';
  import { m } from '$lib/paraglide/messages.js';
  import { itemTaskPath, router } from '$lib/router.svelte';

  let data = $state<MyTasksView | undefined>(undefined);
  let error = $state<string | undefined>(undefined);

  let loadSeq = 0;
  $effect(() => {
    const organizationId = org.currentId;
    const mine = ++loadSeq;
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
</script>

<section class="flex flex-col gap-6" aria-labelledby="my-tasks-heading">
  <h1 id="my-tasks-heading" class="text-xl font-semibold">{m.app_nav_my_tasks()}</h1>
  <p role="status" aria-live="polite" class="text-sm text-muted-foreground empty:hidden">
    {error ?? ''}
  </p>
  {#if data !== undefined}
    <dl class="flex flex-wrap gap-x-6 gap-y-2 text-sm" data-testid="my-tasks-counts">
      {#each BUCKETS as b (b)}
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
    {#each data.workspaces as w (w.workspace_id)}
      <section class="flex flex-col gap-2" aria-labelledby={`my-tasks-${w.workspace_id}`}>
        <h2 id={`my-tasks-${w.workspace_id}`} class="font-semibold">{w.workspace_name}</h2>
        <ul class="flex flex-col divide-y rounded border">
          {#each w.tasks as t (t.id)}
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
                <span class="text-xs text-muted-foreground">{bucketLabel(t.bucket)}</span>
                {#if t.due_date !== null}
                  <time datetime={t.due_date} class="text-xs text-muted-foreground"
                    >{t.due_date}</time
                  >
                {/if}
              </a>
            </li>
          {/each}
        </ul>
      </section>
    {/each}
  {/if}
</section>
