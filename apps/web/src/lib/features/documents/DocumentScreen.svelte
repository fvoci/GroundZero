<script lang="ts">
  // 문서를 불러와 동작 막대와 편집기에 넘긴다. 다른 문서로 바뀌면 편집기를 새로 만든다.
  import MarkButtons from '$lib/features/home/MarkButtons.svelte';
  import type { Workspace } from '@fvoci/api-client';

  import type { DocumentView } from '$lib/api/client';
  import { api, ApiError } from '$lib/api/client';
  import AttachmentsPanel from '$lib/features/files/AttachmentsPanel.svelte';
  import Backlinks from '$lib/features/references/Backlinks.svelte';
  import type { ReferenceContext } from '$lib/features/references/context';
  import { referenceHref } from '$lib/features/references/context';
  import { badgeColor, workspaceLabel } from '$lib/features/workspaces/badge';
  import { itemPath, router, workspacePath } from '$lib/router.svelte';
  import DeleteButton from '$lib/features/trash/DeleteButton.svelte';

  import DocumentActions from './DocumentActions.svelte';
  import DocumentEditor from './DocumentEditor.svelte';
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
            ? '문서가 없거나 볼 권한이 없습니다.'
            : '문서를 불러오지 못했습니다.';
      });
    return () => {
      cancelled = true;
    };
  });
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
  <p aria-live="polite">불러오는 중…</p>
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
    <div class="ml-auto flex flex-wrap items-center gap-1">
      <MarkButtons kind="document" id={doc.id} />
      {#if doc.can_delete}
        {@const d = doc}
        <DeleteButton
          what="문서"
          remove={() => api.trashDocument(d.id)}
          ondeleted={() => {
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
  {#key `${doc.id}:${String(epoch)}`}
    <DocumentEditor
      initial={doc}
      references={referencesFor(doc)}
      onsaved={(saved: DocumentView) => {
        // 기록 비교(지금과 비교)와 복원의 기준 리비전이 최신이 되도록 저장 결과를 따라간다.
        doc = saved;
      }}
    />
  {/key}
  <AttachmentsPanel
    target={{ kind: 'document', id: doc.id }}
    canWrite={doc.access === 'owner' || doc.access === 'write'}
  />
  {#key `${doc.id}:${String(epoch)}`}
    <Backlinks kind="document" id={doc.id} {workspaces} />
  {/key}
{/if}
