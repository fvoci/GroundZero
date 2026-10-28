<script lang="ts">
  // 문서 공유·설정·복제·이동 (D-20, D-23, D-24, D-49, D-59). 권한 판단은 서버가 하고,
  // 화면은 현재 권한(access)에 맞는 동작만 보여준다. 이동은 서버 미리보기로 접근 변화를 보여준 뒤 확인받는다.
  // 좁은 화면에서 동작 단추 여덟 개가 두 줄을 차지해, 공유·설정과 (부르는 쪽의) 기록만 두고 나머지는 "더 보기" 메뉴에 둔다.
  import { formatInstant } from '$lib/display';
  import type { Workspace } from '@fvoci/api-client';
  import type { Snippet } from 'svelte';
  import { onMount, tick, untrack } from 'svelte';

  import type {
    DocumentView,
    MovePreviewView,
    MoveTargetInput,
    PublicLinkView,
    ShareRecipientView,
    ShareView,
  } from '$lib/api/client';
  import { api, newRequestId } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import { createFormDraft } from '$lib/features/common/form-draft.svelte';
  import FormDraftOffer from '$lib/features/common/FormDraftOffer.svelte';
  import DiscardChangesConfirm from '$lib/features/common/DiscardChangesConfirm.svelte';
  import { notifyDone } from '$lib/features/feedback/feedback.svelte';
  import { me } from '$lib/me.svelte';
  import { workspaceLabel } from '$lib/features/workspaces/badge';
  import { roleLabel } from '$lib/features/members/roles';
  import { m } from '$lib/paraglide/messages.js';
  import { org } from '$lib/org.svelte';
  import { pathFor, router, workspacePath } from '$lib/router.svelte';
  import { Button } from '$lib/ui/button';
  import { Checkbox } from '$lib/ui/checkbox';
  import * as Dialog from '$lib/ui/dialog';
  import * as DropdownMenu from '$lib/ui/dropdown-menu';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';
  import * as Select from '$lib/ui/select';

  interface Props {
    doc: DocumentView;
    workspaces: Workspace[];
    onchanged: (doc: DocumentView) => void;
    /** 공유·설정 다음, "더 보기" 앞에 늘 보이는 동작(기록). */
    actions?: Snippet;
    /** "더 보기" 메뉴의 복제·이동 뒤에 이어지는 항목(내보내기, 홈 표시, 지우기). */
    more?: Snippet;
    /** "더 보기" 단추. 메뉴에서 연 일이 끝나면 부르는 쪽이 여기로 초점을 돌린다. */
    moreButton?: HTMLButtonElement | null;
  }

  let { doc, workspaces, onchanged, actions, more, moreButton = $bindable(null) }: Props = $props();

  type Panel = 'share' | 'copy' | 'move' | undefined;
  let panel = $state<Panel>(undefined);
  // 마지막으로 연 창. 닫힌 뒤(panel은 이미 비었다) 초점을 어디로 돌릴지 정한다.
  let opened: Panel = undefined;
  let error = $state<string | undefined>(undefined);

  const isOwner = $derived(doc.access === 'owner');
  // 공유·공개 링크·설정은 서버의 관리 판단(`can_manage`)을 따른다. 보관된 항목의 문서는 등급이 read로 낮아져도
  // 워크스페이스 관리자는 보관 해제 없이 넓히는 변경까지 할 수 있다(D-113 ⑯, SQL `app_document_can_manage`).
  // 템플릿 지정과 이동은 owner 등급만이다.
  const canManage = $derived(isOwner || doc.can_manage);
  const canCopy = $derived(isOwner || doc.allow_copy);
  // 공개 링크 정책은 문서가 속한 워크스페이스의 조직 것이다(지금 고른 조직이 아니다). 개인 노트에는 조직 정책이 없다
  // (SQL `app_create_public_link`). 그 조직을 알 수 없으면 숨기지 않고 서버의 거절(409)을 그대로 알린다.
  const publicLinksEnabled = $derived.by(() => {
    if (doc.workspace_id === null) return true;
    const organizationId = workspaces.find((w) => w.id === doc.workspace_id)?.organization_id;
    return org.organizations.find((o) => o.id === organizationId)?.public_links_enabled !== false;
  });
  // 쓰기 대상은 서버 접근 등급(`Workspace.access`)으로만 고른다(D-113). 조직 밖 이동·복제는 제외한다(D-114).
  const writable = $derived(
    workspaces.filter((w) => w.organization_id === org.currentId && w.access !== 'read'),
  );
  const memberOf = (ws: string): boolean => workspaces.some((w) => w.id === ws);

  function fail(e: unknown): void {
    error = errorText(e, m.documents_request_failed());
  }

  function open(next: Panel): void {
    const change = (): void => {
      openNow(next);
    };
    if (panel === 'share') shareDraft.leave.request(change);
    else if (panel === 'copy') copyDraft.leave.request(change);
    else change();
  }

  function openNow(next: Panel): void {
    error = undefined;
    panel = next;
    if (next !== undefined) opened = next;
    if (next === 'share') {
      newLinkUrl = undefined;
      void loadShares();
      void loadLinks();
      void refreshSettings();
    }
    if (next === 'move') void openMove();
  }

  // --- 공유 ---------------------------------------------------------------
  let shares = $state<ShareView[]>([]);
  let shareEmail = $state('');
  let sharePermission = $state<'read' | 'write'>('read');
  let sharing = $state(false);
  type RecipientState =
    | { kind: 'idle' }
    | { kind: 'loading'; key: string }
    | { kind: 'ready'; key: string; recipient: ShareRecipientView }
    | { kind: 'failed'; key: string };
  let recipientState = $state<RecipientState>({ kind: 'idle' });
  const recipientKey = $derived(
    JSON.stringify([doc.id, org.currentId, me.userId, shareEmail.trim().toLowerCase()]),
  );
  const selectedRecipient = $derived(
    recipientState.kind === 'ready' && recipientState.key === recipientKey
      ? recipientState.recipient
      : undefined,
  );
  const canOfferWrite = $derived(
    doc.placement === 'personal' || selectedRecipient?.max_permission === 'write',
  );
  const offeredPermission = $derived(canOfferWrite ? sharePermission : 'read');

  $effect(() => {
    const documentId = doc.id;
    const organizationId = org.currentId;
    const email = shareEmail.trim();
    const key = recipientKey;
    if (
      panel !== 'share' ||
      doc.placement === 'personal' ||
      organizationId === undefined ||
      !email.includes('@')
    ) {
      recipientState = { kind: 'idle' };
      return;
    }
    recipientState = { kind: 'loading', key };
    let current = true;
    const timer = setTimeout(() => {
      void api.shareRecipient(documentId, email, organizationId).then(
        (recipient) => {
          if (current) recipientState = { kind: 'ready', key, recipient };
        },
        () => {
          if (current) recipientState = { kind: 'failed', key };
        },
      );
    }, 200);
    return () => {
      current = false;
      clearTimeout(timer);
    };
  });
  interface ShareDraft {
    email: string;
    permission: 'read' | 'write';
  }
  function resetShare(): void {
    shareEmail = '';
    sharePermission = 'read';
  }
  const shareDraft = createFormDraft<ShareDraft>({
    name: () => `document-share:${doc.id}:${org.currentId ?? ''}`,
    snapshot: () => ({ email: shareEmail, permission: sharePermission }),
    parse: (value) => {
      if (
        typeof value !== 'object' ||
        value === null ||
        !('email' in value) ||
        typeof value.email !== 'string' ||
        !('permission' in value) ||
        (value.permission !== 'read' && value.permission !== 'write')
      )
        return undefined;
      return { email: value.email, permission: value.permission };
    },
    restore: (value) => {
      shareEmail = value.email;
      sharePermission = value.permission;
    },
    reset: resetShare,
    dirty: () => shareEmail !== '',
    busy: () => sharing,
  });
  onMount(shareDraft.attach);
  let sharesSequence = 0;

  async function loadShares(): Promise<void> {
    const sequence = ++sharesSequence;
    const documentId = doc.id;
    const userId = me.userId;
    const current = (): boolean =>
      alive && sequence === sharesSequence && documentId === doc.id && userId === me.userId;
    try {
      const result = await api.shares(documentId);
      if (current()) shares = result;
    } catch (e: unknown) {
      if (current()) fail(e);
    }
  }

  async function addShare(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (sharing) return;
    const organizationId = org.currentId;
    if (organizationId === undefined) {
      error = m.app_organization_required();
      return;
    }
    const attempt = shareDraft.capture();
    if (attempt === undefined) return;
    sharing = true;
    try {
      await api.share(doc.id, shareEmail, offeredPermission, organizationId);
      if (!shareDraft.isOwner(attempt)) return;
      if (shareDraft.complete(attempt)) notifyDone(m.common_saved());
      await loadShares();
    } catch (e: unknown) {
      if (shareDraft.isCurrent(attempt)) fail(e);
    } finally {
      if (shareDraft.isOwner(attempt)) sharing = false;
    }
  }

  // 공유 철회·공개 링크 철회는 한 번 더 묻는다(같은 화면의 다른 지우기처럼). 확인 단추로 초점을 옮기고, 그만두면 처음 누른
  // 단추로 돌려준다.
  interface Asking {
    kind: 'share' | 'link';
    id: string;
  }
  let asking = $state<Asking | undefined>(undefined);
  let askConfirm = $state<HTMLButtonElement | null>(null);
  const askId = (a: Asking): string => `${a.kind}-revoke-${a.id}`;

  async function ask(next: Asking): Promise<void> {
    asking = next;
    await tick();
    askConfirm?.focus();
  }

  async function stopAsking(): Promise<void> {
    const was = asking;
    asking = undefined;
    await tick();
    if (was !== undefined) document.getElementById(askId(was))?.focus();
  }

  async function removeShare(id: string): Promise<void> {
    try {
      await api.unshare(id);
      notifyDone(m.common_saved());
      await loadShares();
    } catch (e: unknown) {
      fail(e);
    }
  }

  // 설정을 읽거나 바꾼 순서. 다시 읽기가 늦게 와 그 뒤에 바꾼 설정을 덮지 않게 한다.
  let settingsSeq = 0;
  // 이 창이 화면에 있는지. 문서 화면은 다른 문서로 옮기면 이 창을 내리고 돌아오면 새로 만든다(A → B → A). 내린 창의 늦은
  // 설정 응답이 같은 id의 새 화면 값을 덮지 않게, 내린 뒤에는 결과를 넘기지 않는다(리뷰).
  let alive = true;
  $effect(() => () => {
    alive = false;
  });
  function settingsChanged(next: DocumentView): void {
    if (alive) onchanged(next);
  }

  // 화면의 문서는 연 뒤로 다시 읽지 않는다. 그사이 다른 소유자가 바꾼 설정(비공개·복제 금지)을 보이도록 설정 창을 열 때
  // 서버의 지금 값을 읽는다. 바꿀 때 되돌리지 않는 것은 서버가 정한다(바꾼 칸만 보냄).
  async function refreshSettings(): Promise<void> {
    const mine = ++settingsSeq;
    try {
      const fresh = await api.document(doc.id);
      if (mine !== settingsSeq || fresh.id !== doc.id) return;
      settingsChanged({
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
    const mine = ++settingsSeq;
    try {
      const saved = await api.setSettings(doc.id, change);
      notifyDone(m.common_saved());
      // 그사이 설정 창을 다시 열어 다시 읽었으면 이 응답과 그 읽기 중 어느 쪽이 더 새로운지 모른다. 그대로 넣으면 그사이
      // 다른 소유자가 바꾼 값을 되돌려 보일 수 있으므로 한 번 더 읽어 서버의 지금 값으로 맞춘다(리뷰).
      if (mine === settingsSeq) settingsChanged(saved);
      else if (alive) void refreshSettings();
    } catch (e: unknown) {
      fail(e);
    } finally {
      settingsBusy = false;
    }
  }

  // 템플릿도 같은 문서의 설정이다. 겹쳐 보내면 늦게 온 다른 설정 응답(그때의 템플릿 값)이 화면을 서버와 다르게 되돌린다.
  async function setTemplate(on: boolean): Promise<void> {
    if (settingsBusy) return;
    settingsBusy = true;
    const mine = ++settingsSeq;
    try {
      await api.setDocumentTemplate(doc.id, on);
      notifyDone(m.common_saved());
      if (mine === settingsSeq) settingsChanged({ ...doc, is_template: on });
      // 템플릿 응답에는 다른 설정이 없다. 창을 열 때 시작한 다시 읽기가 이 저장 때문에 버려졌을 수 있으므로, 서버의 지금
      // 값으로 한 번 더 맞춘다(리뷰: 그사이 다른 소유자가 바꾼 비공개·복제 금지가 창을 다시 열 때까지 옛 값으로 보였다).
      if (alive) void refreshSettings();
    } catch (e: unknown) {
      fail(e);
    } finally {
      settingsBusy = false;
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
    if (!publicLinksEnabled) return;
    try {
      const days = Number(linkExpiryDays);
      const expiresAt =
        days > 0 ? new Date(Date.now() + days * 24 * 60 * 60 * 1000).toISOString() : null;
      newLinkUrl = (await api.createPublicLink(doc.id, expiresAt)).url;
      notifyDone(m.common_saved());
      await loadLinks();
    } catch (e: unknown) {
      fail(e);
    }
  }

  async function revokeLink(id: string): Promise<void> {
    try {
      await api.revokePublicLink(id);
      notifyDone(m.common_saved());
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
  let copyBaseline = $state('');
  let copying = $state(false);
  interface CopyDraft {
    title: string;
    target: string;
    baseline: string;
    pending: { requestId: string; signature: string } | null;
  }
  let copyPending = $state<CopyDraft['pending']>(null);
  function resetCopy(): void {
    copyBaseline = m.documents_copy_default_title({ title: doc.title });
    copyTitle = copyBaseline;
    copyTarget = 'personal';
    copyPending = null;
  }
  const copyDraft = createFormDraft<CopyDraft>({
    name: () => `document-copy:${doc.id}:${org.currentId ?? ''}`,
    snapshot: () => ({
      title: copyTitle,
      target: copyTarget,
      baseline: copyBaseline,
      pending: copyPending,
    }),
    parse: (value) => {
      if (
        typeof value !== 'object' ||
        value === null ||
        !('title' in value) ||
        typeof value.title !== 'string' ||
        !('target' in value) ||
        typeof value.target !== 'string' ||
        !('baseline' in value) ||
        typeof value.baseline !== 'string' ||
        !('pending' in value)
      )
        return undefined;
      const pending = value.pending;
      let restoredPending: CopyDraft['pending'] = null;
      if (pending !== null) {
        if (
          typeof pending !== 'object' ||
          !('requestId' in pending) ||
          typeof pending.requestId !== 'string' ||
          !('signature' in pending) ||
          typeof pending.signature !== 'string'
        )
          return undefined;
        restoredPending = { requestId: pending.requestId, signature: pending.signature };
      }
      return {
        title: value.title,
        target: value.target,
        baseline: value.baseline,
        pending: restoredPending,
      };
    },
    restore: (value) => {
      copyTitle = value.title;
      copyTarget = value.target;
      copyBaseline = value.baseline;
      copyPending = value.pending;
    },
    reset: resetCopy,
    dirty: () => copyTitle !== copyBaseline || copyTarget !== 'personal' || copyPending !== null,
    busy: () => copying,
  });
  onMount(copyDraft.attach);
  let formIdentity: string | undefined;
  $effect(() => {
    const identity = JSON.stringify([me.userId, doc.id, org.currentId]);
    if (identity === formIdentity) return;
    formIdentity = identity;
    untrack(() => {
      resetShare();
      resetCopy();
      sharing = false;
      copying = false;
      panel = undefined;
      error = undefined;
    });
  });

  async function submitCopy(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (copying) return;
    const target = copyTarget === 'personal' ? null : copyTarget;
    const signature = JSON.stringify({ doc: doc.id, target, title: copyTitle });
    if (copyPending?.signature !== signature)
      copyPending = { requestId: newRequestId(), signature };
    const attempt = copyDraft.capture();
    if (attempt === undefined) return;
    copying = true;
    try {
      const copied = await api.copyDocument(doc.id, copyPending.requestId, target, copyTitle);
      if (!copyDraft.complete(attempt)) return;
      notifyDone(m.common_saved());
      panel = undefined;
      router.navigate(pathFor(copied, memberOf));
    } catch (e: unknown) {
      if (copyDraft.isCurrent(attempt)) fail(e);
    } finally {
      if (copyDraft.isOwner(attempt)) copying = false;
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
  {#if canManage}
    <Button
      size="sm"
      variant="outline"
      onclick={() => {
        open('share');
      }}>{m.documents_share_settings()}</Button
    >
  {/if}
  {@render actions?.()}
  <DropdownMenu.Root>
    <DropdownMenu.Trigger>
      {#snippet child({ props })}
        <Button {...props} size="sm" variant="outline" bind:ref={moreButton}
          >{m.documents_more_actions()}</Button
        >
      {/snippet}
    </DropdownMenu.Trigger>
    <DropdownMenu.Content align="start" class="min-w-44">
      {#if canCopy}
        <DropdownMenu.Item
          onSelect={() => {
            open('copy');
          }}>{m.documents_copy()}</DropdownMenu.Item
        >
      {/if}
      {#if isOwner && writable.length > 0}
        <DropdownMenu.Item
          onSelect={() => {
            open('move');
          }}>{m.documents_move()}</DropdownMenu.Item
        >
      {/if}
      {@render more?.()}
    </DropdownMenu.Content>
  </DropdownMenu.Root>
</div>

<Dialog.Root
  bind:open={
    () => panel !== undefined,
    (value: boolean) => {
      if (!value) open(undefined);
    }
  }
>
  <!-- 복제·이동은 "더 보기" 메뉴의 항목(닫히며 사라짐)에서 열린다. 닫으면 브라우저에 맡기지 않고 "더 보기"로 초점을 돌린다. -->
  <Dialog.Content
    class="max-h-[calc(100dvh-2rem)] grid-cols-[minmax(0,1fr)] overflow-y-auto"
    onCloseAutoFocus={(e: Event) => {
      if (opened !== 'copy' && opened !== 'move') return;
      e.preventDefault();
      moreButton?.focus();
    }}
  >
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
          {@const recipient = s.email ?? s.display_name}
          {@const effectivePermission = s.max_permission === 'read' ? 'read' : s.permission}
          <li class="flex flex-wrap items-center gap-2 text-sm">
            <span>{recipient}</span>
            <span class="text-muted-foreground"
              >{effectivePermission === 'write'
                ? m.documents_permission_write()
                : m.documents_permission_read()}</span
            >
            {#if s.recipient_role !== null}
              <span class="rounded border px-1 text-xs">{roleLabel(s.recipient_role)}</span>
            {/if}
            <span class="text-xs text-muted-foreground"
              >{m.documents_share_limit({
                permission:
                  s.max_permission === 'write'
                    ? m.documents_permission_write()
                    : m.documents_permission_read(),
              })}</span
            >
            {#if asking?.kind === 'share' && asking.id === s.id}
              <span class="ml-auto flex flex-wrap items-center gap-1">
                <span id="share-revoke-warning"
                  >{m.documents_unshare_confirm({ email: recipient })}</span
                >
                <Button
                  size="sm"
                  variant="destructive"
                  bind:ref={askConfirm}
                  aria-describedby="share-revoke-warning"
                  onclick={() => {
                    asking = undefined;
                    void removeShare(s.id);
                  }}>{m.documents_revoke()}</Button
                >
                <Button size="sm" variant="ghost" onclick={() => void stopAsking()}
                  >{m.common_cancel()}</Button
                >
              </span>
            {:else}
              <Button
                id={askId({ kind: 'share', id: s.id })}
                size="sm"
                variant="ghost"
                class="ml-auto"
                onclick={() => void ask({ kind: 'share', id: s.id })}>{m.documents_revoke()}</Button
              >
            {/if}
          </li>
        {:else}
          <li class="text-sm text-muted-foreground">{m.documents_share_empty()}</li>
        {/each}
      </ul>
      {#if canManage}
        <!-- 공유·복제는 보내는 동안 입력과 되살리기를 잠근다(D-97). 그사이 바뀐 입력은 성공(공유 저장·복제본으로 이동)과
             실패 알림을 숨기고, 복제는 다시 내면 새 요청 id로 두 번째 사본을 만든다. bits-ui Select는 조상 fieldset의
             disabled를 보지 않으므로 따로 잠근다. -->
        {#each shareDraft.offers as offer (offer.key)}
          <fieldset class="contents" disabled={sharing}>
            <FormDraftOffer
              onrestore={() => {
                shareDraft.restore(offer);
              }}
              ondiscard={() => {
                shareDraft.discard(offer);
              }}
            />
          </fieldset>
        {/each}
        <fieldset class="min-w-0" disabled={sharing}>
          <form class="flex flex-wrap items-end gap-2" onsubmit={(e) => void addShare(e)}>
            <div class="flex min-w-0 flex-1 basis-48 flex-col gap-1">
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
              disabled={sharing}
              value={offeredPermission}
              onValueChange={(v: string) => {
                sharePermission = v === 'write' && canOfferWrite ? 'write' : 'read';
              }}
            >
              <Select.Trigger class="w-24" aria-label={m.documents_share_permission()}
                >{offeredPermission === 'write'
                  ? m.documents_permission_write()
                  : m.documents_permission_read()}</Select.Trigger
              >
              <Select.Content>
                <Select.Item value="read" label={m.documents_permission_read()}
                  >{m.documents_permission_read()}</Select.Item
                >
                {#if canOfferWrite}
                  <Select.Item value="write" label={m.documents_permission_write()}
                    >{m.documents_permission_write()}</Select.Item
                  >
                {/if}
              </Select.Content>
            </Select.Root>
            <Button type="submit" disabled={sharing || shareEmail.trim() === ''}
              >{m.documents_share()}</Button
            >
          </form>
          {#if selectedRecipient !== undefined}
            <p class="mt-2 flex flex-wrap gap-2 text-sm" aria-live="polite">
              {#if selectedRecipient.recipient_role !== null}
                <span>{roleLabel(selectedRecipient.recipient_role)}</span>
              {/if}
              <span
                >{m.documents_share_limit({
                  permission:
                    selectedRecipient.max_permission === 'write'
                      ? m.documents_permission_write()
                      : m.documents_permission_read(),
                })}</span
              >
            </p>
          {:else if recipientState.kind === 'failed' && recipientState.key === recipientKey}
            <p class="mt-2 text-sm text-muted-foreground" aria-live="polite">
              {m.documents_recipient_unavailable()}
            </p>
          {/if}
        </fieldset>
      {/if}
      <div class="flex flex-col gap-2 border-t pt-3">
        {#if doc.placement !== 'personal'}
          <div class="flex items-center gap-2">
            <Checkbox
              id="doc-private"
              disabled={settingsBusy}
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
            disabled={settingsBusy}
            checked={doc.allow_copy}
            onCheckedChange={(v) => void setSettings({ allow_copy: v === true })}
          />
          <Label for="doc-allow-copy">{m.documents_allow_copy()}</Label>
        </div>
        {#if isOwner}
          <div class="flex items-center gap-2">
            <Checkbox
              id="doc-template"
              disabled={settingsBusy}
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
        <div role="status" class="empty:hidden">
          {#if !publicLinksEnabled}
            <p>{m.documents_public_links_disabled()}</p>
          {/if}
          {#if newLinkUrl !== undefined && publicLinksEnabled}
            <div class="flex flex-col gap-1 rounded border px-2 py-2">
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
        </div>
        <ul class="flex flex-col gap-1 text-sm" data-testid="public-link-list">
          {#each links as link (link.id)}
            <li class="flex flex-wrap items-center gap-2">
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
              {#if link.active && asking?.kind === 'link' && asking.id === link.id}
                <span class="ml-auto flex flex-wrap items-center gap-1">
                  <span id="link-revoke-warning">{m.documents_revoke_link_confirm()}</span>
                  <Button
                    size="sm"
                    variant="destructive"
                    bind:ref={askConfirm}
                    aria-describedby="link-revoke-warning"
                    onclick={() => {
                      asking = undefined;
                      void revokeLink(link.id);
                    }}>{m.documents_revoke()}</Button
                  >
                  <Button size="sm" variant="ghost" onclick={() => void stopAsking()}
                    >{m.common_cancel()}</Button
                  >
                </span>
              {:else if link.active}
                <Button
                  id={askId({ kind: 'link', id: link.id })}
                  size="sm"
                  variant="ghost"
                  class="ml-auto"
                  onclick={() => void ask({ kind: 'link', id: link.id })}
                  >{m.documents_revoke()}</Button
                >
              {/if}
            </li>
          {/each}
        </ul>
        {#if canManage && publicLinksEnabled}
          <div class="flex flex-wrap items-center gap-2">
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
      {#each copyDraft.offers as offer (offer.key)}
        <fieldset class="contents" disabled={copying}>
          <FormDraftOffer
            onrestore={() => {
              copyDraft.restore(offer);
            }}
            ondiscard={() => {
              copyDraft.discard(offer);
            }}
          />
        </fieldset>
      {/each}
      <fieldset class="min-w-0" disabled={copying}>
        <form class="flex flex-col gap-3" onsubmit={(e) => void submitCopy(e)}>
          <div class="flex flex-col gap-1">
            <Label for="copy-title">{m.documents_new_title()}</Label>
            <Input id="copy-title" bind:value={copyTitle} maxlength={200} required />
          </div>
          <div class="flex flex-col gap-1">
            <span class="text-sm font-medium" id="copy-target-label"
              >{m.documents_copy_target()}</span
            >
            <Select.Root
              type="single"
              disabled={copying}
              value={copyTarget}
              onValueChange={(v: string) => {
                // 보낼 때 대상·제목이 달라졌으면 새 요청 ID를 만든다.
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
            <Button
              variant="ghost"
              type="button"
              onclick={() => {
                open(undefined);
              }}>{m.common_cancel()}</Button
            >
            <Button type="submit" disabled={copying || copyTitle.trim() === ''}
              >{m.documents_copy()}</Button
            >
          </Dialog.Footer>
        </form>
      </fieldset>
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
        {@const gains = preview.changes
          .filter((c) => c.change === 'gain')
          .map((c) => c.display_name)}
        {@const loses = preview.changes
          .filter((c) => c.change === 'lose')
          .map((c) => c.display_name)}
        {@const regraded = preview.changes.filter((c) => c.change === 'change')}
        <div class="grid gap-2 text-sm md:grid-cols-2" data-testid="move-preview">
          <div>
            <h3 class="font-semibold">{m.documents_move_gains()}</h3>
            <p>{gains.length === 0 ? m.documents_none() : gains.join(', ')}</p>
          </div>
          <div>
            <h3 class="font-semibold">{m.documents_move_loses()}</h3>
            <p>{loses.length === 0 ? m.documents_none() : loses.join(', ')}</p>
          </div>
          {#if regraded.length > 0}
            <div class="md:col-span-2">
              <h3 class="font-semibold">{m.documents_move_changes()}</h3>
              <p>
                {regraded
                  .map((c) => `${c.display_name} (${c.from_grade ?? '—'} → ${c.to_grade ?? '—'})`)
                  .join(', ')}
              </p>
            </div>
          {/if}
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

<DiscardChangesConfirm
  open={shareDraft.leave.open}
  onkeep={shareDraft.leave.keep}
  ondiscard={shareDraft.discardAndLeave}
/>
<DiscardChangesConfirm
  open={copyDraft.leave.open}
  onkeep={copyDraft.leave.keep}
  ondiscard={copyDraft.discardAndLeave}
/>
