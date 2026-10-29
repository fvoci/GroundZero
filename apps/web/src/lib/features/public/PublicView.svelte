<script lang="ts">
  // 비로그인 공개 보기 (D-20). 읽기 전용이며 편집기 인스턴스도 편집 불가로 만든다.
  import type { PublicDocumentView } from '$lib/api/client';
  import { api, ApiError } from '$lib/api/client';
  import TiptapEditor from '$lib/features/documents/TiptapEditor.svelte';
  import { m } from '$lib/paraglide/messages.js';
  import { flushSync } from 'svelte';

  interface Props {
    token: string;
  }

  const { token }: Props = $props();

  let doc = $state<PublicDocumentView | undefined>(undefined);
  let error = $state<string | undefined>(undefined);
  let sequence = 0;

  function clear(): void {
    doc = undefined;
    error = undefined;
  }

  async function load(t: string): Promise<void> {
    const mine = ++sequence;
    clear();
    try {
      const fresh = await api.publicDocument(t);
      if (mine === sequence) doc = fresh;
    } catch (e: unknown) {
      if (mine !== sequence) return;
      error =
        e instanceof ApiError && e.status === 404 ? m.public_link_gone() : m.public_load_failed();
    }
  }

  function hideForCache(event: PageTransitionEvent): void {
    if (!event.persisted) return;
    sequence += 1;
    flushSync(clear);
  }

  function restore(event: PageTransitionEvent): void {
    if (!event.persisted) return;
    // BFCache는 DOM·메모리도 되살린다. 응답을 기다리기 전에 기존 제목·본문을 지운다.
    flushSync(clear);
    void load(token);
  }

  $effect(() => {
    const t = token;
    void load(t);
    return () => {
      sequence += 1;
    };
  });
</script>

<svelte:window onpageshow={restore} onpagehide={hideForCache} />

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
