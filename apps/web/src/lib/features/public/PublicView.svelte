<script lang="ts">
  // 비로그인 공개 보기 (D-20). 읽기 전용이며 편집기 인스턴스도 편집 불가로 만든다.
  import type { PublicDocumentView } from '$lib/api/client';
  import { api, ApiError } from '$lib/api/client';
  import TiptapEditor from '$lib/features/documents/TiptapEditor.svelte';

  interface Props {
    token: string;
  }

  const { token }: Props = $props();

  let doc = $state<PublicDocumentView | undefined>(undefined);
  let error = $state<string | undefined>(undefined);

  $effect(() => {
    const t = token;
    let cancelled = false;
    api
      .publicDocument(t)
      .then((d) => {
        if (!cancelled) doc = d;
      })
      .catch((e: unknown) => {
        if (cancelled) return;
        error =
          e instanceof ApiError && e.status === 404
            ? '링크가 없거나 만료·철회되었습니다.'
            : '문서를 불러오지 못했습니다.';
      });
    return () => {
      cancelled = true;
    };
  });
</script>

<main class="mx-auto flex w-full max-w-3xl flex-col gap-4 px-4 py-8">
  {#if error !== undefined}
    <p role="alert">{error}</p>
  {:else if doc === undefined}
    <p aria-live="polite">불러오는 중…</p>
  {:else}
    <p class="text-sm text-muted-foreground">공개 링크로 보는 읽기 전용 문서입니다.</p>
    <h1 class="text-2xl font-semibold">{doc.title}</h1>
    <TiptapEditor content={doc.body} editable={false} label="본문" />
  {/if}
</main>
