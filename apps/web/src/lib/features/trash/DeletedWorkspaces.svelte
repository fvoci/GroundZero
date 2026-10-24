<script lang="ts">
  // 내가 소유한 지운 워크스페이스(D-51). 보존 기간 안에 되살리거나 영구 삭제한다.
  import { tick } from 'svelte';

  import type { DeletedWorkspaceView } from '$lib/api/client';
  import { api } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import { formatInstant } from '$lib/display';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';

  interface Props {
    /** 되살린 뒤 사이드바의 워크스페이스 목록을 다시 읽는다. */
    onchanged: () => void;
  }

  const { onchanged }: Props = $props();

  let list = $state<DeletedWorkspaceView[]>([]);
  let error = $state<string | undefined>(undefined);
  let status = $state('');
  let confirmPurge = $state<string | undefined>(undefined);
  let reloadKey = $state(0);
  let seq = 0;

  $effect(() => {
    // reloadKey를 읽어 되살리기·영구 삭제 뒤 다시 불러온다.
    if (reloadKey < 0) return;
    const mine = ++seq;
    api
      .deletedWorkspaces()
      .then((found) => {
        if (mine === seq) list = found;
      })
      .catch((e: unknown) => {
        if (mine === seq) error = errorText(e, m.trash_workspaces_load_failed());
      });
  });

  async function run(action: () => Promise<unknown>, done: string): Promise<void> {
    error = undefined;
    try {
      await action();
      status = done;
      confirmPurge = undefined;
      reloadKey += 1;
      onchanged();
      // 누른 단추가 목록에서 사라지므로 초점을 제목으로 옮긴다.
      await tick();
      document.getElementById('deleted-workspaces-heading')?.focus();
    } catch (e: unknown) {
      error = errorText(e, m.trash_action_failed());
    }
  }

  async function askPurge(id: string): Promise<void> {
    confirmPurge = id;
    await tick();
    document.getElementById(`purge-cancel-${id}`)?.focus();
  }

  const when = (iso: string): string => formatInstant(iso);
</script>

{#if list.length > 0 || error !== undefined || status !== ''}
  <section class="flex flex-col gap-3" aria-labelledby="deleted-workspaces-heading">
    <h2 id="deleted-workspaces-heading" class="font-semibold" tabindex="-1">
      {m.trash_workspaces_heading()}
    </h2>
    <p class="text-sm text-muted-foreground">
      {m.trash_workspaces_hint()}
    </p>
    <!-- 오류는 따로 알리고(alert), 완료 알림은 처음부터 있는 polite 영역에서 바뀐다(알림 영역을 겹치지 않는다). -->
    {#if error !== undefined}
      <p role="alert" class="text-sm text-destructive">{error}</p>
    {/if}
    <p aria-live="polite" class="text-sm empty:sr-only">{error === undefined ? status : ''}</p>
    <ul class="flex flex-col gap-2" data-testid="deleted-workspaces">
      {#each list as w (w.id)}
        <li class="flex flex-wrap items-center gap-2 rounded-md border p-2 text-sm">
          <span class="min-w-0 flex-1">
            {w.name}
            <span class="block text-xs text-muted-foreground">
              {m.trash_deleted_at({ when: when(w.deleted_at) })} ·
              {w.expired ? m.trash_expired() : m.trash_until({ when: when(w.expires_at) })}
            </span>
          </span>
          {#if !w.expired}
            <Button
              size="sm"
              variant="outline"
              aria-label={m.trash_restore_named({ name: w.name })}
              onclick={() =>
                void run(() => api.restoreWorkspace(w.id), m.trash_restored({ label: w.name }))}
              >{m.trash_restore()}</Button
            >
          {/if}
          {#if confirmPurge === w.id}
            <span>{m.trash_workspace_purge_question()}</span>
            <Button
              size="sm"
              variant="destructive"
              aria-label={m.trash_purge_named({ name: w.name })}
              onclick={() =>
                void run(() => api.purgeWorkspace(w.id), m.trash_purged({ label: w.name }))}
              >{m.trash_purge()}</Button
            >
            <Button
              id={`purge-cancel-${w.id}`}
              size="sm"
              variant="ghost"
              onclick={() => (confirmPurge = undefined)}>{m.common_cancel()}</Button
            >
          {:else}
            <Button
              size="sm"
              variant="ghost"
              aria-label={m.trash_purge_named_ellipsis({ name: w.name })}
              onclick={() => void askPurge(w.id)}>{m.trash_purge_ellipsis()}</Button
            >
          {/if}
        </li>
      {/each}
    </ul>
  </section>
{/if}
