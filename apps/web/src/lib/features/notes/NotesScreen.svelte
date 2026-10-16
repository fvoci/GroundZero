<script lang="ts">
  // 내 개인 노트 (D-22, D-49). 워크스페이스 관리자에게도 보이지 않는다.
  import type { SummaryView } from '$lib/api/client';
  import { api, ApiError } from '$lib/api/client';
  import CreateForm from '$lib/features/workspaces/CreateForm.svelte';
  import { router, standalonePath } from '$lib/router.svelte';

  let notes = $state<SummaryView[] | undefined>(undefined);
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
    <ul class="flex flex-col gap-1" data-testid="note-list">
      {#each notes as note (note.id)}
        <li>
          <a
            class="block rounded px-2 py-1.5 hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
            href={standalonePath(note.id)}
            onclick={(e: MouseEvent) => {
              e.preventDefault();
              router.navigate(standalonePath(note.id));
            }}>{note.title}</a
          >
        </li>
      {/each}
    </ul>
  {/if}
  <CreateForm
    id="new-note"
    label="새 노트 제목"
    placeholder="예: 아이디어"
    submitLabel="노트 만들기"
    create={async (requestId: string, title: string) => {
      const note = await api.createNote(requestId, title);
      router.navigate(standalonePath(note.id));
    }}
  />
</section>
