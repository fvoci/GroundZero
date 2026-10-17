<script lang="ts">
  // 휴지통 (D-39, D-41): 워크스페이스 휴지통(관리자는 전부, 작성자는 자기가 지운 것) 또는 내 개인 노트 휴지통.
  // 보존 기간이 지나면 되살릴 수 없고 "만료"로 남는다. 영구 삭제는 관리자(개인 노트는 소유자)가 한다.
  import type { Workspace } from '@fvoci/api-client';

  import type { TrashEntryView, TrashView } from '$lib/api/client';
  import { api, ApiError } from '$lib/api/client';
  import { localTimeZone, wallTime } from '$lib/features/events/time';
  import { Button } from '$lib/ui/button';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';

  interface Props {
    /** 없으면 내 개인 노트 휴지통. */
    workspace: Workspace | undefined;
  }

  const { workspace }: Props = $props();

  const zone = localTimeZone();
  let trash = $state<TrashView | undefined>(undefined);
  let error = $state<string | undefined>(undefined);
  let status = $state('');
  let confirmPurge = $state<string | undefined>(undefined);
  let days = $state('');
  let editingDays = false;
  let reloadKey = $state(0);

  const isAdmin = $derived(workspace === undefined || workspace.role === 'admin');
  const request = $derived({ ws: workspace?.id ?? null, key: reloadKey });
  let seq = 0;

  $effect(() => {
    const { ws } = request;
    const mine = ++seq;
    api
      .trash(ws)
      .then((t) => {
        if (mine !== seq) return;
        trash = t;
        // 입력 중인 보존 기간을 다시 불러온 값으로 덮지 않는다(처음과 저장한 뒤에만 채운다).
        if (days === '' || !editingDays) days = String(t.retention_days);
        error = undefined;
      })
      .catch((e: unknown) => {
        if (mine === seq)
          error = e instanceof ApiError ? e.message : '휴지통을 불러오지 못했습니다.';
      });
  });

  async function run(action: () => Promise<unknown>, done: string): Promise<void> {
    error = undefined;
    try {
      await action();
      status = done;
      confirmPurge = undefined;
      reloadKey += 1;
    } catch (e: unknown) {
      error = e instanceof ApiError ? e.message : '처리하지 못했습니다.';
    }
  }

  const KIND = { item: '항목', document: '문서', task: '업무', event: '일정' } as const;
  const when = (iso: string): string => wallTime(iso, zone).replace('T', ' ');
  const expiredCount = $derived(trash?.entries.filter((e) => e.expired).length ?? 0);
  const label = (e: TrashEntryView): string =>
    `${KIND[e.kind]} “${e.title}”${e.member_count > 1 ? `(하위 포함 ${String(e.member_count)}개)` : ''}`;
</script>

<section class="flex flex-col gap-4" aria-labelledby="trash-heading">
  <h1 id="trash-heading" class="text-xl font-semibold">
    {workspace === undefined ? '내 노트 휴지통' : `${workspace.name} 휴지통`}
  </h1>
  {#if trash !== undefined}
    <p class="text-sm text-muted-foreground">
      지운 날부터 {trash.retention_days}일 동안 되살릴 수 있습니다. 기간이 지난 것은 되살릴 수 없고
      {isAdmin ? '영구 삭제할 수 있습니다' : '관리자가 영구 삭제합니다'}.
    </p>
  {/if}
  {#if error !== undefined}
    <p role="alert" class="text-sm text-destructive">{error}</p>
  {/if}
  <p class="sr-only" aria-live="polite">{status}</p>

  {#if workspace !== undefined && isAdmin}
    <form
      class="flex flex-wrap items-end gap-2"
      onsubmit={(e) => {
        e.preventDefault();
        const n = Number(days.trim());
        if (!Number.isInteger(n) || n < 1 || n > 3650) {
          error = '보존 기간은 1~3650일입니다.';
          return;
        }
        void run(
          () => api.setTrashRetention(workspace.id, n),
          `보존 기간을 ${String(n)}일로 바꿨습니다.`,
        );
      }}
    >
      <div class="flex flex-col gap-1">
        <Label for="trash-retention">보존 기간(일)</Label>
        <Input
          id="trash-retention"
          class="w-28"
          inputmode="numeric"
          bind:value={days}
          oninput={() => (editingDays = true)}
        />
      </div>
      <Button type="submit" variant="outline">바꾸기</Button>
    </form>
  {/if}

  {#if isAdmin && expiredCount > 0}
    <div>
      <Button
        variant="destructive"
        size="sm"
        onclick={() =>
          void run(
            () => api.purgeExpiredTrash(workspace?.id ?? null),
            `만료된 ${String(expiredCount)}개를 영구 삭제했습니다.`,
          )}>만료된 {expiredCount}개 영구 삭제</Button
      >
    </div>
  {/if}

  <ul class="flex flex-col gap-2" data-testid="trash-list">
    {#each trash?.entries ?? [] as e (e.id)}
      <li class="flex flex-wrap items-center gap-2 rounded-md border p-2 text-sm">
        <span class="min-w-0 flex-1">
          {label(e)}
          <span class="block text-xs text-muted-foreground">
            {e.deleted_by_name ?? ''} · {when(e.deleted_at)}에 지움 ·
            {e.expired ? '만료됨' : `${when(e.expires_at)}까지`}
          </span>
        </span>
        {#if !e.expired}
          <Button
            size="sm"
            variant="outline"
            onclick={() => void run(() => api.restoreTrash(e.id), `되살렸습니다: ${label(e)}`)}
            >되살리기</Button
          >
        {/if}
        {#if e.can_purge}
          {#if confirmPurge === e.id}
            <span role="status">되돌릴 수 없습니다. 영구 삭제할까요?</span>
            <Button
              size="sm"
              variant="destructive"
              onclick={() => void run(() => api.purgeTrash(e.id), `영구 삭제했습니다: ${label(e)}`)}
              >영구 삭제</Button
            >
            <Button size="sm" variant="ghost" onclick={() => (confirmPurge = undefined)}
              >취소</Button
            >
          {:else}
            <Button size="sm" variant="ghost" onclick={() => (confirmPurge = e.id)}
              >영구 삭제…</Button
            >
          {/if}
        {/if}
      </li>
    {:else}
      <li class="text-sm text-muted-foreground">휴지통이 비어 있습니다.</li>
    {/each}
  </ul>
</section>
