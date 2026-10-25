<script lang="ts">
  import { untrack } from 'svelte';
  import type { Activity } from '@fvoci/api-client';
  import { api, type PageCursor } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import { formatInstant } from '$lib/display';
  import { org } from '$lib/org.svelte';
  import { m } from '$lib/paraglide/messages.js';
  import { router } from '$lib/router.svelte';
  import { Button } from '$lib/ui/button';
  import { activityPath, activityText } from './activity';

  const {
    kind,
    targetId,
    workspaceId,
    version = 0,
  }: {
    kind?: 'item' | 'document' | 'task';
    targetId?: string;
    workspaceId?: string;
    version?: number;
  } = $props();
  let items = $state<Activity[]>([]);
  let next = $state<PageCursor | undefined>();
  let loading = $state(false);
  let error = $state<string | undefined>();
  let sequence = 0;
  let failedCursor: PageCursor | undefined;
  const request = $derived({ organizationId: org.currentId, kind, targetId, version });

  async function load(cursor?: PageCursor, scope = request): Promise<void> {
    const { organizationId, kind, targetId } = scope;
    if (organizationId === undefined) return;
    const ticket = ++sequence;
    loading = true;
    error = undefined;
    failedCursor = cursor;
    try {
      if (kind !== undefined && targetId !== undefined) {
        const page = await api.targetActivity(kind, targetId, cursor);
        if (ticket !== sequence) return;
        items =
          cursor === undefined
            ? page.activity
            : [...items, ...page.activity.filter((a) => !items.some((b) => a.id === b.id))];
        next =
          page.next_before != null && page.next_before_id != null
            ? { before: page.next_before, before_id: page.next_before_id }
            : undefined;
      } else {
        const recent = await api.recentActivity(organizationId);
        if (ticket !== sequence) return;
        items = recent;
        next = undefined;
      }
    } catch (e: unknown) {
      if (ticket === sequence) error = errorText(e, m.home_activity_failed());
    } finally {
      if (ticket === sequence) loading = false;
    }
  }

  $effect(() => {
    const scope = request;
    items = [];
    next = undefined;
    error = undefined;
    loading = false;
    untrack(() => void load(undefined, scope));
    return () => {
      sequence += 1;
    };
  });
</script>

<div class="flex min-w-0 flex-col gap-3">
  <p role="status" class="text-sm text-muted-foreground empty:hidden">
    {org.currentId === undefined ? m.app_organization_required() : loading ? m.app_loading() : ''}
  </p>
  {#if error !== undefined}
    <div>
      <p role="alert" class="text-sm text-destructive">{error}</p>
      <Button variant="outline" disabled={loading} onclick={() => void load(failedCursor)}
        >{m.common_retry()}</Button
      >
    </div>
  {/if}
  <ul class="flex flex-col divide-y" aria-label={m.home_activity_heading()}>
    {#each items as activity (activity.id)}
      {@const href = activityPath(activity, workspaceId)}
      <li class="flex flex-wrap justify-between gap-2 py-2 text-sm [overflow-wrap:anywhere]">
        {#if href !== undefined}<a
            class="min-w-0 underline"
            {href}
            onclick={(e) => {
              if (e.ctrlKey || e.metaKey || e.shiftKey || e.button !== 0) return;
              e.preventDefault();
              router.navigate(href);
            }}>{activityText(activity)}</a
          >{:else}<span>{activityText(activity)}</span>{/if}
        <time class="text-xs text-muted-foreground" datetime={activity.created_at}
          >{formatInstant(activity.created_at)}</time
        >
      </li>
    {/each}
  </ul>
  {#if org.currentId !== undefined && !loading && error === undefined && items.length === 0}<p
      class="text-sm text-muted-foreground"
    >
      {m.home_activity_empty()}
    </p>{/if}
  {#if next !== undefined}<Button
      variant="outline"
      disabled={loading}
      onclick={() => void load(next)}>{m.references_load_more()}</Button
    >{/if}
</div>
