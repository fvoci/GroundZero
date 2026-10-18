<script lang="ts">
  // 휴지통 (D-39, D-41): 워크스페이스 휴지통(관리자는 전부, 작성자는 자기가 지운 것) 또는 내 개인 노트 휴지통.
  // 보존 기간이 지나면 되살릴 수 없고 "만료"로 남는다. 영구 삭제는 관리자(개인 노트는 소유자)가 한다.
  import type { Workspace } from '@fvoci/api-client';

  import type { TrashEntryView, TrashView } from '$lib/api/client';
  import { api } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import { formatInstant } from '$lib/display';
  import { notifyDone } from '$lib/features/feedback/feedback.svelte';
  import { m } from '$lib/paraglide/messages.js';
  import { Button } from '$lib/ui/button';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';

  interface Props {
    /** 없으면 내 개인 노트 휴지통. */
    workspace: Workspace | undefined;
  }

  const { workspace }: Props = $props();

  let trash = $state<TrashView | undefined>(undefined);
  let error = $state<string | undefined>(undefined);
  let status = $state('');
  let confirmPurge = $state<string | undefined>(undefined);
  let days = $state('');
  let editingDays = false;
  let versionDays = $state('');
  let editingVersionDays = false;
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
        if (!editingVersionDays)
          versionDays = t.version_retention_days === null ? '' : String(t.version_retention_days);
        error = undefined;
      })
      .catch((e: unknown) => {
        if (mine === seq) error = errorText(e, m.trash_load_failed());
      });
  });

  async function run(action: () => Promise<unknown>, done: string): Promise<void> {
    error = undefined;
    try {
      await action();
      status = done;
      notifyDone(done);
      confirmPurge = undefined;
      reloadKey += 1;
    } catch (e: unknown) {
      error = errorText(e, m.trash_action_failed());
    }
  }

  function kindLabel(kind: TrashEntryView['kind']): string {
    switch (kind) {
      case 'module':
        return m.trash_kind_module();
      case 'item':
        return m.trash_kind_item();
      case 'document':
        return m.trash_kind_document();
      case 'task':
        return m.trash_kind_task();
      case 'event':
        return m.trash_kind_event();
      case 'file':
        return m.trash_kind_file();
    }
  }
  const when = (iso: string): string => formatInstant(iso);
  const expiredCount = $derived(trash?.entries.filter((e) => e.expired).length ?? 0);
  // 만료된 것 일괄 삭제 확인(관리자 직접 삭제는 참조 때문에 남긴 묶음도 지운다, O-34의 기존 정책).
  const EXPIRED = 'expired';
  const keptCount = $derived(
    trash?.entries.filter((e) => e.expired && e.kept_for_references).length ?? 0,
  );
  const label = (e: TrashEntryView): string =>
    e.member_count > 1
      ? m.trash_entry_with_children({
          kind: kindLabel(e.kind),
          title: e.title,
          count: e.member_count,
        })
      : m.trash_entry({ kind: kindLabel(e.kind), title: e.title });
</script>

<section class="flex flex-col gap-4" aria-labelledby="trash-heading">
  <h1 id="trash-heading" class="text-xl font-semibold">
    {workspace === undefined
      ? m.trash_my_heading()
      : m.trash_workspace_heading({ workspace: workspace.name })}
  </h1>
  {#if trash !== undefined}
    <p class="text-sm text-muted-foreground">
      {isAdmin
        ? m.trash_retention_admin({ days: trash.retention_days })
        : m.trash_retention_member({ days: trash.retention_days })}
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
          error = m.trash_retention_invalid();
          return;
        }
        void run(
          () => api.setTrashRetention(workspace.id, n),
          m.trash_retention_changed({ days: n }),
        );
      }}
    >
      <div class="flex flex-col gap-1">
        <Label for="trash-retention">{m.trash_retention_label()}</Label>
        <Input
          id="trash-retention"
          class="w-28"
          inputmode="numeric"
          bind:value={days}
          oninput={() => (editingDays = true)}
        />
      </div>
      <Button type="submit" variant="outline">{m.trash_change()}</Button>
    </form>
    <form
      class="flex flex-wrap items-end gap-2"
      aria-label={m.trash_version_retention_region()}
      onsubmit={(e) => {
        e.preventDefault();
        const raw = versionDays.trim();
        const n = raw === '' ? null : Number(raw);
        if (n !== null && (!Number.isInteger(n) || n < 1 || n > 3650)) {
          error = m.trash_version_retention_invalid();
          return;
        }
        void run(
          () => api.setVersionRetention(workspace.id, n),
          n === null ? m.trash_versions_forever() : m.trash_versions_days({ days: n }),
        );
      }}
    >
      <div class="flex flex-col gap-1">
        <Label for="version-retention">{m.trash_version_retention_label()}</Label>
        <Input
          id="version-retention"
          class="w-28"
          inputmode="numeric"
          bind:value={versionDays}
          oninput={() => (editingVersionDays = true)}
        />
      </div>
      <Button type="submit" variant="outline">{m.trash_change()}</Button>
      <p class="w-full text-sm text-muted-foreground">
        {m.trash_version_retention_hint()}
      </p>
    </form>
  {/if}

  {#if isAdmin && expiredCount > 0}
    <div class="flex flex-wrap items-center gap-2 text-sm">
      {#if confirmPurge === EXPIRED}
        <span role="status">
          {m.trash_irreversible()}
          {#if keptCount > 0}
            {m.trash_purge_expired_kept({ count: keptCount })}
          {/if}
          {m.trash_purge_expired_question({ count: expiredCount })}
        </span>
        <Button
          variant="destructive"
          size="sm"
          onclick={() =>
            void run(
              () => api.purgeExpiredTrash(workspace?.id ?? null),
              m.trash_purged_expired({ count: expiredCount }),
            )}>{m.trash_purge()}</Button
        >
        <Button size="sm" variant="ghost" onclick={() => (confirmPurge = undefined)}
          >{m.common_cancel()}</Button
        >
      {:else}
        <Button variant="destructive" size="sm" onclick={() => (confirmPurge = EXPIRED)}
          >{m.trash_purge_expired_button({ count: expiredCount })}</Button
        >
      {/if}
    </div>
  {/if}

  <ul class="flex flex-col gap-2" data-testid="trash-list">
    {#each trash?.entries ?? [] as e (e.id)}
      <li class="flex flex-wrap items-center gap-2 rounded-md border p-2 text-sm">
        <span class="min-w-0 flex-1">
          {label(e)}
          <span class="block text-xs text-muted-foreground">
            {e.deleted_by_name ?? ''} · {m.trash_deleted_at({ when: when(e.deleted_at) })} ·
            {e.expired ? m.trash_expired() : m.trash_until({ when: when(e.expires_at) })}
          </span>
          {#if e.kept_for_references}
            <span class="block text-xs text-muted-foreground">
              {m.trash_kept_hint()}
            </span>
          {/if}
        </span>
        {#if !e.expired || e.kept_for_references}
          <Button
            size="sm"
            variant="outline"
            onclick={() =>
              void run(() => api.restoreTrash(e.id), m.trash_restored({ label: label(e) }))}
            >{m.trash_restore()}</Button
          >
        {/if}
        {#if e.can_purge}
          {#if confirmPurge === e.id}
            <span role="status"
              >{e.kept_for_references
                ? m.trash_purge_question_kept()
                : m.trash_purge_question()}</span
            >
            <Button
              size="sm"
              variant="destructive"
              onclick={() =>
                void run(() => api.purgeTrash(e.id), m.trash_purged({ label: label(e) }))}
              >{m.trash_purge()}</Button
            >
            <Button size="sm" variant="ghost" onclick={() => (confirmPurge = undefined)}
              >{m.common_cancel()}</Button
            >
          {:else}
            <Button size="sm" variant="ghost" onclick={() => (confirmPurge = e.id)}
              >{m.trash_purge_ellipsis()}</Button
            >
          {/if}
        {/if}
      </li>
    {:else}
      <li class="text-sm text-muted-foreground">{m.trash_empty()}</li>
    {/each}
  </ul>
</section>
