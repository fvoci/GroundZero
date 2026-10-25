<script lang="ts">
  import { untrack } from 'svelte';
  import type { Workspace } from '@fvoci/api-client';
  import { api, type NotificationView, type PageCursor } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import { formatInstant } from '$lib/display';
  import ReceivedInvitations from '$lib/features/members/ReceivedInvitations.svelte';
  import { invitationChanges } from '$lib/features/home/invitations.svelte';
  import { org } from '$lib/org.svelte';
  import { m } from '$lib/paraglide/messages.js';
  import { itemPath, itemTaskPath, notificationPath, pathFor, router } from '$lib/router.svelte';
  import { Button } from '$lib/ui/button';
  import { notificationText } from './notification-text';

  const {
    workspaces,
    onchanged,
    onaccepted = () => Promise.resolve(),
  }: {
    workspaces: Workspace[];
    onchanged: () => void;
    onaccepted?: () => Promise<void>;
  } = $props();
  let items = $state<NotificationView[]>([]);
  let next = $state<PageCursor | null>(null);
  let unread = $state<number | undefined>();
  let loaded = $state(false);
  let loading = $state(false);
  let error = $state<string | undefined>();
  let readError = $state<string | undefined>();
  let marking = $state(false);
  let failedRead = $state<string | null>(null);
  let failedCursor: PageCursor | undefined;
  let generation = 0;
  let attempted: string | undefined;
  const selectedId = $derived(
    router.route.name === 'notifications' ? router.route.notificationId : undefined,
  );
  const selected = $derived(items.find((n) => n.id === selectedId));
  const request = $derived({
    organizationId: org.currentId,
    invitations: invitationChanges.version,
  });

  async function load(cursor?: PageCursor, scope = request): Promise<void> {
    const { organizationId } = scope;
    if (organizationId === undefined) return;
    const ticket = generation;
    loading = true;
    error = undefined;
    failedCursor = cursor;
    try {
      const [page, count] = await Promise.all([
        api.notifications(organizationId, cursor),
        api.unreadCount(organizationId),
      ]);
      if (ticket !== generation) return;
      items =
        cursor === undefined
          ? page.notifications
          : [...items, ...page.notifications.filter((n) => !items.some((i) => i.id === n.id))];
      next = page.next;
      unread = count;
      loaded = true;
    } catch (e: unknown) {
      if (ticket === generation) error = errorText(e, m.references_notifications_load_failed());
    } finally {
      if (ticket === generation) loading = false;
    }
  }

  $effect(() => {
    const scope = request;
    generation += 1;
    items = [];
    next = null;
    unread = undefined;
    loaded = false;
    loading = false;
    error = undefined;
    readError = undefined;
    marking = false;
    attempted = undefined;
    untrack(() => void load(undefined, scope));
    return () => {
      generation += 1;
    };
  });

  async function mark(id: string | null): Promise<void> {
    const organizationId = org.currentId;
    if (organizationId === undefined || marking) return;
    const ticket = generation;
    marking = true;
    readError = undefined;
    failedRead = id;
    try {
      await api.markNotificationsRead(organizationId, id);
      if (ticket !== generation) return;
      items = items.map((n) => (id === null || n.id === id ? { ...n, read: true } : n));
      onchanged();
      // 읽은 수를 빼서 추정하지 않는다. 목록 밖 알림까지 서버가 센다(D-118).
      const count = await api.unreadCount(organizationId);
      if (ticket === generation) unread = count;
    } catch (e: unknown) {
      if (ticket === generation) readError = errorText(e, m.references_notifications_read_failed());
    } finally {
      if (ticket === generation) marking = false;
    }
  }

  $effect(() => {
    const notification = selected;
    if (
      notification !== undefined &&
      !notification.read &&
      !marking &&
      (readError === undefined || failedRead !== notification.id) &&
      attempted !== notification.id
    ) {
      attempted = notification.id;
      untrack(() => void mark(notification.id));
    }
    // 단건 조회 계약이 없으므로 직접 연결된 id를 커서로 찾는다. 없는/철회된 알림은 마지막 페이지에서 안내한다.
    if (
      selectedId !== undefined &&
      notification === undefined &&
      loaded &&
      !loading &&
      error === undefined &&
      next !== null
    ) {
      untrack(() => void load(next ?? undefined));
    }
  });

  function targetPath(n: NotificationView): string | undefined {
    if (n.task_id !== null && n.workspace_id !== null && n.item_id !== null)
      return itemTaskPath(n.workspace_id, n.item_id, n.task_id);
    if (n.document_id !== null)
      return pathFor(
        { id: n.document_id, workspace_id: n.workspace_id, item_id: n.item_id },
        (ws) => workspaces.some((w) => w.id === ws),
      );
    if (n.workspace_id !== null && n.item_id !== null) return itemPath(n.workspace_id, n.item_id);
    return undefined;
  }

  function go(e: MouseEvent, path: string): void {
    if (e.ctrlKey || e.metaKey || e.shiftKey || e.button !== 0) return;
    e.preventDefault();
    router.navigate(path);
  }
</script>

<section class="flex min-w-0 flex-col gap-4" aria-labelledby="notifications-heading">
  <div class="flex flex-wrap items-center gap-2">
    <h1 id="notifications-heading" class="text-xl font-semibold">
      {m.references_notifications_heading()}
    </h1>
    {#if org.current}<span class="text-sm text-muted-foreground">{org.current.name}</span>{/if}
    {#if unread !== undefined}<span class="text-sm"
        >{m.references_unread_count({ count: unread })}</span
      >{/if}
    {#if unread !== undefined && unread > 0}<Button
        size="sm"
        variant="outline"
        disabled={marking}
        onclick={() => void mark(null)}>{m.references_notifications_read_all()}</Button
      >{/if}
  </div>
  <p role="status" class="text-sm text-muted-foreground empty:hidden">
    {org.currentId === undefined
      ? m.app_organization_required()
      : loading
        ? m.references_notifications_loading()
        : ''}
  </p>
  {#if readError !== undefined}<div class="flex flex-wrap items-center gap-2">
      <p role="alert" class="text-sm text-destructive">{readError}</p>
      <Button variant="outline" disabled={marking} onclick={() => void mark(failedRead)}
        >{m.common_retry()}</Button
      >
    </div>{/if}
  {#if error !== undefined}<div>
      <p role="alert" class="text-destructive">{error}</p>
      <Button variant="outline" disabled={loading} onclick={() => void load(failedCursor)}
        >{m.common_retry()}</Button
      >
    </div>{/if}
  {#if org.currentId !== undefined}
    <div class="grid min-w-0 grid-cols-1 gap-4 @2xl:grid-cols-2">
      <div class={[selectedId !== undefined && 'hidden @2xl:block', 'min-w-0']}>
        {#if loaded && items.length === 0}<p class="text-muted-foreground">
            {m.references_notifications_empty()}
          </p>{/if}
        <ul class="flex flex-col gap-1" data-testid="notifications">
          {#each items as n (n.id)}
            <li>
              <a
                class={[
                  'flex flex-wrap gap-2 rounded border p-3 text-sm break-words hover:bg-accent',
                  selectedId === n.id && 'bg-accent',
                ]}
                aria-current={selectedId === n.id ? 'true' : undefined}
                href={notificationPath(n.id)}
                onclick={(e) => {
                  go(e, notificationPath(n.id));
                }}
              >
                {#if !n.read}<span class="font-semibold">{m.references_notifications_unread()}</span
                  >{/if}
                <span class="min-w-0 [overflow-wrap:anywhere]">{notificationText(n)}</span>
                <time datetime={n.created_at} class="text-xs text-muted-foreground"
                  >{formatInstant(n.created_at)}</time
                >
              </a>
            </li>
          {/each}
        </ul>
        {#if next !== null}<Button
            variant="outline"
            disabled={loading}
            onclick={() => void load(next ?? undefined)}>{m.references_load_more()}</Button
          >{/if}
      </div>
      <div class={[selectedId === undefined && 'hidden @2xl:block', 'min-w-0 rounded border p-3']}>
        {#if selectedId !== undefined}<Button
            variant="ghost"
            class="mb-3 @2xl:hidden"
            onclick={() => {
              router.navigate('/notifications');
            }}>{m.references_notifications_back()}</Button
          >{/if}
        {#if selected !== undefined}
          <h2 class="font-semibold [overflow-wrap:anywhere]">{notificationText(selected)}</h2>
          <p class="my-2 text-sm text-muted-foreground">{formatInstant(selected.created_at)}</p>
          {#if selected.excerpt !== null}<p
              class="mb-3 whitespace-pre-wrap [overflow-wrap:anywhere]"
            >
              {selected.excerpt}
            </p>{/if}
          {#if selected.kind === 'invite' && selected.invitation_id !== null}
            <ReceivedInvitations invitationId={selected.invitation_id} {onaccepted} {onchanged} />
          {:else}
            {@const path = targetPath(selected)}
            {#if path !== undefined}<a
                class="underline"
                href={path}
                onclick={(e) => {
                  go(e, path);
                }}>{m.references_notification_open_target()}</a
              >{/if}
          {/if}
        {:else if selectedId !== undefined && loaded && !loading && next === null}
          <p>{m.references_notification_unavailable()}</p>
        {:else if selectedId === undefined}
          <p class="text-muted-foreground">{m.references_notifications_choose()}</p>
        {/if}
      </div>
    </div>
  {/if}
</section>
