<script lang="ts">
  // 워크스페이스의 위키 문서 목록과 새 문서 만들기.
  import type { Workspace } from '@fvoci/api-client';

  import type { SummaryView } from '$lib/api/client';

  import { api, ApiError } from '$lib/api/client';
  import { documentPath, router } from '$lib/router.svelte';

  import ModulesSection from '$lib/features/modules/ModulesSection.svelte';

  import CreateForm from './CreateForm.svelte';

  interface Props {
    workspace: Workspace;
  }

  const { workspace }: Props = $props();

  let docs = $state<SummaryView[] | undefined>(undefined);
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
</script>

<section class="flex flex-col gap-6" aria-labelledby="ws-heading">
  <h1 id="ws-heading" class="text-xl font-semibold">{workspace.name} · 위키</h1>
  {#if error !== undefined}
    <p role="alert" class="text-destructive">{error}</p>
  {:else if docs === undefined}
    <p aria-live="polite">불러오는 중…</p>
  {:else if docs.length === 0}
    <p class="text-muted-foreground">위키 문서가 없습니다.</p>
  {:else}
    <ul class="flex flex-col gap-1" data-testid="wiki-list">
      {#each docs as doc (doc.id)}
        <li>
          <a
            class="flex items-baseline gap-2 rounded px-2 py-1.5 hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
            href={documentPath(workspace.id, doc.id)}
            onclick={(e) => {
              e.preventDefault();
              router.navigate(documentPath(workspace.id, doc.id));
            }}
          >
            <span class="font-mono text-sm text-muted-foreground">{doc.reference}</span>
            <span>{doc.title}</span>
          </a>
        </li>
      {/each}
    </ul>
  {/if}
  {#if canWrite}
    <CreateForm
      id="new-document"
      label="새 문서 제목"
      placeholder="예: 회의록"
      submitLabel="문서 만들기"
      create={async (requestId: string, title: string) => {
        const doc = await api.createWikiDocument(workspace.id, requestId, title);
        router.navigate(documentPath(workspace.id, doc.id));
      }}
    />
  {/if}
</section>

<ModulesSection {workspace} />
