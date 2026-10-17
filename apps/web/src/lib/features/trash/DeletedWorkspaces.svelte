<script lang="ts">
  // 내가 소유한 지운 워크스페이스(D-51). 보존 기간 안에 되살리거나 영구 삭제한다.
  import { tick } from 'svelte';

  import type { DeletedWorkspaceView } from '$lib/api/client';
  import { api, ApiError } from '$lib/api/client';
  import { localTimeZone, wallTime } from '$lib/features/events/time';
  import { Button } from '$lib/ui/button';

  interface Props {
    /** 되살린 뒤 사이드바의 워크스페이스 목록을 다시 읽는다. */
    onchanged: () => void;
  }

  const { onchanged }: Props = $props();

  const zone = localTimeZone();
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
        if (mine === seq)
          error = e instanceof ApiError ? e.message : '지운 워크스페이스를 불러오지 못했습니다.';
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
      error = e instanceof ApiError ? e.message : '처리하지 못했습니다.';
    }
  }

  async function askPurge(id: string): Promise<void> {
    confirmPurge = id;
    await tick();
    document.getElementById(`purge-cancel-${id}`)?.focus();
  }

  const when = (iso: string): string => wallTime(iso, zone).replace('T', ' ');
</script>

{#if list.length > 0 || error !== undefined || status !== ''}
  <section class="flex flex-col gap-3" aria-labelledby="deleted-workspaces-heading">
    <h2 id="deleted-workspaces-heading" class="font-semibold" tabindex="-1">지운 워크스페이스</h2>
    <p class="text-sm text-muted-foreground">
      내가 소유한 워크스페이스입니다. 보존 기간 안에는 모든 내용과 멤버가 그대로 돌아옵니다. 영구
      삭제하면 모든 내용이 지워지며 되돌릴 수 없습니다(각자의 개인 시간 기록은 남습니다).
    </p>
    <p aria-live="polite" class="text-sm">
      {#if error !== undefined}<span class="text-destructive">{error}</span>{:else}{status}{/if}
    </p>
    <ul class="flex flex-col gap-2" data-testid="deleted-workspaces">
      {#each list as w (w.id)}
        <li class="flex flex-wrap items-center gap-2 rounded-md border p-2 text-sm">
          <span class="min-w-0 flex-1">
            {w.name}
            <span class="block text-xs text-muted-foreground">
              {when(w.deleted_at)}에 지움 · {w.expired ? '만료됨' : `${when(w.expires_at)}까지`}
            </span>
          </span>
          {#if !w.expired}
            <Button
              size="sm"
              variant="outline"
              aria-label={`${w.name} 되살리기`}
              onclick={() => void run(() => api.restoreWorkspace(w.id), `되살렸습니다: ${w.name}`)}
              >되살리기</Button
            >
          {/if}
          {#if confirmPurge === w.id}
            <span>모든 내용이 지워집니다. 영구 삭제할까요?</span>
            <Button
              size="sm"
              variant="destructive"
              aria-label={`${w.name} 영구 삭제`}
              onclick={() =>
                void run(() => api.purgeWorkspace(w.id), `영구 삭제했습니다: ${w.name}`)}
              >영구 삭제</Button
            >
            <Button
              id={`purge-cancel-${w.id}`}
              size="sm"
              variant="ghost"
              onclick={() => (confirmPurge = undefined)}>취소</Button
            >
          {:else}
            <Button
              size="sm"
              variant="ghost"
              aria-label={`${w.name} 영구 삭제…`}
              onclick={() => void askPurge(w.id)}>영구 삭제…</Button
            >
          {/if}
        </li>
      {/each}
    </ul>
  </section>
{/if}
