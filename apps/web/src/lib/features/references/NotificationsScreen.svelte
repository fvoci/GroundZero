<script lang="ts">
  // 내 알림. 지금 볼 수 있는 문서에 대한 것만 서버가 돌려준다 (D-39).
  import { formatInstant } from '$lib/display';
  import type { Workspace } from '@fvoci/api-client';

  import type { NotificationView } from '$lib/api/client';
  import { api } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import { m } from '$lib/paraglide/messages.js';
  import { itemPath, itemTaskPath, pathFor, router } from '$lib/router.svelte';
  import { Button } from '$lib/ui/button';

  interface Props {
    workspaces: Workspace[];
    /** 읽음 상태가 바뀌면 사이드바 개수를 다시 센다. */
    onchanged: () => void;
  }

  const { workspaces, onchanged }: Props = $props();

  let items = $state<NotificationView[] | undefined>(undefined);
  let error = $state<string | undefined>(undefined);

  async function load(): Promise<void> {
    try {
      items = await api.notifications();
      error = undefined;
    } catch (e: unknown) {
      error = errorText(e, m.references_notifications_load_failed());
    }
  }

  void load();

  const memberOf = (ws: string): boolean => workspaces.some((w) => w.id === ws);
  const unread = $derived(items?.filter((n) => !n.read).length ?? 0);

  async function open(n: NotificationView): Promise<void> {
    if (!n.read) {
      await api.markNotificationsRead(n.id).catch(() => undefined);
      onchanged();
    }
    if (n.task_id !== null) {
      if (n.workspace_id !== null && n.item_id !== null)
        router.navigate(itemTaskPath(n.workspace_id, n.item_id, n.task_id));
      return;
    }
    if (n.document_id !== null)
      router.navigate(
        pathFor({ id: n.document_id, workspace_id: n.workspace_id, item_id: n.item_id }, memberOf),
      );
    else if (n.workspace_id !== null && n.item_id !== null)
      router.navigate(itemPath(n.workspace_id, n.item_id));
  }

  /** 알림 문장(종류별). */
  function message(n: NotificationView): string {
    const who = n.actor_name ?? m.references_notification_someone();
    const target = `${n.document_reference ?? ''} ${n.document_title}`.trim();
    const date = n.due_date ?? '';
    switch (n.kind) {
      case 'mention':
        return m.references_notification_mention({ who, target });
      case 'comment':
        return m.references_notification_comment({ who, target });
      case 'reply':
        return m.references_notification_reply({ who, target });
      case 'due':
        return m.references_notification_due({ target, date });
      case 'expiry':
        return m.references_notification_expiry({ target, date });
      case 'review':
        return m.references_notification_review({
          target,
          subject: n.subject ?? m.references_notification_review_subject(),
          date,
        });
      case 'deadline':
        return m.references_notification_deadline({ target, subject: n.subject ?? '', date });
      default:
        return target;
    }
  }

  async function readAll(): Promise<void> {
    await api.markNotificationsRead(null);
    await load();
    onchanged();
  }
</script>

<section class="flex flex-col gap-4" aria-labelledby="notifications-heading">
  <div class="flex items-center gap-2">
    <h1 id="notifications-heading" class="text-xl font-semibold">
      {m.references_notifications_heading()}
    </h1>
    {#if unread > 0}
      <Button size="sm" variant="outline" class="ml-auto" onclick={() => void readAll()}
        >{m.references_notifications_read_all()}</Button
      >
    {/if}
  </div>
  {#if error !== undefined}
    <p role="alert" class="text-destructive">{error}</p>
  {:else if items === undefined}
    <p aria-live="polite">{m.references_notifications_loading()}</p>
  {:else if items.length === 0}
    <p class="text-muted-foreground">{m.references_notifications_empty()}</p>
  {:else}
    <ul class="flex flex-col gap-1" data-testid="notifications">
      {#each items as n (n.id)}
        <li>
          <button
            type="button"
            class="flex w-full items-baseline gap-2 rounded px-2 py-1.5 text-left hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
            onclick={() => void open(n)}
          >
            {#if !n.read}<span class="sr-only">{m.references_notifications_unread()}</span><span
                aria-hidden="true"
                class="size-2 shrink-0 self-center rounded-full bg-primary"
              ></span>{/if}
            <span class="flex min-w-0 flex-col">
              <span>{message(n)}</span>
              {#if n.excerpt !== null}
                <span class="truncate text-sm text-muted-foreground">“{n.excerpt}”</span>
              {/if}
            </span>
            <span class="ml-auto shrink-0 text-sm text-muted-foreground"
              >{formatInstant(n.created_at)}</span
            >
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</section>
