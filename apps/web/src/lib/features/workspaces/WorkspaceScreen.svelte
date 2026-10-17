<script lang="ts">
  // 워크스페이스의 위키 문서 목록과 새 문서 만들기.
  import type { Workspace } from '@fvoci/api-client';

  import type { SummaryView } from '$lib/api/client';

  import { api, ApiError, newRequestId } from '$lib/api/client';
  import DocumentTree from '$lib/features/documents/DocumentTree.svelte';
  import type { TreeMove } from '$lib/features/documents/tree';
  import { documentPath, router } from '$lib/router.svelte';

  import ModulesSection from '$lib/features/modules/ModulesSection.svelte';

  import TemplatePicker from '$lib/features/documents/TemplatePicker.svelte';

  import CreateForm from './CreateForm.svelte';

  interface Props {
    workspace: Workspace;
  }

  const { workspace }: Props = $props();

  let docs = $state<SummaryView[] | undefined>(undefined);
  let templateId = $state('');
  let error = $state<string | undefined>(undefined);

  $effect(() => {
    const wsId = workspace.id;
    let cancelled = false;
    docs = undefined;
    error = undefined;
    api
      .wikiDocuments(wsId)
      .then((list) => {
        if (!cancelled) docs = list;
      })
      .catch((e: unknown) => {
        if (!cancelled) error = e instanceof ApiError ? e.message : '목록을 불러오지 못했습니다.';
      });
    return () => {
      cancelled = true;
    };
  });

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
      realtimeMessage = on
        ? '실시간 공동 편집을 켰습니다. 문서를 다시 열면 함께 고칠 수 있습니다.'
        : '실시간 공동 편집을 껐습니다.';
    } catch (e: unknown) {
      realtimeMessage = e instanceof ApiError ? e.message : '바꾸지 못했습니다.';
    }
  }
</script>

<section class="flex flex-col gap-6" aria-labelledby="ws-heading">
  <h1 id="ws-heading" class="text-xl font-semibold">{workspace.name} · 위키</h1>
  <p class="text-sm text-muted-foreground">
    이 워크스페이스의 관리자는 비공개 문서를 포함해 이 공간의 모든 문서를 볼 수 있습니다. 각자의
    개인 노트와 개인 시간 기록은 관리자에게도 보이지 않습니다.
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
        실시간 공동 편집(문서를 여럿이 동시에 고침)
      </label>
      <p class="text-muted-foreground">
        켜면 이 워크스페이스 문서의 본문을 바로 함께 고치고, 조용해지거나 모두 나가면 리비전으로
        남깁니다. 개인 노트는 해당하지 않습니다.
      </p>
      <p aria-live="polite">{realtimeMessage}</p>
    </div>
  {/if}
  {#if error !== undefined}
    <p role="alert" class="text-destructive">{error}</p>
  {:else if docs === undefined}
    <p aria-live="polite">불러오는 중…</p>
  {:else if docs.length === 0}
    <p class="text-muted-foreground">위키 문서가 없습니다.</p>
  {:else}
    <DocumentTree
      {docs}
      label={`${workspace.name} 위키 문서`}
      hrefFor={(id: string) => documentPath(workspace.id, id)}
      navigate={(path: string) => {
        router.navigate(path);
      }}
      {canWrite}
      onmove={async (id: string, move: TreeMove) => {
        await api.setParent(id, move.parentId, move.beforeId);
        docs = await api.wikiDocuments(workspace.id);
      }}
      oncreatechild={async (parentId: string) => {
        const doc = await api.createWikiDocument(workspace.id, newRequestId(), '새 문서', parentId);
        router.navigate(documentPath(workspace.id, doc.id));
      }}
    />
  {/if}
  {#if canWrite}
    <TemplatePicker workspaceId={workspace.id} id="new-document-template" bind:value={templateId} />
    <CreateForm
      id="new-document"
      label="새 문서 제목"
      placeholder="예: 회의록"
      submitLabel="문서 만들기"
      create={async (requestId: string, title: string) => {
        const doc = await api.createWikiDocument(
          workspace.id,
          requestId,
          title,
          null,
          templateId === '' ? null : templateId,
        );
        router.navigate(documentPath(workspace.id, doc.id));
      }}
    />
  {/if}
</section>

<ModulesSection {workspace} />
