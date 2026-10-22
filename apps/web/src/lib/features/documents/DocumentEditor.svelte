<script lang="ts">
  // 위키 문서 편집 화면 (첫 사용자 흐름).
  // - 저장은 자동(입력 후 잠시 멈추면)과 수동 버튼 둘 다. 저장 상태를 항상 보여준다.
  // - 같은 내용의 재시도는 같은 request_id를 다시 보낸다. 내용이 바뀌면 새 request_id.
  // - 서버가 409를 주면 초안을 버리지 않고, 서버 버전과 비교해 사용자가 고른다 (D-53).
  // - 미저장 초안은 로컬에 보관하고, 저장 전 이탈을 경고한다 (D-27).
  // 이 저장 방식은 JSON 리비전 저장이며 공동 편집이 아니다 (D-62).
  import { formatInstant } from '$lib/display';
  import { onMount, untrack } from 'svelte';

  import type { DocNode, DocumentView } from '$lib/api/client';
  import { api, ApiError, fileContentUrl, NetworkError, newRequestId } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import { RetryKey } from '$lib/api/retry-key';
  import {
    accountSwitching,
    reportSessionExpired,
    switchAccount,
  } from '$lib/features/feedback/feedback.svelte';
  import { uploadFile } from '$lib/features/files/upload';
  import { me as currentUser } from '$lib/me.svelte';
  import type { ReferenceContext } from '$lib/features/references/context';
  import { m } from '$lib/paraglide/messages.js';
  import { router, standalonePath } from '$lib/router.svelte';
  import { Button } from '$lib/ui/button';
  import * as Dialog from '$lib/ui/dialog';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';

  import { describeSaveBlock } from './content-notices';
  import type { Draft } from './drafts';
  import {
    canonicalJson,
    adoptOrphans,
    clearDraft,
    loadDraft,
    loadOffers,
    plainText,
    removeOffer,
    storeDraft,
  } from './drafts';
  import { contentIssues } from './paste-guard';
  import TiptapEditor from './TiptapEditor.svelte';

  interface Props {
    initial: DocumentView;
    onsaved?: (doc: DocumentView) => void;
    references?: ReferenceContext;
    /** 저장하지 않은 편집이 있는지 알린다(내보내기 안내, D-75). */
    onunsaved?: (unsaved: boolean) => void;
  }

  const { initial, onsaved, references, onunsaved }: Props = $props();

  const INLINE_IMAGES = ['image/png', 'image/jpeg', 'image/gif', 'image/webp'];

  /** 본문에 넣을 파일: 이 문서의 첨부로 올리고 그 버전의 주소를 돌려준다(D-27 이미지·첨부). */
  async function insertFile(file: File): Promise<{ href: string; name: string; image: boolean }> {
    const done = await uploadFile(file, {
      expectedUser: editingUser,
      target: { kind: 'document', id: server.id },
    });
    if (done.file_id === null) throw new Error(done.failure ?? m.documents_upload_failed());
    const image = INLINE_IMAGES.includes(file.type);
    return {
      href: fileContentUrl(done.file_id, done.version ?? undefined, image),
      name: file.name,
      image,
    };
  }

  type Status = 'saved' | 'dirty' | 'saving' | 'retrying' | 'conflict' | 'error' | 'signed-out';

  // 서버가 확인한 최신 상태. 편집 내용은 따로 둔다.
  // 부모가 문서마다 {#key}로 새로 만들므로 처음 값만 읽는다.
  const start = untrack(() => initial);
  // 이 편집을 시작한 사용자. 저장 요청마다 그대로 보내 서버가 세션의 사용자와 비교한다(#60). 보내기 직전에 지금
  // 로그인한 사용자로 다시 채우지 않는다(그사이 다른 탭에서 계정을 바꿨으면 그 계정으로 조용히 바뀐다).
  const editingUser = untrack(() => currentUser.userId);
  let server = $state<DocumentView>(start);
  let title = $state(start.title);
  let body = $state<DocNode>(start.body);
  let status = $state<Status>('saved');
  $effect(() => {
    onunsaved?.(status !== 'saved');
  });
  // 편집기가 사라지면(실시간 편집으로 바뀌는 등) 저장하지 않은 편집 표시도 거둔다.
  $effect(() => () => {
    onunsaved?.(false);
  });
  let message = $state<string | undefined>(undefined);
  let conflict = $state<DocumentView | undefined>(undefined);
  // 저장하는 사이 실시간 공동 편집이 켜져 본문을 여기서 저장할 수 없다.
  let realtimeOn = $state(false);
  // 고르게 할 초안이 실시간 편집기가 남긴 줄의 것인지. 그렇다면 고르기 전까지 줄에 그대로 두고(이 편집기의 입력이
  // 덮지 않게), 되살리거나 버릴 때만 줄에서 뺀다.
  let restorableQueued = false;
  // 보관된 초안이 서버 내용과 같으면(이미 저장됨) 지우고 안내하지 않는다. 이 편집기의 초안이 없으면 실시간 편집기가
  // 남긴 줄의 첫 초안을 고르게 한다.
  function pendingDraft(): ReturnType<typeof loadDraft> {
    const isServer = (d: Draft): boolean =>
      canonicalJson({ title: d.title.trim(), body: d.body }) ===
      canonicalJson({ title: start.title, body: start.body });
    const draft = loadDraft(start.id);
    if (draft !== undefined) {
      if (!isServer(draft)) return draft;
      clearDraft(start.id);
    }
    for (const queued of loadOffers(start.id)) {
      if (isServer(queued)) {
        removeOffer(start.id, queued);
        continue;
      }
      restorableQueued = true;
      return queued;
    }
    return undefined;
  }
  let restorable = $state(pendingDraft());
  // 닫힌 탭이 남긴 이 문서의 초안을 줄로 넘겨받는다(살아 있는 탭의 것은 두고). 고를 것이 없으면 그것을 고르게 한다.
  void adoptOrphans(start.id).then((adopted) => {
    if (adopted > 0 && restorable === undefined) restorable = pendingDraft();
  });
  let confirmOverwrite = $state(false);
  let editorVersion = $state(0);

  // 진행 중이거나 실패한 저장 시도. 내용이 같으면 같은 request_id로 재시도한다.
  let attempt: { requestId: string; snapshot: string } | undefined;
  // 표현만 바뀐 412에서 한 번 다시 저장했는지(되풀이 방지).
  let representationRetried = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let destroyed = false;
  // 이 창을 연 계정이 아닌 계정으로 바뀐 것을 알았다(보내기 전 확인 또는 서버의 409 `account_changed`). 이 창은 그
  // 계정으로 다시 읽히는 중이다: 더 보내지 않고(예약·재시도·직접 저장 모두) 편집은 앞 계정의 초안으로만 남긴다.
  let accountChanged = false;

  const AUTOSAVE_DELAY_MS = 1500;
  const RETRY_DELAY_MS = 5000;

  // 서버(jsonb)는 객체 키 순서를 바꿔 돌려준다. 비교는 키를 정렬한 정규형으로 한다.
  function snapshot(): string {
    return canonicalJson({ title: title.trim(), body });
  }

  function isDirty(): boolean {
    return snapshot() !== canonicalJson({ title: server.title, body: server.body });
  }

  function schedule(delay: number): void {
    if (timer !== undefined) clearTimeout(timer);
    timer = setTimeout(() => {
      timer = undefined;
      void save();
    }, delay);
  }

  function edited(): void {
    if (!server.can_edit || status === 'conflict') return;
    const signedOut = status === 'signed-out';
    status = isDirty() ? 'dirty' : 'saved';
    if (status === 'dirty') {
      storeDraft(server.id, {
        baseRevision: server.revision,
        title,
        body,
        savedAt: new Date().toISOString(),
      });
      if (!accountChanged) schedule(AUTOSAVE_DELAY_MS);
    } else {
      // 서버 내용과 같아졌으면(문제 내용을 지우는 등) 지난 저장 안내는 더 필요 없다(로그아웃 안내는 남긴다).
      // 복원 제안 중인 초안은 사용자가 고를 때까지 남긴다.
      // 실시간이 켜져 여기서 저장할 수 없다는 안내(사본 단추)는 남긴다.
      if (!signedOut && !realtimeOn) message = undefined;
      if (restorable === undefined) clearDraft(server.id);
    }
  }

  async function save(): Promise<void> {
    if (
      !server.can_edit ||
      status === 'saving' ||
      status === 'conflict' ||
      realtimeOn ||
      accountChanged
    )
      return;
    if (!isDirty()) {
      if (status !== 'signed-out') message = undefined;
      status = 'saved';
      if (restorable === undefined) clearDraft(server.id);
      return;
    }
    if (title.trim() === '') {
      status = 'error';
      message = m.documents_title_required();
      return;
    }
    // 서버가 받지 않을 내용(바깥 이미지·허용하지 않는 링크)은 보내지 않고 이유와 고칠 방법을 보인다.
    // 초안은 edited()가 이미 이 브라우저에 남겼다.
    const blocked = describeSaveBlock(contentIssues(body));
    if (blocked !== undefined) {
      status = 'error';
      message = blocked;
      return;
    }
    // 보내기 전에 이 창을 연 계정이 아직 로그인돼 있는지 본다. 다른 탭에서 다른 계정으로 로그인하면 쿠키가 바뀌어,
    // 이 창이 돌아오기(focus) 전에 나가는 예약·재시도 저장이 앞 계정의 편집을 새 계정의 이름으로 남긴다. 직접 누른
    // 저장도 화면은 앞 계정의 것이라 같게 본다. 확인하지 못하면 보내지 않는다. 요청 id·보낼 내용은 확인 뒤에 정한다.
    // 확인과 전송 사이에 바뀐 경우는 서버가 막는다(요청의 기대 사용자, 아래 409 `account_changed`). 이 확인은 그
    // 전에 이미 보이는 전환에서 본문을 아예 보내지 않게 하는 앞단 방어로 남긴다.
    status = 'saving';
    const who = await signedInAs();
    if (who === 'gone') return;
    if (who === 'other') {
      // 입력은 앞 계정의 초안(사용자별 키)으로 이미 남아 있다(edited()).
      status = 'dirty';
      stopSending();
      switchAccount();
      return;
    }
    if (who === 'signed-out') {
      status = 'signed-out';
      message = m.documents_signed_out();
      reportSessionExpired();
      return;
    }
    if (who === 'unknown') {
      status = 'retrying';
      message = m.documents_save_retrying();
      schedule(RETRY_DELAY_MS);
      return;
    }
    const snap = snapshot();
    if (attempt?.snapshot !== snap) attempt = { requestId: newRequestId(), snapshot: snap };
    const { requestId } = attempt;
    status = 'saving';
    message = undefined;
    try {
      const result = await api.saveDocument(server.id, {
        expectedUser: editingUser,
        requestId,
        baseRevision: server.revision,
        title: title.trim(),
        body,
        etag: server.etag,
      });
      if (destroyed) return;
      if (result.kind === 'conflict') {
        if (result.current.revision === server.revision && !representationRetried) {
          // 412인데 리비전이 그대로다: 내용은 같고 이 사람에게 보이는 표현(권한 표시 등)만 바뀌었다.
          // 새 ETag로 한 번만 다시 저장한다(D-77). 다시 맞지 않으면 아래의 충돌 화면으로 간다.
          representationRetried = true;
          server = result.current;
          status = 'dirty';
          void save();
          return;
        }
        representationRetried = false;
        conflict = result.current;
        status = 'conflict';
        return;
      }
      representationRetried = false;
      attempt = undefined;
      server = result.document;
      onsaved?.(result.document);
      if (isDirty()) {
        // 저장하는 동안 더 고쳤다. 이어서 저장한다.
        status = 'dirty';
        schedule(AUTOSAVE_DELAY_MS);
      } else {
        status = 'saved';
        clearDraft(server.id);
      }
    } catch (error: unknown) {
      if (destroyed) return;
      if (error instanceof NetworkError) {
        status = 'retrying';
        message = m.documents_save_retrying();
        schedule(RETRY_DELAY_MS);
      } else if (error instanceof ApiError && error.status === 401) {
        status = 'signed-out';
        message = m.documents_signed_out();
      } else if (error instanceof ApiError && error.code === 'account_changed') {
        // 이 편집을 시작한 계정이 아닌 세션으로 나갔다. 서버는 아무것도 바꾸지 않았다. 다시 보내지 않는다(예약도
        // 걸지 않는다). 입력은 앞 계정의 초안(사용자별 키, edited())에 남아 있고, 계정 전환 처리(다시 읽기)는
        // client.ts가 이미 시작했다.
        status = 'dirty';
        message = m.documents_account_changed();
        stopSending();
      } else if (
        error instanceof ApiError &&
        error.code === 'invalid_state' &&
        (await realtimeTurnedOn())
      ) {
        // 실시간이 켜진 뒤에는 본문을 실시간 연결로만 고친다. 고친 내용(초안)은 사본으로 남길 수 있게 한다.
        status = 'error';
        realtimeOn = true;
        message = m.documents_save_realtime_on();
      } else {
        status = 'error';
        message = errorText(error, m.documents_save_failed());
      }
    }
  }

  /** 다른 계정으로 바뀐 것을 알았다: 걸어 둔 예약·재시도를 거두고 이 편집기에서 더 보내지 않는다. */
  function stopSending(): void {
    accountChanged = true;
    if (timer !== undefined) clearTimeout(timer);
    timer = undefined;
  }

  /**
   * 지금 로그인한 계정이 이 창을 연 계정인지. 묻지 못했거나(연결 오류 등) 이 창의 계정을 모르면 `unknown`, 묻는 사이
   * 편집기가 사라졌으면 `gone`.
   */
  async function signedInAs(): Promise<'same' | 'other' | 'signed-out' | 'unknown' | 'gone'> {
    const mine = editingUser;
    if (mine === '') return 'unknown';
    let now: Awaited<ReturnType<typeof api.me>>;
    try {
      now = await api.me();
    } catch {
      return destroyed ? 'gone' : 'unknown';
    }
    if (destroyed) return 'gone';
    if (now === undefined) return 'signed-out';
    return now.id === mine ? 'same' : 'other';
  }

  /** 저장이 409(상태)로 거절됐을 때 그사이 실시간이 켜졌는지. 확인하지 못하면 아니라고 본다. */
  async function realtimeTurnedOn(): Promise<boolean> {
    try {
      return (await api.document(server.id)).realtime;
    } catch {
      return false;
    }
  }

  function useServerVersion(): void {
    if (conflict === undefined) return;
    server = conflict;
    title = conflict.title;
    body = conflict.body;
    editorVersion += 1;
    conflict = undefined;
    attempt = undefined;
    clearDraft(server.id);
    status = 'saved';
    message = undefined;
  }

  async function overwriteWithMine(): Promise<void> {
    if (conflict === undefined) return;
    // 서버 최신본을 확인했다는 뜻으로 기준 리비전을 옮긴 뒤 내 초안을 저장한다.
    server = conflict;
    conflict = undefined;
    attempt = undefined;
    confirmOverwrite = false;
    status = 'dirty';
    await save();
  }

  const copyNoteKey = new RetryKey();
  const copySaveKey = new RetryKey();

  async function keepMineAsCopy(): Promise<void> {
    const copyTitle = m.documents_draft_copy_title({
      title: title.trim() === '' ? server.title : title.trim(),
    });
    // 사본도 같은 서버 규칙으로 저장되므로, 받지 않을 내용이면 노트를 만들기 전에 이유를 보인다.
    const blocked = describeSaveBlock(contentIssues(body));
    if (blocked !== undefined) {
      message = blocked;
      return;
    }
    try {
      // 어디에 있는 문서든(공유받은 문서 포함) 항상 만들 수 있는 내 노트에 보존한다. 응답을 잃고 다시 눌러도
      // 같은 내용이면 같은 노트가 된다.
      const noteRequest = copyNoteKey.for({ copyTitle });
      const created = await api.createNote(editingUser, noteRequest, copyTitle);
      const saveRequest = copySaveKey.for({ note: created.id, copyTitle, body });
      const saved = await api.saveDocument(created.id, {
        expectedUser: editingUser,
        requestId: saveRequest,
        baseRevision: created.revision,
        title: copyTitle,
        body,
      });
      if (saved.kind === 'conflict') throw new Error('copy conflicted');
      copyNoteKey.done(noteRequest);
      copySaveKey.done(saveRequest);
      clearDraft(server.id);
      router.guard = undefined;
      router.navigate(standalonePath(created.id));
    } catch (error: unknown) {
      message = errorText(error, m.documents_copy_failed());
    }
  }

  function restoreDraft(): void {
    if (restorable === undefined) return;
    // 줄의 초안이면 이 편집기의 초안으로 먼저 옮겨 둔 뒤에 줄에서 뺀다(옮기지 못하면 줄에 남긴다).
    if (restorableQueued && storeDraft(server.id, restorable)) {
      removeOffer(server.id, restorable);
      restorableQueued = false;
    }
    title = restorable.title;
    body = restorable.body;
    editorVersion += 1;
    // 초안의 기준 리비전이 서버보다 낮으면 저장 시 충돌로 처리된다(서버 내용을 덮어쓰지 않는다).
    server = { ...server, revision: restorable.baseRevision };
    restorable = undefined;
    edited();
  }

  function discardDraft(): void {
    if (restorable !== undefined && restorableQueued) removeOffer(server.id, restorable);
    else clearDraft(server.id);
    restorableQueued = false;
    restorable = undefined;
  }

  function statusText(s: Status): string {
    switch (s) {
      case 'saved':
        return m.documents_status_saved();
      case 'dirty':
        return m.documents_status_dirty();
      case 'saving':
        return m.documents_status_saving();
      case 'retrying':
        return m.documents_status_retrying();
      case 'conflict':
        return m.documents_status_conflict();
      case 'error':
        return m.documents_status_error();
      case 'signed-out':
        return m.documents_status_signed_out();
    }
  }

  onMount(() => {
    const beforeUnload = (event: BeforeUnloadEvent): void => {
      if (!isDirty()) return;
      // 다른 계정으로 바뀌어 다시 읽을 때는 지금 입력을 앞 계정의 초안으로 남길 수 있으면 막지 않는다. 남기지
      // 못하면(저장소를 쓸 수 없음) 보통 떠나기처럼 경고한다(남아도 저장은 계정 확인에서 막힌다).
      const kept =
        accountSwitching() &&
        storeDraft(server.id, {
          baseRevision: server.revision,
          title,
          body,
          savedAt: new Date().toISOString(),
        });
      if (!kept) event.preventDefault();
    };
    window.addEventListener('beforeunload', beforeUnload);
    router.guard = () => !isDirty() || window.confirm(m.documents_leave_confirm());
    return () => {
      destroyed = true;
      if (timer !== undefined) clearTimeout(timer);
      window.removeEventListener('beforeunload', beforeUnload);
      router.guard = undefined;
    };
  });
</script>

<article class="flex flex-col gap-4" aria-labelledby="doc-title-label">
  <div class="flex flex-wrap items-center gap-2 text-sm text-muted-foreground">
    {#if server.reference !== null}
      <span class="rounded border px-1.5 font-mono" title={m.documents_reference_title()}
        >{server.reference}</span
      >
    {/if}
    <span>{m.documents_revision_n({ revision: server.revision })}</span>
    <span class="ml-auto" role="status" aria-live="polite" data-testid="save-status"
      >{statusText(status)}</span
    >
    {#if server.can_edit}
      <Button
        size="sm"
        disabled={status === 'saving' || status === 'conflict'}
        onclick={() => void save()}>{m.common_save()}</Button
      >
    {:else}
      <span>{m.documents_read_only()}</span>
    {/if}
  </div>

  {#if message !== undefined}
    <p
      class="rounded border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm"
      role="alert"
    >
      {message}
      {#if status === 'signed-out'}
        <a class="underline" href={`/auth/login?return_to=${encodeURIComponent(router.path)}`}
          >{m.documents_sign_in_again()}</a
        >
      {/if}
      {#if realtimeOn}
        <Button size="sm" onclick={() => void keepMineAsCopy()}
          >{m.documents_conflict_keep_copy()}</Button
        >
      {/if}
    </p>
  {/if}

  {#if restorable !== undefined}
    <div
      class="flex flex-wrap items-center gap-2 rounded border px-3 py-2 text-sm"
      role="region"
      aria-label={m.documents_draft_region()}
    >
      <span>{m.documents_draft_found({ time: formatInstant(restorable.savedAt) })}</span>
      <Button size="sm" variant="outline" onclick={restoreDraft}
        >{m.documents_draft_restore()}</Button
      >
      <Button size="sm" variant="ghost" onclick={discardDraft}>{m.documents_draft_discard()}</Button
      >
    </div>
  {/if}

  {#if conflict !== undefined}
    <section
      class="flex flex-col gap-3 rounded border border-amber-500/60 px-3 py-3"
      aria-labelledby="conflict-heading"
    >
      <h2 id="conflict-heading" class="font-semibold">{m.documents_conflict_heading()}</h2>
      <p class="text-sm">
        {m.documents_conflict_body({ revision: conflict.revision })}
      </p>
      <div class="grid gap-3 md:grid-cols-2">
        <div>
          <h3 class="text-sm font-semibold">
            {m.documents_conflict_server({ title: conflict.title })}
          </h3>
          <pre
            class="max-h-60 overflow-auto rounded bg-muted p-2 text-sm whitespace-pre-wrap"
            data-testid="conflict-server">{plainText(conflict.body)}</pre>
        </div>
        <div>
          <h3 class="text-sm font-semibold">{m.documents_conflict_mine({ title })}</h3>
          <pre
            class="max-h-60 overflow-auto rounded bg-muted p-2 text-sm whitespace-pre-wrap"
            data-testid="conflict-mine">{plainText(body)}</pre>
        </div>
      </div>
      <div class="flex flex-wrap gap-2">
        <Button size="sm" onclick={() => void keepMineAsCopy()}
          >{m.documents_conflict_keep_copy()}</Button
        >
        <Button size="sm" variant="outline" onclick={() => (confirmOverwrite = true)}
          >{m.documents_conflict_overwrite()}</Button
        >
        <Button size="sm" variant="ghost" onclick={useServerVersion}
          >{m.documents_conflict_use_server()}</Button
        >
      </div>
    </section>
  {/if}

  <div class="flex flex-col gap-1">
    <Label id="doc-title-label" for="doc-title">{m.documents_title_label()}</Label>
    <Input
      id="doc-title"
      bind:value={title}
      readonly={!server.can_edit}
      maxlength={200}
      oninput={edited}
      class="text-lg font-semibold"
    />
  </div>

  <!-- 서버 버전·보관 초안으로 내용을 바꿀 때는 편집기를 새로 만든다(인스턴스는 마운트마다 하나). -->
  {#key editorVersion}
    <TiptapEditor
      content={body}
      editable={server.can_edit}
      label={m.documents_body_label()}
      {references}
      insertFile={server.can_edit ? insertFile : undefined}
      onchange={(next: DocNode) => {
        body = next;
        edited();
      }}
    />
  {/key}
</article>

<Dialog.Root bind:open={confirmOverwrite}>
  <Dialog.Content>
    <Dialog.Header>
      <Dialog.Title>{m.documents_overwrite_title()}</Dialog.Title>
      <Dialog.Description>
        {m.documents_overwrite_description({ revision: conflict?.revision ?? '' })}
      </Dialog.Description>
    </Dialog.Header>
    <Dialog.Footer>
      <Dialog.Close>
        {#snippet child({ props })}
          <Button variant="ghost" {...props}>{m.common_cancel()}</Button>
        {/snippet}
      </Dialog.Close>
      <Button onclick={() => void overwriteWithMine()}>{m.documents_overwrite_confirm()}</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
