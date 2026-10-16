<script lang="ts">
  // 문서를 불러와 편집기에 넘긴다. 다른 문서로 바뀌면 편집기를 새로 만든다.
  import type { DocumentView } from '$lib/api/client';
  import { api, ApiError } from '$lib/api/client';

  import DocumentEditor from './DocumentEditor.svelte';

  interface Props {
    documentId: string;
  }

  const { documentId }: Props = $props();

  let doc = $state<DocumentView | undefined>(undefined);
  let error = $state<string | undefined>(undefined);

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
</script>

{#if error !== undefined}
  <p role="alert" class="text-destructive">{error}</p>
{:else if doc === undefined}
  <p aria-live="polite">불러오는 중…</p>
{:else}
  {#key doc.id}
    <DocumentEditor initial={doc} />
  {/key}
{/if}
