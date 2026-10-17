<script lang="ts">
  // 내 개인 노트 (D-22, D-49). 워크스페이스 관리자에게도 보이지 않는다.
  import type { SummaryView } from '$lib/api/client';
  import { api, ApiError, newRequestId } from '$lib/api/client';
  import DocumentTree from '$lib/features/documents/DocumentTree.svelte';
  import TemplatePicker from '$lib/features/documents/TemplatePicker.svelte';
  import type { TreeMove } from '$lib/features/documents/tree';
  import CreateForm from '$lib/features/workspaces/CreateForm.svelte';
  import { router, standalonePath } from '$lib/router.svelte';

  let notes = $state<SummaryView[] | undefined>(undefined);
  let templateId = $state('');
  let error = $state<string | undefined>(undefined);

  api
    .myNotes()
    .then((list) => {
      notes = list;
    })
    .catch((e: unknown) => {
      error = e instanceof ApiError ? e.message : '노트를 불러오지 못했습니다.';
    });
</script>

<section class="flex flex-col gap-6" aria-labelledby="notes-heading">
  <div>
    <h1 id="notes-heading" class="text-xl font-semibold">내 노트</h1>
    <p class="text-sm text-muted-foreground">
      나만 보는 노트입니다. 워크스페이스 관리자에게도 보이지 않습니다. 필요하면 특정 사람에게 공유할
      수 있습니다.
    </p>
  </div>
  {#if error !== undefined}
    <p role="alert" class="text-destructive">{error}</p>
  {:else if notes === undefined}
    <p aria-live="polite">불러오는 중…</p>
  {:else if notes.length === 0}
    <p class="text-muted-foreground">노트가 없습니다.</p>
  {:else}
    <DocumentTree
      docs={notes}
      label="내 노트"
      hrefFor={(id: string) => standalonePath(id)}
      navigate={(path: string) => {
        router.navigate(path);
      }}
      canWrite={true}
      onmove={async (id: string, move: TreeMove) => {
        await api.setParent(id, move.parentId, move.beforeId);
        notes = await api.myNotes();
      }}
      oncreatechild={async (parentId: string) => {
        const note = await api.createNote(newRequestId(), '새 노트', parentId);
        router.navigate(standalonePath(note.id));
      }}
    />
  {/if}
  <TemplatePicker workspaceId={null} id="new-note-template" bind:value={templateId} />
  <CreateForm
    id="new-note"
    label="새 노트 제목"
    placeholder="예: 아이디어"
    submitLabel="노트 만들기"
    create={async (requestId: string, title: string) => {
      const note = await api.createNote(
        requestId,
        title,
        null,
        templateId === '' ? null : templateId,
      );
      router.navigate(standalonePath(note.id));
    }}
  />
</section>
