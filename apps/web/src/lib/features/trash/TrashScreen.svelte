<script lang="ts">
  // 휴지통 (D-39, D-41): 워크스페이스 휴지통(관리자는 전부, 작성자는 자기가 지운 것) 또는 내 개인 노트 휴지통.
  // 보존 기간이 지나면 되살릴 수 없고 "만료"로 남는다. 영구 삭제는 관리자(개인 노트는 소유자)가 한다.
  import type { Workspace } from '@fvoci/api-client';
  import { onMount, untrack } from 'svelte';

  import type { TrashEntryView, TrashView } from '$lib/api/client';
  import { api } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import { formatInstant } from '$lib/display';
  import DiscardChangesConfirm from '$lib/features/common/DiscardChangesConfirm.svelte';
  import FormDraftOffer from '$lib/features/common/FormDraftOffer.svelte';
  import { createFormDraft } from '$lib/features/common/form-draft.svelte';
  import { focusConfirm, focusConfirmStart } from '$lib/features/common/inline-confirm';
  import { notifyDone } from '$lib/features/feedback/feedback.svelte';
  import { workspaceNav } from '$lib/features/nav/workspace-nav.svelte';
  import { me } from '$lib/me.svelte';
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
  let editingDays = $state(false);
  // undefined는 아직 모르는 서버 값이다. 빈 문자열 초안을 서버와 같다고 지우지 않는다.
  let savedDays = $state<string | undefined>();
  let versionDays = $state('');
  let editingVersionDays = $state(false);
  let savedVersionDays = $state<string | undefined>();
  // 보존 기간 칸의 입력·저장 오류는 그 칸 옆에 보인다(D-82).
  let daysError = $state<string | undefined>(undefined);
  let versionDaysError = $state<string | undefined>(undefined);
  // 칸마다 마지막으로 낸 저장 번호. 앞서 낸 저장의 늦은 실패가 그 뒤 저장(성공했거나 입력 오류로 막힌 것)의 칸을
  // 덮지 않게 한다.
  let daysSeq = 0;
  let versionDaysSeq = 0;
  let reloadKey = $state(0);

  const isAdmin = $derived(workspace === undefined || workspace.access === 'owner');
  const context = $derived({ ws: workspace?.id ?? null, userId: me.userId, admin: isAdmin });
  const request = $derived({ context, key: reloadKey });
  let loadedContext: typeof context | undefined;
  let seq = 0;

  $effect(() => {
    const { context: owner } = request;
    // 부모의 같은 워크스페이스 객체 교체는 편집 세션 전환이 아니다.
    if (
      loadedContext?.ws !== owner.ws ||
      loadedContext.userId !== owner.userId ||
      loadedContext.admin !== owner.admin
    ) {
      loadedContext = owner;
      untrack(() => {
        trash = undefined;
        savedDays = savedVersionDays = undefined;
        days = versionDays = '';
        editingDays = editingVersionDays = false;
        daysError = versionDaysError = error = undefined;
        status = '';
        confirmPurge = undefined;
      });
    }
    const mine = ++seq;
    api
      .trash(owner.ws)
      .then((t) => {
        if (mine !== seq || owner !== context) return;
        trash = t;
        // 입력 중인 보존 기간을 다시 불러온 값으로 덮지 않는다(처음과 저장한 뒤에만 채운다).
        savedDays = String(t.retention_days);
        savedVersionDays =
          t.version_retention_days === null ? '' : String(t.version_retention_days);
        if (!editingDays) days = savedDays;
        if (!editingVersionDays) versionDays = savedVersionDays;
        error = undefined;
      })
      .catch((e: unknown) => {
        if (mine === seq && owner === context) error = errorText(e, m.trash_load_failed());
      });
    return () => {
      seq += 1;
    };
  });

  // 제출 전의 원문만 보존한다. 두 설정은 각각 저장하므로 초안·성공 처리도 서로 독립적이다(⑱C).
  const daysDraft = createFormDraft({
    name: () =>
      workspace !== undefined && isAdmin ? `trash-retention:${workspace.id}` : undefined,
    snapshot: () => days,
    parse: (raw) => (typeof raw === 'string' ? raw : undefined),
    restore: (value) => {
      days = value;
      editingDays = true;
      daysError = undefined;
    },
    reset: () => {
      days = savedDays ?? '';
      editingDays = false;
      daysError = undefined;
    },
    dirty: () => editingDays && days !== savedDays,
  });
  const versionDraft = createFormDraft({
    name: () =>
      workspace !== undefined && isAdmin ? `trash-version-retention:${workspace.id}` : undefined,
    snapshot: () => versionDays,
    parse: (raw) => (typeof raw === 'string' ? raw : undefined),
    restore: (value) => {
      versionDays = value;
      editingVersionDays = true;
      versionDaysError = undefined;
    },
    reset: () => {
      versionDays = savedVersionDays ?? '';
      editingVersionDays = false;
      versionDaysError = undefined;
    },
    dirty: () => editingVersionDays && versionDays !== savedVersionDays,
  });
  onMount(daysDraft.attach);
  onMount(versionDraft.attach);

  async function saveRetention(version: boolean): Promise<void> {
    const draft = version ? versionDraft : daysDraft;
    const ws = workspace?.id;
    if (ws === undefined || !isAdmin || !draft.active || draft.blocked) return;
    const mine = version ? ++versionDaysSeq : ++daysSeq;
    const raw = (version ? versionDays : days).trim();
    const n = version && raw === '' ? null : Number(raw);
    if (n !== null && (!Number.isInteger(n) || n < 1 || n > 3650)) {
      if (version) versionDaysError = m.trash_version_retention_invalid();
      else daysError = m.trash_retention_invalid();
      return;
    }
    const attempt = draft.capture();
    if (attempt === undefined) return;
    if (version) versionDaysError = undefined;
    else daysError = undefined;
    const current = (): boolean =>
      draft.isOwner(attempt) && mine === (version ? versionDaysSeq : daysSeq);
    try {
      if (version) await api.setVersionRetention(ws, n);
      else if (n !== null) await api.setTrashRetention(ws, n);
      if (!current()) return;
      if (version) savedVersionDays = n === null ? '' : String(n);
      else savedDays = String(n);
      // 제출했던 입력만 처리한다. 저장 중 추가로 친 원문은 그대로 보존한다.
      draft.complete(attempt);
      draft.persist();
      status =
        n === null
          ? m.trash_versions_forever()
          : version
            ? m.trash_versions_days({ days: n })
            : m.trash_retention_changed({ days: n });
      notifyDone(status);
      reloadKey += 1;
    } catch (e: unknown) {
      if (!current() || !draft.isCurrent(attempt)) return;
      const text = errorText(e, m.trash_action_failed());
      if (version) versionDaysError = text;
      else daysError = text;
    }
  }

  /** `fail`이 있으면 실패 문구를 화면 위 공용 알림 대신 그쪽(칸 옆)에 보인다. */
  async function run(
    action: () => Promise<unknown>,
    done: string,
    fail?: (text: string) => void,
  ): Promise<void> {
    error = undefined;
    try {
      await action();
      status = done;
      notifyDone(done);
      confirmPurge = undefined;
      reloadKey += 1;
      // 되살리거나 지운 것이 사이드바의 모듈·위키 목록에 바로 보이게 한다.
      workspaceNav.refreshModules();
      workspaceNav.refreshDocs();
    } catch (e: unknown) {
      const text = errorText(e, m.trash_action_failed());
      if (fail === undefined) error = text;
      else fail(text);
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
    <p role="alert" class="text-sm text-destructive">
      {error}
      {#if trash === undefined}
        <Button
          size="sm"
          variant="outline"
          class="ml-2"
          onclick={() => {
            error = undefined;
            reloadKey += 1;
          }}>{m.common_retry()}</Button
        >
      {/if}
    </p>
  {/if}
  <p class="sr-only" aria-live="polite">{status}</p>

  {#if workspace !== undefined && isAdmin}
    <form
      class="flex flex-wrap items-end gap-2"
      onsubmit={(e) => {
        e.preventDefault();
        void saveRetention(false);
      }}
    >
      {#each daysDraft.offers as entry (entry.key)}
        <FormDraftOffer
          onrestore={() => {
            daysDraft.restore(entry);
          }}
          ondiscard={() => {
            daysDraft.discard(entry);
          }}
        />
      {/each}
      <div class="flex flex-col gap-1">
        <Label for="trash-retention">{m.trash_retention_label()}</Label>
        <Input
          id="trash-retention"
          class="w-28"
          inputmode="numeric"
          bind:value={days}
          disabled={!daysDraft.active}
          oninput={() => (editingDays = true)}
          aria-invalid={daysError !== undefined}
          aria-describedby={daysError !== undefined ? 'trash-retention-error' : undefined}
        />
      </div>
      <Button type="submit" variant="outline" disabled={!daysDraft.active || daysDraft.blocked}
        >{m.trash_change()}</Button
      >
      {#if daysError !== undefined}
        <p id="trash-retention-error" role="alert" class="w-full text-sm text-destructive">
          {daysError}
        </p>
      {/if}
    </form>
    <form
      class="flex flex-wrap items-end gap-2"
      aria-label={m.trash_version_retention_region()}
      onsubmit={(e) => {
        e.preventDefault();
        void saveRetention(true);
      }}
    >
      {#each versionDraft.offers as entry (entry.key)}
        <FormDraftOffer
          onrestore={() => {
            versionDraft.restore(entry);
          }}
          ondiscard={() => {
            versionDraft.discard(entry);
          }}
        />
      {/each}
      <div class="flex flex-col gap-1">
        <Label for="version-retention">{m.trash_version_retention_label()}</Label>
        <Input
          id="version-retention"
          class="w-28"
          inputmode="numeric"
          bind:value={versionDays}
          disabled={!versionDraft.active}
          oninput={() => (editingVersionDays = true)}
          aria-invalid={versionDaysError !== undefined}
          aria-describedby={versionDaysError !== undefined ? 'version-retention-error' : undefined}
        />
      </div>
      <Button
        type="submit"
        variant="outline"
        disabled={!versionDraft.active || versionDraft.blocked}>{m.trash_change()}</Button
      >
      {#if versionDaysError !== undefined}
        <p id="version-retention-error" role="alert" class="w-full text-sm text-destructive">
          {versionDaysError}
        </p>
      {/if}
      <p class="w-full text-sm text-muted-foreground">
        {m.trash_version_retention_hint()}
      </p>
    </form>
  {/if}

  <DiscardChangesConfirm
    open={daysDraft.leave.open}
    onkeep={daysDraft.leave.keep}
    ondiscard={daysDraft.discardAndLeave}
  />
  <DiscardChangesConfirm
    open={versionDraft.leave.open}
    onkeep={versionDraft.leave.keep}
    ondiscard={versionDraft.discardAndLeave}
  />

  {#if isAdmin && expiredCount > 0}
    <div class="flex flex-wrap items-center gap-2 text-sm">
      {#if confirmPurge === EXPIRED}
        <span id="trash-purge-expired-question">
          {m.trash_irreversible()}
          {#if keptCount > 0}
            {m.trash_purge_expired_kept({ count: keptCount })}
          {/if}
          {m.trash_purge_expired_question({ count: expiredCount })}
        </span>
        <Button
          variant="destructive"
          size="sm"
          data-confirm-go="trash-purge-expired"
          aria-describedby="trash-purge-expired-question"
          onclick={() =>
            void run(
              () => api.purgeExpiredTrash(workspace?.id ?? null),
              m.trash_purged_expired({ count: expiredCount }),
            )}>{m.trash_purge()}</Button
        >
        <Button
          size="sm"
          variant="ghost"
          onclick={() => {
            confirmPurge = undefined;
            void focusConfirmStart('trash-purge-expired');
          }}>{m.common_cancel()}</Button
        >
      {:else}
        <Button
          variant="destructive"
          size="sm"
          data-confirm="trash-purge-expired"
          onclick={() => {
            confirmPurge = EXPIRED;
            void focusConfirm('trash-purge-expired');
          }}>{m.trash_purge_expired_button({ count: expiredCount })}</Button
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
            <span id={`trash-purge-question-${e.id}`}
              >{e.kept_for_references
                ? m.trash_purge_question_kept()
                : m.trash_purge_question()}</span
            >
            <Button
              size="sm"
              variant="destructive"
              data-confirm-go={`trash-purge-${e.id}`}
              aria-describedby={`trash-purge-question-${e.id}`}
              onclick={() =>
                void run(() => api.purgeTrash(e.id), m.trash_purged({ label: label(e) }))}
              >{m.trash_purge()}</Button
            >
            <Button
              size="sm"
              variant="ghost"
              onclick={() => {
                confirmPurge = undefined;
                void focusConfirmStart(`trash-purge-${e.id}`);
              }}>{m.common_cancel()}</Button
            >
          {:else}
            <Button
              size="sm"
              variant="ghost"
              data-confirm={`trash-purge-${e.id}`}
              onclick={() => {
                confirmPurge = e.id;
                void focusConfirm(`trash-purge-${e.id}`);
              }}>{m.trash_purge_ellipsis()}</Button
            >
          {/if}
        {/if}
      </li>
    {:else}
      <!-- 목록을 받기 전에는 "휴지통이 비었습니다" 대신 불러오는 중이라고 보인다. -->
      {#if trash !== undefined}
        <li class="text-sm text-muted-foreground">{m.trash_empty()}</li>
      {:else if error === undefined}
        <li class="text-sm text-muted-foreground">{m.app_loading()}</li>
      {/if}
    {/each}
  </ul>
</section>
