<script lang="ts">
  // 나에게 공유된 문서 (D-23). 제공자가 확인한 이메일 기준이다.
  import type { SharedView } from '$lib/api/client';
  import { api, ApiError } from '$lib/api/client';
  import { router, standalonePath } from '$lib/router.svelte';

  let docs = $state<SharedView[] | undefined>(undefined);
  let error = $state<string | undefined>(undefined);

  api
    .sharedWithMe()
    .then((list) => {
      docs = list;
    })
    .catch((e: unknown) => {
      error = e instanceof ApiError ? e.message : '목록을 불러오지 못했습니다.';
    });
</script>

<section class="flex flex-col gap-6" aria-labelledby="shared-heading">
  <h1 id="shared-heading" class="text-xl font-semibold">공유받은 문서</h1>
  {#if error !== undefined}
    <p role="alert" class="text-destructive">{error}</p>
  {:else if docs === undefined}
    <p aria-live="polite">불러오는 중…</p>
  {:else if docs.length === 0}
    <p class="text-muted-foreground">공유받은 문서가 없습니다.</p>
  {:else}
    <ul class="flex flex-col gap-1" data-testid="shared-list">
      {#each docs as doc (doc.id)}
        <li>
          <a
            class="flex flex-wrap items-baseline gap-2 rounded px-2 py-1.5 hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
            href={standalonePath(doc.id)}
            onclick={(e: MouseEvent) => {
              e.preventDefault();
              router.navigate(standalonePath(doc.id));
            }}
          >
            <span>{doc.title}</span>
            <span class="text-sm text-muted-foreground"
              >{doc.shared_by}님이 공유 · {doc.permission === 'write' ? '고치기' : '읽기'}</span
            >
          </a>
        </li>
      {/each}
    </ul>
  {/if}
</section>
