<script lang="ts">
  // 문서 공유·설정·복제·이동 (D-20, D-23, D-24, D-49, D-59). 권한 판단은 서버가 하고,
  // 화면은 현재 권한(access)에 맞는 동작만 보여준다. 이동은 서버 미리보기로 접근 변화를 보여준 뒤 확인받는다.
  import { formatInstant } from '$lib/display';
  import type { Workspace } from '@fvoci/api-client';

  import type {
    DocumentView,
    MovePreviewView,
    MoveTargetInput,
    PublicLinkView,
    ShareView,
  } from '$lib/api/client';
  import { api } from '$lib/api/client';
  import { RetryKey } from '$lib/api/retry-key';
  import { errorText } from '$lib/api/errors';
  import { workspaceLabel } from '$lib/features/workspaces/badge';
  import { m } from '$lib/paraglide/messages.js';
  import { pathFor, router, workspacePath } from '$lib/router.svelte';
  import { Button } from '$lib/ui/button';
  import { Checkbox } from '$lib/ui/checkbox';
  import * as Dialog from '$lib/ui/dialog';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';
  import * as Select from '$lib/ui/select';

  interface Props {
    doc: DocumentView;
    workspaces: Workspace[];
    onchanged: (doc: DocumentView) => void;
  }

  const { doc, workspaces, onchanged }: Props = $props();

  type Panel = 'share' | 'copy' | 'move' | undefined;
  let panel = $state<Panel>(undefined);
  let error = $state<string | undefined>(undefined);

  const isOwner = $derived(doc.access === 'owner');
  // 보관된 항목 아래 문서: 소유 권한자도 노출을 줄이는 변경만 할 수 있다.
  const restrictOnly = $derived(!isOwner && doc.can_manage);
  const canCopy = $derived(isOwner || doc.allow_copy);
  const writable = $derived(workspaces.filter((w) => w.role !== 'reader'));
  const memberOf = (ws: string): boolean => workspaces.some((w) => w.id === ws);

  function fail(e: unknown): void {
    error = errorText(e, m.documents_request_failed());
  }

  function open(next: Panel): void {
    error = undefined;
    panel = next;
    if (next === 'share') {
      newLinkUrl = undefined;
      void loadShares();
      void loadLinks();
      void refreshSettings();
    }
    if (next === 'copy') {
      copyTitle = m.documents_copy_default_title({ title: doc.title });
      copyTarget = 'personal';
      copyKey.done();
    }
    if (next === 'move') void openMove();
  }

  // --- 공유 ---------------------------------------------------------------
  let shares = $state<ShareView[]>([]);
  let shareEmail = $state('');
  let sharePermission = $state<'read' | 'write'>('read');

  async function loadShares(): Promise<void> {
    try {
      shares = await api.shares(doc.id);
    } catch (e: unknown) {
      fail(e);
    }
  }

  async function addShare(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    try {
      await api.share(doc.id, shareEmail, sharePermission);
      shareEmail = '';
      await loadShares();
    } catch (e: unknown) {
      fail(e);
    }
  }

  async function removeShare(id: string): Promise<void> {
    try {
      await api.unshare(id);
      await loadShares();
    } catch (e: unknown) {
      fail(e);
    }
  }

  // 설정을 읽거나 바꾼 순서. 다시 읽기가 늦게 와 그 뒤에 바꾼 설정을 덮지 않게 한다.
  let settingsSeq = 0;

  // 화면의 문서는 연 뒤로 다시 읽지 않는다. 그사이 다른 소유자가 바꾼 설정(비공개·복제 금지)을 보이도록 설정 창을 열 때
  // 서버의 지금 값을 읽는다. 바꿀 때 되돌리지 않는 것은 서버가 정한다(바꾼 칸만 보냄).
  async function refreshSettings(): Promise<void> {
    const mine = ++settingsSeq;
    try {
      const fresh = await api.document(doc.id);
      if (mine !== settingsSeq || fresh.id !== doc.id) return;
      onchanged({
        ...doc,
        visibility: fresh.visibility,
        allow_copy: fresh.allow_copy,
        is_template: fresh.is_template,
      });
    } catch (e: unknown) {
      fail(e);
    }
  }

  // 설정 요청이 진행 중인지. 응답 전에 다른 체크를 누르면 화면이 첫 변경 전의 값을 보이므로 그동안 막는다.
  let settingsBusy = $state(false);

  /**
   * 바꾼 설정 하나만 보낸다. 건드리지 않은 설정은 서버가 잠근 뒤의 지금 값 그대로 둔다(0085, 감사 W6: 화면의 옛 값을
   * 함께 보내 다른 소유자의 비공개·복제 금지를 되돌렸다).
   */
  async function setSettings(change: {
    visibility?: 'workspace' | 'private';
    allow_copy?: boolean;
  }): Promise<void> {
    if (settingsBusy) return;
    settingsBusy = true;
    settingsSeq += 1;
    try {
      onchanged(await api.setSettings(doc.id, change));
    } catch (e: unknown) {
      fail(e);
    } finally {
      settingsBusy = false;
    }
  }

  async function setTemplate(on: boolean): Promise<void> {
    settingsSeq += 1;
    try {
      await api.setDocumentTemplate(doc.id, on);
      onchanged({ ...doc, is_template: on });
    } catch (e: unknown) {
      fail(e);
    }
  }

  // --- 공개 링크 (D-20) ----------------------------------------------------
  let links = $state<PublicLinkView[]>([]);
  let linkExpiryDays = $state('7');
  let newLinkUrl = $state<string | undefined>(undefined);

  async function loadLinks(): Promise<void> {
    try {
      links = await api.publicLinks(doc.id);
    } catch (e: unknown) {
      fail(e);
    }
  }

  async function createLink(): Promise<void> {
    try {
      const days = Number(linkExpiryDays);
      const expiresAt =
        days > 0 ? new Date(Date.now() + days * 24 * 60 * 60 * 1000).toISOString() : null;
      newLinkUrl = (await api.createPublicLink(doc.id, expiresAt)).url;
      await loadLinks();
    } catch (e: unknown) {
      fail(e);
    }
  }

  async function revokeLink(id: string): Promise<void> {
    try {
      await api.revokePublicLink(id);
      newLinkUrl = undefined;
      await loadLinks();
    } catch (e: unknown) {
      fail(e);
    }
  }

  const EXPIRY_DAYS = ['1', '7', '30', '0'];
  const expiryLabel = (v: string): string =>
    v === '0' ? m.documents_expiry_none() : m.documents_expiry_days({ days: Number(v) });

  // --- 복제 ---------------------------------------------------------------
  let copyTitle = $state('');
  let copyTarget = $state('personal');
  // 같은 대상·제목의 재시도는 같은 요청 id, 고치면 새 id.
  const copyKey = new RetryKey();

  async function submitCopy(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    try {
      const target = copyTarget === 'personal' ? null : copyTarget;
      const requestId = copyKey.for({ doc: doc.id, target, title: copyTitle });
      const copied = await api.copyDocument(doc.id, requestId, target, copyTitle);
      copyKey.done(requestId);
      panel = undefined;
      router.navigate(pathFor(copied, memberOf));
    } catch (e: unknown) {
      fail(e);
    }
  }

  // --- 이동 ---------------------------------------------------------------
  // 대상: 쓸 수 있는 워크스페이스의 위키, 그리고 그 안에서 내가 쓸 수 있는 항목 (D-59).
  interface MoveOption {
    value: string;
    label: string;
    target: MoveTargetInput;
  }
  let moveOptions = $state<MoveOption[]>([]);
  let moveTarget = $state('');
  let preview = $state<MovePreviewView | undefined>(undefined);
  const selectedTarget = $derived(moveOptions.find((o) => o.value === moveTarget));

  async function itemOptions(ws: Workspace): Promise<MoveOption[]> {
    const modules = (await api.modules(ws.id)).filter((mod) => !mod.archived);
    const lists = await Promise.all(modules.map((mod) => api.items(mod.id)));
    return lists
      .flat()
      .filter((it) => !it.archived && it.access !== 'read' && it.id !== doc.item_id)
      .map((it) => ({
        value: `item:${it.id}`,
        label: `${workspaceLabel(ws, workspaces)} · ${it.reference ?? ''} ${it.title}`,
        target: { workspaceId: ws.id, itemId: it.id },
      }));
  }

  async function openMove(): Promise<void> {
    moveOptions = [];
    moveTarget = '';
    preview = undefined;
    try {
      const perWorkspace = await Promise.all(
        writable.map(async (ws) => {
          const wiki: MoveOption[] =
            ws.id === doc.workspace_id && doc.item_id === null
              ? []
              : [
                  {
                    value: `wiki:${ws.id}`,
                    label: m.documents_wiki({ workspace: workspaceLabel(ws, workspaces) }),
                    target: { workspaceId: ws.id, itemId: null },
                  },
                ];
          return [...wiki, ...(await itemOptions(ws))];
        }),
      );
      moveOptions = perWorkspace.flat();
      moveTarget = moveOptions[0]?.value ?? '';
      await loadPreview();
    } catch (e: unknown) {
      fail(e);
    }
  }

  async function loadPreview(): Promise<void> {
    preview = undefined;
    if (selectedTarget === undefined) return;
    try {
      preview = await api.movePreview(doc.id, selectedTarget.target);
    } catch (e: unknown) {
      fail(e);
    }
  }

  async function confirmMove(): Promise<void> {
    if (preview === undefined || selectedTarget === undefined) return;
    try {
      const result = await api.moveDocument(doc.id, selectedTarget.target, preview);
      if (result.kind === 'changed') {
        // 그사이 접근 변화가 바뀌었다. 새 미리보기를 보여주고 다시 확인받는다.
        preview = result.preview;
        error = m.documents_move_changed();
        return;
      }
      panel = undefined;
      const { outcome } = result;
      if (outcome.document === null) {
        // 비공개 문서를 작성 권한만 있는 곳으로 옮기면 나는 더 이상 볼 수 없다.
        router.navigate(workspacePath(outcome.workspace_id));
        return;
      }
      onchanged(outcome.document);
      router.navigate(pathFor(outcome.document, memberOf));
    } catch (e: unknown) {
      fail(e);
    }
  }

  function targetLabel(id: string): string {
    if (id === 'personal') return m.documents_my_notes();
    const ws = workspaces.find((w) => w.id === id);
    return ws === undefined ? id : workspaceLabel(ws, workspaces);
  }
</script>

<div
  class="flex flex-wrap items-center gap-2"
  role="toolbar"
  aria-label={m.documents_actions_label()}
>
  {#if doc.placement === 'personal' && isOwner}
    <span class="rounded border px-1.5 text-sm text-muted-foreground"
      >{m.documents_personal_owner()}</span
    >
  {:else if doc.placement === 'personal'}
    <span class="rounded border px-1.5 text-sm text-muted-foreground"
      >{doc.access === 'write'
        ? m.documents_personal_shared_write()
        : m.documents_personal_shared_read()}</span
    >
  {:else if doc.visibility === 'private'}
    <span class="rounded border px-1.5 text-sm text-muted-foreground">{m.documents_private()}</span>
  {/if}
  {#if isOwner || doc.can_manage}
    <Button
      size="sm"
      variant="outline"
      onclick={() => {
        open('share');
      }}>{m.documents_share_settings()}</Button
    >
  {/if}
  {#if canCopy}
    <Button
      size="sm"
      variant="outline"
      onclick={() => {
        open('copy');
      }}>{m.documents_copy()}</Button
    >
  {/if}
  {#if isOwner && writable.length > 0}
    <Button
      size="sm"
      variant="outline"
      onclick={() => {
        open('move');
      }}>{m.documents_move()}</Button
    >
  {/if}
</div>

<Dialog.Root
  open={panel !== undefined}
  onOpenChange={(o: boolean) => {
    if (!o) panel = undefined;
  }}
>
  <Dialog.Content>
    {#if panel === 'share'}
      <Dialog.Header>
        <Dialog.Title>{m.documents_share_title()}</Dialog.Title>
        <Dialog.Description>
          {m.documents_share_description()}
          {#if doc.placement === 'personal'}
            {m.documents_share_description_personal()}
          {/if}
        </Dialog.Description>
      </Dialog.Header>
      <ul class="flex flex-col gap-1" data-testid="share-list">
        {#each shares as s (s.id)}
          <li class="flex items-center gap-2 text-sm">
            <span>{s.email}</span>
            <span class="text-muted-foreground"
              >{s.permission === 'write'
                ? m.documents_permission_write()
                : m.documents_permission_read()}</span
            >
            <Button size="sm" variant="ghost" class="ml-auto" onclick={() => void removeShare(s.id)}
              >{m.documents_revoke()}</Button
            >
          </li>
        {:else}
          <li class="text-sm text-muted-foreground">{m.documents_share_empty()}</li>
        {/each}
      </ul>
      {#if restrictOnly}
        <p class="text-sm text-muted-foreground" role="note">
          {m.documents_restrict_only()}
        </p>
      {:else}
        <form class="flex flex-wrap items-end gap-2" onsubmit={(e) => void addShare(e)}>
          <div class="flex min-w-48 flex-1 flex-col gap-1">
            <Label for="share-email">{m.documents_email()}</Label>
            <Input
              id="share-email"
              type="email"
              bind:value={shareEmail}
              autocomplete="off"
              required
            />
          </div>
          <Select.Root
            type="single"
            value={sharePermission}
            onValueChange={(v: string) => {
              sharePermission = v === 'write' ? 'write' : 'read';
            }}
          >
            <Select.Trigger class="w-24" aria-label={m.documents_share_permission()}
              >{sharePermission === 'write'
                ? m.documents_permission_write()
                : m.documents_permission_read()}</Select.Trigger
            >
            <Select.Content>
              <Select.Item value="read" label={m.documents_permission_read()}
                >{m.documents_permission_read()}</Select.Item
              >
              <Select.Item value="write" label={m.documents_permission_write()}
                >{m.documents_permission_write()}</Select.Item
              >
            </Select.Content>
          </Select.Root>
          <Button type="submit" disabled={shareEmail.trim() === ''}>{m.documents_share()}</Button>
        </form>
      {/if}
      <div class="flex flex-col gap-2 border-t pt-3">
        {#if doc.placement !== 'personal'}
          <div class="flex items-center gap-2">
            <Checkbox
              id="doc-private"
              disabled={settingsBusy || (restrictOnly && doc.visibility === 'private')}
              checked={doc.visibility === 'private'}
              onCheckedChange={(v) =>
                void setSettings({ visibility: v === true ? 'private' : 'workspace' })}
            />
            <Label for="doc-private">{m.documents_private_label()}</Label>
          </div>
        {/if}
        <div class="flex items-center gap-2">
          <Checkbox
            id="doc-allow-copy"
            disabled={settingsBusy || (restrictOnly && !doc.allow_copy)}
            checked={doc.allow_copy}
            onCheckedChange={(v) => void setSettings({ allow_copy: v === true })}
          />
          <Label for="doc-allow-copy">{m.documents_allow_copy()}</Label>
        </div>
        {#if isOwner}
          <div class="flex items-center gap-2">
            <Checkbox
              id="doc-template"
              checked={doc.is_template}
              onCheckedChange={(v) => void setTemplate(v === true)}
            />
            <Label for="doc-template">{m.documents_use_as_template()}</Label>
          </div>
        {/if}
        <p class="text-sm text-muted-foreground">
          {m.documents_copy_limit()}
        </p>
      </div>
      <section class="flex flex-col gap-2 border-t pt-3" aria-labelledby="public-link-heading">
        <h3 id="public-link-heading" class="text-sm font-semibold">{m.documents_public_link()}</h3>
        <p class="text-sm text-muted-foreground">
          {m.documents_public_link_hint()}
        </p>
        {#if newLinkUrl !== undefined}
          <div class="flex flex-col gap-1 rounded border px-2 py-2" role="status">
            <Label for="new-link-url">{m.documents_new_link()}</Label>
            <Input
              id="new-link-url"
              readonly
              value={newLinkUrl}
              onfocus={(e: FocusEvent) => {
                if (e.currentTarget instanceof HTMLInputElement) e.currentTarget.select();
              }}
            />
          </div>
        {/if}
        <ul class="flex flex-col gap-1 text-sm" data-testid="public-link-list">
          {#each links as link (link.id)}
            <li class="flex items-center gap-2">
              <span
                >{m.documents_link_created({
                  date: formatInstant(link.created_at, { dateStyle: 'medium' }),
                })}</span
              >
              <span class="text-muted-foreground">
                {link.active
                  ? link.expires_at === null
                    ? m.documents_expiry_none()
                    : m.documents_link_until({
                        date: formatInstant(link.expires_at, { dateStyle: 'medium' }),
                      })
                  : m.documents_link_closed()}
              </span>
              {#if link.active}
                <Button
                  size="sm"
                  variant="ghost"
                  class="ml-auto"
                  onclick={() => void revokeLink(link.id)}>{m.documents_revoke()}</Button
                >
              {/if}
            </li>
          {/each}
        </ul>
        {#if isOwner}
          <div class="flex items-center gap-2">
            <Select.Root
              type="single"
              value={linkExpiryDays}
              onValueChange={(v: string) => {
                linkExpiryDays = v;
              }}
            >
              <Select.Trigger class="w-28" aria-label={m.documents_link_expiry_label()}
                >{expiryLabel(linkExpiryDays)}</Select.Trigger
              >
              <Select.Content>
                {#each EXPIRY_DAYS as days (days)}
                  <Select.Item value={days} label={expiryLabel(days)}
                    >{expiryLabel(days)}</Select.Item
                  >
                {/each}
              </Select.Content>
            </Select.Root>
            <Button size="sm" onclick={() => void createLink()}>{m.documents_create_link()}</Button>
          </div>
        {/if}
      </section>
    {:else if panel === 'copy'}
      <Dialog.Header>
        <Dialog.Title>{m.documents_copy()}</Dialog.Title>
        <Dialog.Description>
          {m.documents_copy_description()}
        </Dialog.Description>
      </Dialog.Header>
      <form class="flex flex-col gap-3" onsubmit={(e) => void submitCopy(e)}>
        <div class="flex flex-col gap-1">
          <Label for="copy-title">{m.documents_new_title()}</Label>
          <Input id="copy-title" bind:value={copyTitle} maxlength={200} required />
        </div>
        <div class="flex flex-col gap-1">
          <span class="text-sm font-medium" id="copy-target-label">{m.documents_copy_target()}</span
          >
          <Select.Root
            type="single"
            value={copyTarget}
            onValueChange={(v: string) => {
              // 대상만 바꾼 것으로는 키를 비우지 않는다(보낼 때 내용이 다르면 for가 새 id를 준다. 되돌린 뒤 다시 보내면 같은 id).
              copyTarget = v;
            }}
          >
            <Select.Trigger aria-labelledby="copy-target-label"
              >{targetLabel(copyTarget)}</Select.Trigger
            >
            <Select.Content>
              <Select.Item value="personal" label={m.documents_my_notes()}
                >{m.documents_my_notes()}</Select.Item
              >
              {#each writable as w (w.id)}
                <Select.Item value={w.id} label={workspaceLabel(w, workspaces)}
                  >{m.documents_wiki({ workspace: workspaceLabel(w, workspaces) })}</Select.Item
                >
              {/each}
            </Select.Content>
          </Select.Root>
        </div>
        <Dialog.Footer>
          <Button variant="ghost" type="button" onclick={() => (panel = undefined)}
            >{m.common_cancel()}</Button
          >
          <Button type="submit" disabled={copyTitle.trim() === ''}>{m.documents_copy()}</Button>
        </Dialog.Footer>
      </form>
    {:else if panel === 'move'}
      <Dialog.Header>
        <Dialog.Title>{m.documents_move_title()}</Dialog.Title>
        <Dialog.Description>
          {m.documents_move_description()}
        </Dialog.Description>
      </Dialog.Header>
      <div class="flex flex-col gap-1">
        <span class="text-sm font-medium" id="move-target-label">{m.documents_move_target()}</span>
        <Select.Root
          type="single"
          value={moveTarget}
          onValueChange={(v: string) => {
            moveTarget = v;
            void loadPreview();
          }}
        >
          <Select.Trigger aria-labelledby="move-target-label"
            >{selectedTarget?.label ?? m.documents_move_none()}</Select.Trigger
          >
          <Select.Content>
            {#each moveOptions as option (option.value)}
              <Select.Item value={option.value} label={option.label}>{option.label}</Select.Item>
            {/each}
          </Select.Content>
        </Select.Root>
      </div>
      {#if preview !== undefined}
        <div class="grid gap-2 text-sm md:grid-cols-2" data-testid="move-preview">
          <div>
            <h3 class="font-semibold">{m.documents_move_gains()}</h3>
            <p>{preview.gains.length === 0 ? m.documents_none() : preview.gains.join(', ')}</p>
          </div>
          <div>
            <h3 class="font-semibold">{m.documents_move_loses()}</h3>
            <p>{preview.loses.length === 0 ? m.documents_none() : preview.loses.join(', ')}</p>
          </div>
        </div>
        {#if preview.history_moves_with_document}
          <p class="text-sm text-muted-foreground">
            {m.documents_move_history()}
          </p>
        {/if}
        <p class="text-sm text-muted-foreground">
          {preview.reference_changes
            ? m.documents_move_reference_changes()
            : m.documents_move_reference_same()}
        </p>
        {#if preview.children_left_behind > 0}
          <p class="text-sm" role="note">
            {m.documents_move_children({ count: preview.children_left_behind })}
          </p>
        {/if}
        {#if preview.active_public_links > 0}
          <p class="text-sm" role="note">
            {m.documents_move_public_links({ count: preview.active_public_links })}
          </p>
        {/if}
      {/if}
      <Dialog.Footer>
        <Button variant="ghost" onclick={() => (panel = undefined)}>{m.common_cancel()}</Button>
        <Button disabled={preview === undefined} onclick={() => void confirmMove()}
          >{m.documents_move()}</Button
        >
      </Dialog.Footer>
    {/if}
    {#if error !== undefined}
      <p role="alert" class="text-sm text-destructive">{error}</p>
    {/if}
  </Dialog.Content>
</Dialog.Root>
