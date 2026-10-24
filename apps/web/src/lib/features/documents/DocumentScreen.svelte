<script lang="ts">
  // 문서를 불러와 동작 막대와 편집기에 넘긴다. 다른 문서로 바뀌면 편집기를 새로 만든다.
  import MarkButtons from '$lib/features/home/MarkButtons.svelte';
  import type { Workspace } from '@fvoci/api-client';

  import type { DocumentView } from '$lib/api/client';
  import { api, ApiError } from '$lib/api/client';
  import AttachmentsPanel from '$lib/features/files/AttachmentsPanel.svelte';
  import CommentsPanel from '$lib/features/comments/CommentsPanel.svelte';
  import Backlinks from '$lib/features/references/Backlinks.svelte';
  import ChildDocuments from './ChildDocuments.svelte';
  import { parseReferenceHash } from '$lib/features/references/focus';
  import ReferenceFocus from '$lib/features/references/ReferenceFocus.svelte';
  import type { ReferenceContext } from '$lib/features/references/context';
  import { referenceHref } from '$lib/features/references/context';
  import { badgeColor, workspaceLabel } from '$lib/features/workspaces/badge';
  import { workspaceNav } from '$lib/features/nav/workspace-nav.svelte';
  import { m } from '$lib/paraglide/messages.js';
  import { itemPath, router, workspacePath } from '$lib/router.svelte';
  import * as DropdownMenu from '$lib/ui/dropdown-menu';
  import DeleteButton from '$lib/features/trash/DeleteButton.svelte';

  import DocumentActions from './DocumentActions.svelte';
  import DocumentEditor from './DocumentEditor.svelte';
  import DocumentOutline from './DocumentOutline.svelte';
  import ExportDialog from './ExportDialog.svelte';
  import LiveDocumentEditor from './LiveDocumentEditor.svelte';
  import RevisionHistory from './RevisionHistory.svelte';

  interface Props {
    documentId: string;
    workspaces: Workspace[];
  }

  const { documentId, workspaces }: Props = $props();

  let doc = $state<DocumentView | undefined>(undefined);
  let error = $state<string | undefined>(undefined);
  // 복원 뒤 편집기를 새 내용으로 다시 만든다(저장하지 않은 편집은 편집기의 초안 보존이 지킨다).
  let epoch = $state(0);
  // 편집기에 저장하지 않은 편집이 있는지(내보내기 안내).
  let unsaved = $state(false);
  // 목차가 읽는 본문 영역.
  let bodyArea = $state<HTMLDivElement | undefined>(undefined);
  // "더 보기" 메뉴에서 여는 내보내기·지우기 확인과, 그 일이 끝나면 초점을 돌려줄 "더 보기" 단추.
  let exportOpen = $state(false);
  let deleting = $state(false);
  let moreButton = $state<HTMLButtonElement | null>(null);

  $effect(() => {
    const id = documentId;
    let cancelled = false;
    doc = undefined;
    error = undefined;
    // 다른 문서로 옮기면 화면은 그대로라 앞 문서에서 연 지우기 확인·내보내기가 새 문서에 이어진다(리뷰: 확인을 누르면
    // 새 문서가 휴지통으로 간다). 문서가 바뀔 때 닫는다.
    deleting = false;
    exportOpen = false;
    api
      .document(id)
      .then((d) => {
        if (!cancelled) doc = d;
      })
      .catch((e: unknown) => {
        if (cancelled) return;
        error =
          e instanceof ApiError && e.status === 404
            ? m.documents_not_found()
            : m.documents_load_failed();
      });
    return () => {
      cancelled = true;
    };
  });
  /** 실시간이 꺼졌거나 권한이 바뀌었다. 문서를 다시 읽어 지금 상태(일반 편집·읽기 전용)로 다시 연다. */
  function reopen(): void {
    const id = documentId;
    api
      .document(id)
      .then((d) => {
        if (id !== documentId) return;
        doc = d;
        epoch += 1;
      })
      .catch((e: unknown) => {
        if (id !== documentId) return;
        error =
          e instanceof ApiError && e.status === 404
            ? m.documents_not_found()
            : m.documents_load_failed();
      });
  }
  /**
   * 지금 주소의 문서에 대한 결과인지. 다른 문서로 옮긴 뒤(같은 화면에서 문서만 바뀜) 늦게 온 이전 문서의 저장·복원·설정
   * 결과를 화면에 넣으면 새 주소 아래에 이전 문서가 다시 보인다.
   */
  const isShown = (d: DocumentView): boolean => d.id === documentId;
  const memberOf = (ws: string): boolean => workspaces.some((w) => w.id === ws);

  function referencesFor(d: DocumentView): ReferenceContext {
    return {
      workspaceId: d.workspace_id,
      describeWorkspace: (id) => {
        const ws = workspaces.find((w) => w.id === id);
        return ws === undefined
          ? undefined
          : { label: workspaceLabel(ws, workspaces), color: badgeColor(ws.id) };
      },
      hrefFor: (target) => referenceHref(target, memberOf),
      navigate: (path) => {
        router.navigate(path);
      },
    };
  }
</script>

{#if error !== undefined}
  <p role="alert" class="text-destructive">{error}</p>
{:else if doc === undefined}
  <p aria-live="polite">{m.documents_loading()}</p>
{:else}
  {@const shownDoc = doc}
  <div class="flex flex-wrap items-center gap-2">
    <DocumentActions
      {doc}
      {workspaces}
      bind:moreButton
      onchanged={(next: DocumentView) => {
        // 설정·이동은 리비전을 바꾸지 않는다. 편집 중인 내용은 편집기가 그대로 가진다.
        if (doc !== undefined && isShown(next))
          doc = { ...doc, ...next, body: doc.body, title: doc.title, revision: doc.revision };
      }}
    >
      {#snippet actions()}
        <RevisionHistory
          doc={shownDoc}
          onrestored={(restored: DocumentView) => {
            if (!isShown(restored)) return;
            doc = restored;
            epoch += 1;
          }}
        />
      {/snippet}
      {#snippet more()}
        {#if shownDoc.can_export}
          <DropdownMenu.Item onSelect={() => (exportOpen = true)}
            >{m.documents_export()}</DropdownMenu.Item
          >
        {/if}
        <DropdownMenu.Separator />
        <MarkButtons kind="document" id={shownDoc.id} layout="menu" />
        {#if shownDoc.can_delete}
          <DropdownMenu.Separator />
          <DropdownMenu.Item variant="destructive" onSelect={() => (deleting = true)}
            >{m.trash_delete_what({ what: m.documents_delete_what() })}</DropdownMenu.Item
          >
        {/if}
      {/snippet}
    </DocumentActions>
    {#if doc.can_export}
      <ExportDialog bind:open={exportOpen} {doc} {unsaved} references={referencesFor(doc)} />
    {/if}
    <div class="ml-auto flex flex-wrap items-center gap-1">
      {#if doc.can_delete}
        {@const d = doc}
        <!-- "더 보기"의 지우기를 고르면 여기서 한 번 더 묻는다. 취소하면 초점을 "더 보기"로 돌린다. -->
        <DeleteButton
          bind:confirming={deleting}
          trigger={false}
          oncancel={() => moreButton?.focus()}
          what={m.documents_delete_what()}
          remove={() => api.trashDocument(d.id)}
          ondeleted={() => {
            workspaceNav.refreshDocs();
            router.navigate(
              d.workspace_id === null
                ? '/notes'
                : d.item_id === null
                  ? workspacePath(d.workspace_id)
                  : itemPath(d.workspace_id, d.item_id),
            );
          }}
        />
      {/if}
    </div>
  </div>
  <!-- 본문 편집기와 같은 키: 복원·다시 열기로 본문을 새로 그리면 참조 자리도 다시 찾는다. -->
  {#key `${doc.id}:${String(epoch)}:${String(doc.realtime)}`}
    <ReferenceFocus root={bodyArea} target={parseReferenceHash(router.hash)} />
  {/key}
  <!-- 본문 영역이 넓으면 본문 옆에 목차(Outline처럼 읽기 위치를 잡기 쉽게). 좁거나 글꼴을 키웠으면 본문만. -->
  <div class="@2xl:grid @2xl:grid-cols-[minmax(0,1fr)_14rem] @2xl:gap-6">
    <div bind:this={bodyArea} class="min-w-0">
      {#key `${doc.id}:${String(epoch)}:${String(doc.realtime)}`}
        {#if doc.realtime}
          <LiveDocumentEditor
            initial={doc}
            references={referencesFor(doc)}
            onsaved={(saved: DocumentView) => {
              if (!isShown(saved)) {
                // 이전 문서의 제목 저장이 늦게 끝났다(제목 칸을 떠나며 다른 문서로 옮김). 화면은 그대로 두고 목록만
                // 새 제목으로 다시 읽는다.
                workspaceNav.refreshDocs();
                return;
              }
              // 같은 문서를 다시 연 뒤 앞 편집기의 제목 저장이 늦게 끝났으면(A → B → A) 다시 읽은 더 새 리비전을 옛 결과로
              // 덮지 않는다(기록·복원의 기준이 옛 리비전으로 돌아감). 목록만 새 제목으로 다시 읽는다.
              if (doc !== undefined && saved.revision < doc.revision) {
                workspaceNav.refreshDocs();
                return;
              }
              if (saved.title !== doc?.title) workspaceNav.refreshDocs();
              doc = saved;
            }}
            onrealtimeoff={reopen}
            onreload={reopen}
          />
        {:else}
          <DocumentEditor
            initial={doc}
            references={referencesFor(doc)}
            onsaved={(saved: DocumentView) => {
              // 기록 비교(지금과 비교)와 복원의 기준 리비전이 최신이 되도록 저장 결과를 따라간다.
              if (!isShown(saved)) return;
              if (saved.title !== doc?.title) workspaceNav.refreshDocs();
              doc = saved;
            }}
            onunsaved={(value: boolean) => {
              unsaved = value;
            }}
          />
        {/if}
      {/key}
    </div>
    <aside class="hidden @2xl:block">
      <div class="sticky top-4">
        <DocumentOutline root={bodyArea} />
      </div>
    </aside>
  </div>
  <!-- 본문 바로 아래에 이 문서와 이어진 문서(하위 문서, 이곳을 가리키는 문서)를 함께 둔다. 첨부·댓글은 그다음. -->
  {#key doc.id}
    <ChildDocuments {doc} {workspaces} />
  {/key}
  {#key `${doc.id}:${String(epoch)}`}
    <Backlinks kind="document" id={doc.id} {workspaces} />
  {/key}
  <AttachmentsPanel
    target={{ kind: 'document', id: doc.id }}
    canWrite={doc.access === 'owner' || doc.access === 'write'}
  />
  <CommentsPanel target={{ kind: 'document', id: doc.id }} />
{/if}
