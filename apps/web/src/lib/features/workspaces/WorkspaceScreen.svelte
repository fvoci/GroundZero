<script lang="ts">
  // 워크스페이스의 위키 문서 목록과 새 문서 만들기.
  import type { Workspace } from '@fvoci/api-client';

  import { api, newRequestId } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import DocumentTree from '$lib/features/documents/DocumentTree.svelte';
  import type { TreeMove } from '$lib/features/documents/tree';
  import { m } from '$lib/paraglide/messages.js';
  import { documentPath, router } from '$lib/router.svelte';

  import ModulesSection from '$lib/features/modules/ModulesSection.svelte';
  import { workspaceNav } from '$lib/features/nav/workspace-nav.svelte';

  import TemplatePicker from '$lib/features/documents/TemplatePicker.svelte';

  import CreateForm from './CreateForm.svelte';

  interface Props {
    workspace: Workspace;
  }

  const { workspace }: Props = $props();

  // 위키 문서 목록은 사이드바와 같이 쓴다(한 번만 받는다).
  $effect(() => {
    workspaceNav.show(workspace.id);
  });
  const docs = $derived(
    workspaceNav.workspaceId === workspace.id && workspaceNav.docs.state === 'ready'
      ? workspaceNav.docs.list
      : undefined,
  );
  const error = $derived(
    workspaceNav.workspaceId === workspace.id && workspaceNav.docs.state === 'error'
      ? errorText(workspaceNav.docs.error, m.workspaces_docs_load_failed())
      : undefined,
  );
  let templateId = $state('');

  const canWrite = $derived(workspace.role !== 'reader');

  // 실시간 공동 편집(D-27, D-70). 관리자만 켜고 끈다. 끄면 열린 편집은 리비전을 남기고 닫힌다.
  let realtime = $state(false);
  let realtimeMessage = $state('');
  $effect(() => {
    realtime = workspace.realtime_enabled;
    realtimeMessage = '';
  });
  async function setRealtime(on: boolean): Promise<void> {
    try {
      await api.setWorkspaceRealtime(workspace.id, on);
      realtime = on;
      realtimeMessage = on ? m.workspaces_realtime_on() : m.workspaces_realtime_off();
    } catch (e: unknown) {
      realtimeMessage = errorText(e, m.workspaces_realtime_failed());
    }
  }
</script>

<section class="flex flex-col gap-6" aria-labelledby="ws-heading">
  <h1 id="ws-heading" class="text-xl font-semibold">
    {m.workspaces_wiki_heading({ workspace: workspace.name })}
  </h1>
  <p class="text-sm text-muted-foreground">
    {m.workspaces_admin_notice()}
  </p>
  {#if workspace.role === 'admin'}
    <div class="flex flex-col gap-1 text-sm">
      <label class="flex items-center gap-2">
        <input
          type="checkbox"
          checked={realtime}
          onchange={(e: Event) => {
            if (e.currentTarget instanceof HTMLInputElement)
              void setRealtime(e.currentTarget.checked);
          }}
        />
        {m.workspaces_realtime_label()}
      </label>
      <p class="text-muted-foreground">
        {m.workspaces_realtime_hint()}
      </p>
      <p aria-live="polite">{realtimeMessage}</p>
    </div>
  {/if}
  {#if error !== undefined}
    <p role="alert" class="text-destructive">{error}</p>
  {:else if docs === undefined}
    <p aria-live="polite">{m.app_loading()}</p>
  {:else if docs.length === 0}
    <p class="text-muted-foreground">{m.workspaces_wiki_empty()}</p>
  {:else}
    <DocumentTree
      {docs}
      label={m.workspaces_wiki_tree({ workspace: workspace.name })}
      hrefFor={(id: string) => documentPath(workspace.id, id)}
      navigate={(path: string) => {
        router.navigate(path);
      }}
      {canWrite}
      onmove={async (id: string, move: TreeMove) => {
        // 실패해도(권한 철회·동시 변경) 최신 목록을 다시 읽는다.
        try {
          await api.setParent(id, move.parentId, move.beforeId);
        } finally {
          workspaceNav.setDocs(workspace.id, await api.wikiDocuments(workspace.id));
        }
      }}
      oncreatechild={async (parentId: string) => {
        const doc = await api.createWikiDocument(
          workspace.id,
          newRequestId(),
          m.workspaces_untitled_document(),
          parentId,
        );
        workspaceNav.refreshDocs();
        router.navigate(documentPath(workspace.id, doc.id));
      }}
    />
  {/if}
  {#if canWrite}
    <TemplatePicker workspaceId={workspace.id} id="new-document-template" bind:value={templateId} />
    <CreateForm
      id="new-document"
      label={m.workspaces_new_doc_title()}
      placeholder={m.workspaces_new_doc_placeholder()}
      submitLabel={m.workspaces_create_doc()}
      create={async (requestId: string, title: string) => {
        const doc = await api.createWikiDocument(
          workspace.id,
          requestId,
          title,
          null,
          templateId === '' ? null : templateId,
        );
        workspaceNav.refreshDocs();
        router.navigate(documentPath(workspace.id, doc.id));
      }}
    />
  {/if}
</section>

<ModulesSection {workspace} />
