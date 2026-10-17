<script lang="ts">
  // 위키 문서 편집 화면 (첫 사용자 흐름).
  // - 저장은 자동(입력 후 잠시 멈추면)과 수동 버튼 둘 다. 저장 상태를 항상 보여준다.
  // - 같은 내용의 재시도는 같은 request_id를 다시 보낸다. 내용이 바뀌면 새 request_id.
  // - 서버가 409를 주면 초안을 버리지 않고, 서버 버전과 비교해 사용자가 고른다 (D-53).
  // - 미저장 초안은 로컬에 보관하고, 저장 전 이탈을 경고한다 (D-27).
  // 이 저장 방식은 JSON 리비전 저장이며 공동 편집이 아니다 (D-62).
  import { onMount, untrack } from 'svelte';

  import type { DocNode, DocumentView } from '$lib/api/client';
  import { api, ApiError, fileContentUrl, NetworkError, newRequestId } from '$lib/api/client';
  import { uploadFile } from '$lib/features/files/upload';
  import type { ReferenceContext } from '$lib/features/references/context';
  import { router, standalonePath } from '$lib/router.svelte';
  import { Button } from '$lib/ui/button';
  import * as Dialog from '$lib/ui/dialog';
  import { Input } from '$lib/ui/input';
  import { Label } from '$lib/ui/label';

  import { canonicalJson, clearDraft, loadDraft, plainText, storeDraft } from './drafts';
  import TiptapEditor from './TiptapEditor.svelte';

  interface Props {
    initial: DocumentView;
    onsaved?: (doc: DocumentView) => void;
    references?: ReferenceContext;
  }

  const { initial, onsaved, references }: Props = $props();

  const INLINE_IMAGES = ['image/png', 'image/jpeg', 'image/gif', 'image/webp'];

  /** 본문에 넣을 파일: 이 문서의 첨부로 올리고 그 버전의 주소를 돌려준다(D-27 이미지·첨부). */
  async function insertFile(file: File): Promise<{ href: string; name: string; image: boolean }> {
    const done = await uploadFile(file, { target: { kind: 'document', id: server.id } });
    if (done.file_id === null) throw new Error(done.failure ?? '업로드를 마치지 못했습니다');
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
  let server = $state<DocumentView>(start);
  let title = $state(start.title);
  let body = $state<DocNode>(start.body);
  let status = $state<Status>('saved');
  let message = $state<string | undefined>(undefined);
  let conflict = $state<DocumentView | undefined>(undefined);
  // 보관된 초안이 서버 내용과 같으면(이미 저장됨) 지우고 안내하지 않는다.
  function pendingDraft(): ReturnType<typeof loadDraft> {
    const draft = loadDraft(start.id);
    if (draft === undefined) return undefined;
    const same =
      canonicalJson({ title: draft.title.trim(), body: draft.body }) ===
      canonicalJson({ title: start.title, body: start.body });
    if (same) {
      clearDraft(start.id);
      return undefined;
    }
    return draft;
  }
  let restorable = $state(pendingDraft());
  let confirmOverwrite = $state(false);
  let editorVersion = $state(0);

  // 진행 중이거나 실패한 저장 시도. 내용이 같으면 같은 request_id로 재시도한다.
  let attempt: { requestId: string; snapshot: string } | undefined;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let destroyed = false;

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
    status = isDirty() ? 'dirty' : 'saved';
    if (status === 'dirty') {
      storeDraft(server.id, {
        baseRevision: server.revision,
        title,
        body,
        savedAt: new Date().toISOString(),
      });
      schedule(AUTOSAVE_DELAY_MS);
    }
  }

  async function save(): Promise<void> {
    if (!server.can_edit || status === 'saving' || status === 'conflict') return;
    if (!isDirty()) {
      status = 'saved';
      clearDraft(server.id);
      return;
    }
    if (title.trim() === '') {
      status = 'error';
      message = '제목을 입력하세요.';
      return;
    }
    const snap = snapshot();
    if (attempt?.snapshot !== snap) attempt = { requestId: newRequestId(), snapshot: snap };
    const { requestId } = attempt;
    status = 'saving';
    message = undefined;
    try {
      const result = await api.saveDocument(server.id, {
        requestId,
        baseRevision: server.revision,
        title: title.trim(),
        body,
      });
      if (destroyed) return;
      if (result.kind === 'conflict') {
        conflict = result.current;
        status = 'conflict';
        return;
      }
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
        message = '서버에 연결할 수 없어 다시 시도합니다. 편집 내용은 이 브라우저에 보관됩니다.';
        schedule(RETRY_DELAY_MS);
      } else if (error instanceof ApiError && error.status === 401) {
        status = 'signed-out';
        message = '로그인이 만료됐습니다. 편집 내용은 이 브라우저에 보관됩니다. 다시 로그인하세요.';
      } else {
        status = 'error';
        message = error instanceof Error ? error.message : '저장하지 못했습니다.';
      }
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

  async function keepMineAsCopy(): Promise<void> {
    const copyTitle = `${title.trim() === '' ? server.title : title.trim()} (내 초안)`;
    try {
      // 어디에 있는 문서든(공유받은 문서 포함) 항상 만들 수 있는 내 노트에 보존한다.
      const created = await api.createNote(newRequestId(), copyTitle);
      const saved = await api.saveDocument(created.id, {
        requestId: newRequestId(),
        baseRevision: created.revision,
        title: copyTitle,
        body,
      });
      if (saved.kind === 'conflict') throw new Error('copy conflicted');
      clearDraft(server.id);
      router.guard = undefined;
      router.navigate(standalonePath(created.id));
    } catch (error: unknown) {
      message =
        error instanceof Error
          ? `복사본을 만들지 못했습니다: ${error.message}`
          : '복사본을 만들지 못했습니다.';
    }
  }

  function restoreDraft(): void {
    if (restorable === undefined) return;
    title = restorable.title;
    body = restorable.body;
    editorVersion += 1;
    // 초안의 기준 리비전이 서버보다 낮으면 저장 시 충돌로 처리된다(서버 내용을 덮어쓰지 않는다).
    server = { ...server, revision: restorable.baseRevision };
    restorable = undefined;
    edited();
  }

  function discardDraft(): void {
    clearDraft(server.id);
    restorable = undefined;
  }

  const statusText: Record<Status, string> = {
    saved: '저장됨',
    dirty: '저장 대기',
    saving: '저장 중…',
    retrying: '재시도 대기',
    conflict: '충돌 — 선택 필요',
    error: '저장 실패',
    'signed-out': '로그인 필요',
  };

  onMount(() => {
    const beforeUnload = (event: BeforeUnloadEvent): void => {
      if (isDirty()) event.preventDefault();
    };
    window.addEventListener('beforeunload', beforeUnload);
    router.guard = () =>
      !isDirty() ||
      window.confirm(
        '저장하지 않은 변경이 있습니다. 이동할까요? (초안은 이 브라우저에 보관됩니다)',
      );
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
      <span class="rounded border px-1.5 font-mono" title="문서 참조">{server.reference}</span>
    {/if}
    <span>리비전 {server.revision}</span>
    <span class="ml-auto" role="status" aria-live="polite" data-testid="save-status"
      >{statusText[status]}</span
    >
    {#if server.can_edit}
      <Button
        size="sm"
        disabled={status === 'saving' || status === 'conflict'}
        onclick={() => void save()}>저장</Button
      >
    {:else}
      <span>읽기 전용</span>
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
          >다시 로그인</a
        >
      {/if}
    </p>
  {/if}

  {#if restorable !== undefined}
    <div
      class="flex flex-wrap items-center gap-2 rounded border px-3 py-2 text-sm"
      role="region"
      aria-label="보관된 초안"
    >
      <span
        >이 브라우저에 저장하지 않은 초안이 있습니다 ({new Date(
          restorable.savedAt,
        ).toLocaleString()}).</span
      >
      <Button size="sm" variant="outline" onclick={restoreDraft}>초안 복원</Button>
      <Button size="sm" variant="ghost" onclick={discardDraft}>초안 버리기</Button>
    </div>
  {/if}

  {#if conflict !== undefined}
    <section
      class="flex flex-col gap-3 rounded border border-amber-500/60 px-3 py-3"
      aria-labelledby="conflict-heading"
    >
      <h2 id="conflict-heading" class="font-semibold">다른 곳에서 먼저 저장됐습니다</h2>
      <p class="text-sm">
        서버에는 리비전 {conflict.revision}이 있습니다. 내 초안은 그대로 남아 있습니다. 비교한 뒤
        선택하세요.
      </p>
      <div class="grid gap-3 md:grid-cols-2">
        <div>
          <h3 class="text-sm font-semibold">서버 버전 — {conflict.title}</h3>
          <pre
            class="max-h-60 overflow-auto rounded bg-muted p-2 text-sm whitespace-pre-wrap"
            data-testid="conflict-server">{plainText(conflict.body)}</pre>
        </div>
        <div>
          <h3 class="text-sm font-semibold">내 초안 — {title}</h3>
          <pre
            class="max-h-60 overflow-auto rounded bg-muted p-2 text-sm whitespace-pre-wrap"
            data-testid="conflict-mine">{plainText(body)}</pre>
        </div>
      </div>
      <div class="flex flex-wrap gap-2">
        <Button size="sm" onclick={() => void keepMineAsCopy()}>내 초안을 내 노트로 보존</Button>
        <Button size="sm" variant="outline" onclick={() => (confirmOverwrite = true)}
          >내 초안으로 다시 저장</Button
        >
        <Button size="sm" variant="ghost" onclick={useServerVersion}>서버 버전으로 바꾸기</Button>
      </div>
    </section>
  {/if}

  <div class="flex flex-col gap-1">
    <Label id="doc-title-label" for="doc-title">제목</Label>
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
      label="본문"
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
      <Dialog.Title>내 초안으로 다시 저장할까요?</Dialog.Title>
      <Dialog.Description>
        서버의 리비전 {conflict?.revision}을 기준으로 내 초안을 새 리비전으로 저장합니다. 서버
        버전은 리비전 기록에 남습니다.
      </Dialog.Description>
    </Dialog.Header>
    <Dialog.Footer>
      <Dialog.Close>
        {#snippet child({ props })}
          <Button variant="ghost" {...props}>취소</Button>
        {/snippet}
      </Dialog.Close>
      <Button onclick={() => void overwriteWithMine()}>다시 저장</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
