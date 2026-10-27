<script lang="ts">
  // 워크스페이스의 위키 문서 목록과 새 문서 만들기.
  import type { Workspace } from '@fvoci/api-client';

  import { api, newRequestId } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import type { CreateFormContext } from '$lib/features/common/form-draft';
  import DocumentTree from '$lib/features/documents/DocumentTree.svelte';
  import { notifyDone } from '$lib/features/feedback/feedback.svelte';
  import type { TreeMove } from '$lib/features/documents/tree';
  import { moveThenReload } from '$lib/features/documents/tree';
  import { m } from '$lib/paraglide/messages.js';
  import { documentPath, router } from '$lib/router.svelte';

  import ModulesSection from '$lib/features/modules/ModulesSection.svelte';
  import { workspaceNav } from '$lib/features/nav/workspace-nav.svelte';

  import TemplatePicker from '$lib/features/documents/TemplatePicker.svelte';

  import CreateForm from './CreateForm.svelte';

  interface Props {
    workspace: Workspace;
    /** 실시간 공동 편집을 켜거나 껐다. 앱이 워크스페이스 목록을 고쳐 다시 들어와도 그 값을 보이게 한다. */
    onrealtimechanged?: (workspaceId: string, enabled: boolean) => void;
  }

  const { workspace, onrealtimechanged }: Props = $props();

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
  // 새 문서를 보내는 동안 템플릿 선택을 잠근다. 그 값도 초안(draftContext)이라 바꾸면 성공한 만들기를 버리고, 다시 내면
  // 새 요청 id로 두 번째 문서가 생긴다.
  let creating = $state(false);
  // 워크스페이스를 옮겨도 이 화면은 다시 만들어지지 않는다. 앞 워크스페이스에서 고른 템플릿으로 새 문서를 만들지 않게
  // 워크스페이스가 바뀌면 비운다(같은 워크스페이스의 정보만 새로 오면 그대로 둔다).
  let templateWorkspace: string | undefined;
  $effect(() => {
    if (workspace.id === templateWorkspace) return;
    templateWorkspace = workspace.id;
    templateId = '';
  });

  // 쓰기·관리 여부는 역할이 아니라 서버의 접근 등급(app_access)에서 온다(D-113).
  const canWrite = $derived(workspace.access !== 'read');

  // 실시간 공동 편집(D-27, D-70). 관리자만 켜고 끈다. 끄면 열린 편집은 리비전을 남기고 닫힌다.
  let realtime = $state(false);
  // 바꾸기 실패 안내(성공은 토스트).
  let realtimeMessage = $state('');
  // 안내가 어느 워크스페이스의 것인지. 같은 워크스페이스의 새 값(바꾼 뒤 앱이 목록을 고침)으로는 안내를 지우지 않는다.
  let messageFor = '';
  $effect(() => {
    realtime = workspace.realtime_enabled;
    if (workspace.id !== messageFor) realtimeMessage = '';
  });
  // 바꾸는 요청이 진행 중인지. 겹쳐 보내면 늦게 온 앞 요청의 응답이 마지막 선택과 다른 값을 앱의 목록과 안내에 남긴다.
  let realtimeBusy = $state(false);
  async function setRealtime(on: boolean): Promise<void> {
    if (realtimeBusy) return;
    realtimeBusy = true;
    const id = workspace.id;
    const before = realtime;
    // 칸은 누른 대로 바뀌어 있다. 실패하면 이전 값으로 되돌려야 칸도 다시 그려진다.
    realtime = on;
    try {
      await api.setWorkspaceRealtime(id, on);
      onrealtimechanged?.(id, on);
      if (workspace.id !== id) return;
      // 일반 완료는 토스트다(D-82). 화면 안 영역에는 앞선 실패만 남아 있었으므로 비운다.
      realtimeMessage = '';
      notifyDone(
        on ? m.workspaces_realtime_on() : m.workspaces_realtime_off(),
        'workspace-realtime',
      );
    } catch (e: unknown) {
      if (workspace.id !== id) return;
      realtime = before;
      messageFor = id;
      realtimeMessage = errorText(e, m.workspaces_realtime_failed());
    } finally {
      realtimeBusy = false;
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
  {#if workspace.access === 'owner'}
    <div class="flex flex-col gap-1 text-sm">
      <label class="flex items-center gap-2">
        <input
          type="checkbox"
          disabled={realtimeBusy}
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
  {:else}
    <DocumentTree
      {docs}
      label={m.workspaces_wiki_tree({ workspace: workspace.name })}
      empty={m.workspaces_wiki_empty()}
      hrefFor={(id: string) => documentPath(workspace.id, id)}
      navigate={(path: string) => {
        router.navigate(path);
      }}
      {canWrite}
      onmove={(id: string, move: TreeMove) =>
        // 실패해도(권한 철회·동시 변경) 최신 목록을 다시 읽는다.
        moveThenReload(
          () => api.setParent(id, move.parentId, move.beforeId),
          async () => {
            workspaceNav.setDocs(workspace.id, await api.wikiDocuments(workspace.id));
          },
        )}
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
    <TemplatePicker
      workspaceId={workspace.id}
      id="new-document-template"
      bind:value={templateId}
      disabled={creating}
    />
    <CreateForm
      id="new-document"
      draftName={`wiki-new:${workspace.id}`}
      bind:draftContext={templateId}
      bind:busy={creating}
      label={m.workspaces_new_doc_title()}
      placeholder={m.workspaces_new_doc_placeholder()}
      submitLabel={m.workspaces_create_doc()}
      create={async (requestId: string, title: string, context: CreateFormContext) => {
        const workspaceId = workspace.id;
        const doc = await api.createWikiDocument(
          workspaceId,
          requestId,
          title,
          null,
          templateId === '' ? null : templateId,
        );
        if (!context.saved()) return;
        workspaceNav.refreshDocs();
        router.navigate(documentPath(workspaceId, doc.id));
      }}
    />
  {/if}
</section>

<ModulesSection {workspace} />
