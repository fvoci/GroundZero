<script lang="ts">
  // 비로그인 공개 보기 (D-20). 읽기 전용이며 편집기 인스턴스도 편집 불가로 만든다.
  import type { PublicDocumentView } from '$lib/api/client';
  import { api, ApiError } from '$lib/api/client';
  import TiptapEditor from '$lib/features/documents/TiptapEditor.svelte';
  import { m } from '$lib/paraglide/messages.js';

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
          e instanceof ApiError && e.status === 404 ? m.public_link_gone() : m.public_load_failed();
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
    <p aria-live="polite">{m.public_loading()}</p>
  {:else}
    <p class="text-sm text-muted-foreground">{m.public_read_only_hint()}</p>
    <h1 class="text-2xl font-semibold">{doc.title}</h1>
    <TiptapEditor content={doc.body} editable={false} label={m.public_body_label()} />
  {/if}
</main>
