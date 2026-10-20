<script lang="ts">
  // 내 개인 노트 (D-22, D-49). 워크스페이스 관리자에게도 보이지 않는다.
  import type { SummaryView } from '$lib/api/client';
  import { api, newRequestId } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
  import DocumentTree from '$lib/features/documents/DocumentTree.svelte';
  import TemplatePicker from '$lib/features/documents/TemplatePicker.svelte';
  import type { TreeMove } from '$lib/features/documents/tree';
  import { moveThenReload } from '$lib/features/documents/tree';
  import CreateForm from '$lib/features/workspaces/CreateForm.svelte';
  import { m } from '$lib/paraglide/messages.js';
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
      error = errorText(e, m.notes_load_failed());
    });
</script>

<section class="flex flex-col gap-6" aria-labelledby="notes-heading">
  <div>
    <h1 id="notes-heading" class="text-xl font-semibold">{m.notes_heading()}</h1>
    <p class="text-sm text-muted-foreground">
      {m.notes_hint()}
    </p>
  </div>
  {#if error !== undefined}
    <p role="alert" class="text-destructive">{error}</p>
  {:else if notes === undefined}
    <p aria-live="polite">{m.notes_loading()}</p>
  {:else}
    <DocumentTree
      docs={notes}
      label={m.notes_heading()}
      empty={m.notes_empty()}
      hrefFor={(id: string) => standalonePath(id)}
      navigate={(path: string) => {
        router.navigate(path);
      }}
      canWrite={true}
      onmove={(id: string, move: TreeMove) =>
        // 실패해도(권한 철회·동시 변경) 최신 목록을 다시 읽는다.
        moveThenReload(
          () => api.setParent(id, move.parentId, move.beforeId),
          async () => {
            notes = await api.myNotes();
          },
        )}
      oncreatechild={async (parentId: string) => {
        const note = await api.createNote(newRequestId(), m.notes_new_note_title(), parentId);
        router.navigate(standalonePath(note.id));
      }}
    />
  {/if}
  <TemplatePicker workspaceId={null} id="new-note-template" bind:value={templateId} />
  <CreateForm
    id="new-note"
    label={m.notes_new_title_label()}
    placeholder={m.notes_new_title_placeholder()}
    submitLabel={m.notes_create()}
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
