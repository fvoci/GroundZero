<script lang="ts">
  // 상위 화면으로 돌아가는 경로 링크. 앱 라우터로 이동한다(새로고침 없음).
  import { router } from '$lib/router.svelte';

  interface Props {
    path: string;
    label: string;
  }

  const { path, label }: Props = $props();
</script>

<nav aria-label="경로" class="text-sm">
  <a
    class="underline"
    href={path}
    onclick={(e: MouseEvent) => {
      e.preventDefault();
      router.navigate(path);
    }}>{label}</a
  >
</nav>
