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

  $effect(() => {
    const id = documentId;
    let cancelled = false;
    doc = undefined;
    error = undefined;
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
  <div class="flex flex-wrap items-center gap-2">
    <DocumentActions
      {doc}
      {workspaces}
      onchanged={(next: DocumentView) => {
        // 설정·이동은 리비전을 바꾸지 않는다. 편집 중인 내용은 편집기가 그대로 가진다.
        if (doc !== undefined)
          doc = { ...doc, ...next, body: doc.body, title: doc.title, revision: doc.revision };
      }}
    />
    <RevisionHistory
      {doc}
      onrestored={(restored: DocumentView) => {
        doc = restored;
        epoch += 1;
      }}
    />
    {#if doc.can_export}
      <ExportDialog {doc} {unsaved} references={referencesFor(doc)} />
    {/if}
    <div class="ml-auto flex flex-wrap items-center gap-1">
      <MarkButtons kind="document" id={doc.id} />
      {#if doc.can_delete}
        {@const d = doc}
        <DeleteButton
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
  {#key doc.id}
    <ReferenceFocus root={bodyArea} target={parseReferenceHash(router.hash)} />
  {/key}
  <!-- 넓은 화면에서는 본문 옆에 목차(Outline처럼 읽기 위치를 잡기 쉽게). 좁은 화면에서는 본문만. -->
  <div class="lg:grid lg:grid-cols-[minmax(0,1fr)_14rem] lg:gap-6">
    <div bind:this={bodyArea} class="min-w-0">
      {#key `${doc.id}:${String(epoch)}:${String(doc.realtime)}`}
        {#if doc.realtime}
          <LiveDocumentEditor
            initial={doc}
            references={referencesFor(doc)}
            onsaved={(saved: DocumentView) => {
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
    <aside class="hidden lg:block">
      <div class="sticky top-4">
        <DocumentOutline root={bodyArea} />
      </div>
    </aside>
  </div>
  {#key doc.id}
    <ChildDocuments {doc} {workspaces} />
  {/key}
  <AttachmentsPanel
    target={{ kind: 'document', id: doc.id }}
    canWrite={doc.access === 'owner' || doc.access === 'write'}
  />
  <CommentsPanel target={{ kind: 'document', id: doc.id }} />
  {#key `${doc.id}:${String(epoch)}`}
    <Backlinks kind="document" id={doc.id} {workspaces} />
  {/key}
{/if}
