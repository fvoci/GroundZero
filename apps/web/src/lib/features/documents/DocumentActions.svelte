<script lang="ts">
  // 문서 공유·설정·복제·이동 (D-20, D-23, D-24, D-49, D-59). 권한 판단은 서버가 하고,
  // 화면은 현재 권한(access)에 맞는 동작만 보여준다. 이동은 서버 미리보기로 접근 변화를 보여준 뒤 확인받는다.
  import type { Workspace } from '@fvoci/api-client';

  import type {
    DocumentView,
    MovePreviewView,
    MoveTargetInput,
    PublicLinkView,
    ShareView,
  } from '$lib/api/client';
  import { api, ApiError, newRequestId } from '$lib/api/client';
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
    error = e instanceof ApiError ? e.message : '요청을 처리하지 못했습니다.';
  }

  function open(next: Panel): void {
    error = undefined;
    panel = next;
    if (next === 'share') {
      newLinkUrl = undefined;
      void loadShares();
      void loadLinks();
    }
    if (next === 'copy') {
      copyTitle = `${doc.title} 사본`;
      copyTarget = 'personal';
      copyRequest = newRequestId();
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

  async function setSettings(
    visibility: 'workspace' | 'private',
    allowCopy: boolean,
  ): Promise<void> {
    try {
      onchanged(await api.setSettings(doc.id, visibility, allowCopy));
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

  const EXPIRY_OPTIONS = [
    { value: '1', label: '1일' },
    { value: '7', label: '7일' },
    { value: '30', label: '30일' },
    { value: '0', label: '만료 없음' },
  ];
  const expiryLabel = (v: string): string => EXPIRY_OPTIONS.find((o) => o.value === v)?.label ?? v;

  // --- 복제 ---------------------------------------------------------------
  let copyTitle = $state('');
  let copyTarget = $state('personal');
  let copyRequest = newRequestId();

  async function submitCopy(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    try {
      const target = copyTarget === 'personal' ? null : copyTarget;
      const copied = await api.copyDocument(doc.id, copyRequest, target, copyTitle);
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
    const modules = (await api.modules(ws.id)).filter((m) => !m.archived);
    const lists = await Promise.all(modules.map((m) => api.items(m.id)));
    return lists
      .flat()
      .filter((it) => !it.archived && it.access !== 'read' && it.id !== doc.item_id)
      .map((it) => ({
        value: `item:${it.id}`,
        label: `${ws.name} · ${it.reference ?? ''} ${it.title}`,
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
                    label: `${ws.name} 위키`,
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
        error = '그사이 접근할 수 있는 사람이 바뀌었습니다. 다시 확인하세요.';
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

  const targetLabel = (id: string): string =>
    id === 'personal' ? '내 노트' : (workspaces.find((w) => w.id === id)?.name ?? id);
</script>

<div class="flex flex-wrap items-center gap-2" role="toolbar" aria-label="문서 동작">
  {#if doc.placement === 'personal' && isOwner}
    <span class="rounded border px-1.5 text-sm text-muted-foreground"
      >개인 노트 · 공유한 사람만 봄</span
    >
  {:else if doc.placement === 'personal'}
    <span class="rounded border px-1.5 text-sm text-muted-foreground"
      >공유받은 개인 노트 · {doc.access === 'write' ? '고치기' : '읽기'}</span
    >
  {:else if doc.visibility === 'private'}
    <span class="rounded border px-1.5 text-sm text-muted-foreground">비공개 문서</span>
  {/if}
  {#if isOwner || doc.can_manage}
    <Button
      size="sm"
      variant="outline"
      onclick={() => {
        open('share');
      }}>공유·설정</Button
    >
  {/if}
  {#if canCopy}
    <Button
      size="sm"
      variant="outline"
      onclick={() => {
        open('copy');
      }}>복제</Button
    >
  {/if}
  {#if isOwner && writable.length > 0}
    <Button
      size="sm"
      variant="outline"
      onclick={() => {
        open('move');
      }}>이동</Button
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
        <Dialog.Title>공유와 설정</Dialog.Title>
        <Dialog.Description>
          공유받은 사람은 이 원본을 읽거나 고칩니다. 받는 사람은 이 이메일로 확인된 계정으로
          로그인해야 합니다.
          {#if doc.placement === 'personal'}
            개인 노트는 공유해도 내 소유이며 받는 사람의 워크스페이스 관리자에게는 보이지 않습니다.
          {/if}
        </Dialog.Description>
      </Dialog.Header>
      <ul class="flex flex-col gap-1" data-testid="share-list">
        {#each shares as s (s.id)}
          <li class="flex items-center gap-2 text-sm">
            <span>{s.email}</span>
            <span class="text-muted-foreground">{s.permission === 'write' ? '고치기' : '읽기'}</span
            >
            <Button size="sm" variant="ghost" class="ml-auto" onclick={() => void removeShare(s.id)}
              >철회</Button
            >
          </li>
        {:else}
          <li class="text-sm text-muted-foreground">공유한 사람이 없습니다.</li>
        {/each}
      </ul>
      {#if restrictOnly}
        <p class="text-sm text-muted-foreground" role="note">
          보관된 항목 아래 문서입니다. 공유·공개 링크 철회와 비공개 전환처럼 노출을 줄이는 변경만 할
          수 있습니다.
        </p>
      {:else}
        <form class="flex flex-wrap items-end gap-2" onsubmit={(e) => void addShare(e)}>
          <div class="flex min-w-48 flex-1 flex-col gap-1">
            <Label for="share-email">이메일</Label>
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
            <Select.Trigger class="w-24" aria-label="공유 권한"
              >{sharePermission === 'write' ? '고치기' : '읽기'}</Select.Trigger
            >
            <Select.Content>
              <Select.Item value="read" label="읽기">읽기</Select.Item>
              <Select.Item value="write" label="고치기">고치기</Select.Item>
            </Select.Content>
          </Select.Root>
          <Button type="submit" disabled={shareEmail.trim() === ''}>공유</Button>
        </form>
      {/if}
      <div class="flex flex-col gap-2 border-t pt-3">
        {#if doc.placement !== 'personal'}
          <div class="flex items-center gap-2">
            <Checkbox
              id="doc-private"
              disabled={restrictOnly && doc.visibility === 'private'}
              checked={doc.visibility === 'private'}
              onCheckedChange={(v) =>
                void setSettings(v === true ? 'private' : 'workspace', doc.allow_copy)}
            />
            <Label for="doc-private">비공개 (작성자·관리자·공유받은 사람만)</Label>
          </div>
        {/if}
        <div class="flex items-center gap-2">
          <Checkbox
            id="doc-allow-copy"
            disabled={restrictOnly && !doc.allow_copy}
            checked={doc.allow_copy}
            onCheckedChange={(v) => void setSettings(doc.visibility, v === true)}
          />
          <Label for="doc-allow-copy">볼 수 있는 사람의 복제 허용</Label>
        </div>
        <p class="text-sm text-muted-foreground">
          복제를 막아도 화면에 보이는 내용을 촬영하는 것까지 막지는 못합니다.
        </p>
      </div>
      <section class="flex flex-col gap-2 border-t pt-3" aria-labelledby="public-link-heading">
        <h3 id="public-link-heading" class="text-sm font-semibold">공개 링크</h3>
        <p class="text-sm text-muted-foreground">
          링크를 가진 사람은 로그인 없이 읽을 수 있습니다. 언제든 철회할 수 있습니다.
        </p>
        {#if newLinkUrl !== undefined}
          <div class="flex flex-col gap-1 rounded border px-2 py-2" role="status">
            <Label for="new-link-url">새 링크 (지금만 볼 수 있습니다)</Label>
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
              <span>{new Date(link.created_at).toLocaleDateString()} 생성</span>
              <span class="text-muted-foreground">
                {link.active
                  ? link.expires_at === null
                    ? '만료 없음'
                    : `~${new Date(link.expires_at).toLocaleDateString()}`
                  : '닫힘'}
              </span>
              {#if link.active}
                <Button
                  size="sm"
                  variant="ghost"
                  class="ml-auto"
                  onclick={() => void revokeLink(link.id)}>철회</Button
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
              <Select.Trigger class="w-28" aria-label="공개 링크 만료"
                >{expiryLabel(linkExpiryDays)}</Select.Trigger
              >
              <Select.Content>
                {#each EXPIRY_OPTIONS as option (option.value)}
                  <Select.Item value={option.value} label={option.label}>{option.label}</Select.Item
                  >
                {/each}
              </Select.Content>
            </Select.Root>
            <Button size="sm" onclick={() => void createLink()}>공개 링크 만들기</Button>
          </div>
        {/if}
      </section>
    {:else if panel === 'copy'}
      <Dialog.Header>
        <Dialog.Title>복제</Dialog.Title>
        <Dialog.Description>
          독립된 새 문서를 만듭니다. 이후 원본과 서로 동기화되지 않습니다.
        </Dialog.Description>
      </Dialog.Header>
      <form class="flex flex-col gap-3" onsubmit={(e) => void submitCopy(e)}>
        <div class="flex flex-col gap-1">
          <Label for="copy-title">새 제목</Label>
          <Input id="copy-title" bind:value={copyTitle} maxlength={200} required />
        </div>
        <div class="flex flex-col gap-1">
          <span class="text-sm font-medium" id="copy-target-label">만들 곳</span>
          <Select.Root
            type="single"
            value={copyTarget}
            onValueChange={(v: string) => {
              copyTarget = v;
              copyRequest = newRequestId();
            }}
          >
            <Select.Trigger aria-labelledby="copy-target-label"
              >{targetLabel(copyTarget)}</Select.Trigger
            >
            <Select.Content>
              <Select.Item value="personal" label="내 노트">내 노트</Select.Item>
              {#each writable as w (w.id)}
                <Select.Item value={w.id} label={w.name}>{w.name} 위키</Select.Item>
              {/each}
            </Select.Content>
          </Select.Root>
        </div>
        <Dialog.Footer>
          <Button variant="ghost" type="button" onclick={() => (panel = undefined)}>취소</Button>
          <Button type="submit" disabled={copyTitle.trim() === ''}>복제</Button>
        </Dialog.Footer>
      </form>
    {:else if panel === 'move'}
      <Dialog.Header>
        <Dialog.Title>문서 이동</Dialog.Title>
        <Dialog.Description>
          위키나 모듈 항목으로 옮깁니다. 이동하면 대상의 번호가 새로 붙습니다(위키는 WIKI, 항목은
          모듈 키). 기존 링크는 문서 ID로 연결되어 계속 열립니다.
        </Dialog.Description>
      </Dialog.Header>
      <div class="flex flex-col gap-1">
        <span class="text-sm font-medium" id="move-target-label">대상</span>
        <Select.Root
          type="single"
          value={moveTarget}
          onValueChange={(v: string) => {
            moveTarget = v;
            void loadPreview();
          }}
        >
          <Select.Trigger aria-labelledby="move-target-label"
            >{selectedTarget?.label ?? '옮길 수 있는 곳이 없습니다'}</Select.Trigger
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
            <h3 class="font-semibold">새로 볼 수 있게 되는 사람</h3>
            <p>{preview.gains.length === 0 ? '없음' : preview.gains.join(', ')}</p>
          </div>
          <div>
            <h3 class="font-semibold">더 이상 볼 수 없게 되는 사람</h3>
            <p>{preview.loses.length === 0 ? '없음' : preview.loses.join(', ')}</p>
          </div>
        </div>
        {#if preview.history_moves_with_document}
          <p class="text-sm text-muted-foreground">
            이전 리비전 기록도 함께 옮겨져, 새로 볼 수 있게 되는 사람에게 보입니다.
          </p>
        {/if}
        <p class="text-sm text-muted-foreground">
          {preview.reference_changes
            ? '대상에서 번호가 새로 붙습니다.'
            : '같은 모듈 안 이동이라 번호가 그대로입니다.'}
        </p>
        {#if preview.active_public_links > 0}
          <p class="text-sm" role="note">
            활성 공개 링크 {preview.active_public_links}개가 그대로 유지되어 로그인 없이 계속 볼 수
            있습니다. 막으려면 공유·설정에서 링크를 철회하세요.
          </p>
        {/if}
      {/if}
      <Dialog.Footer>
        <Button variant="ghost" onclick={() => (panel = undefined)}>취소</Button>
        <Button disabled={preview === undefined} onclick={() => void confirmMove()}>이동</Button>
      </Dialog.Footer>
    {/if}
    {#if error !== undefined}
      <p role="alert" class="text-sm text-destructive">{error}</p>
    {/if}
  </Dialog.Content>
</Dialog.Root>
