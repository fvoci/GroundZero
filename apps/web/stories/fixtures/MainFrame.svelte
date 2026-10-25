<script lang="ts">
  // 앱 본문(src/app/App.svelte의 main)과 같은 폭과 컨테이너 쿼리 틀. 화면 안 여러 열 배치(@2xl:grid-cols-3 등)는 창 폭이
  // 아니라 이 본문 폭을 보므로, 화면 조각(칸반 등)만 그리는 story는 이 틀 안에 넣어야 앱과 같은 열로 나뉜다.
  import type { Snippet } from 'svelte';

  const { children }: { children: Snippet } = $props();
</script>

<main class="@container mx-auto flex w-full max-w-5xl min-w-0 flex-col gap-8">
  {@render children()}
</main>
