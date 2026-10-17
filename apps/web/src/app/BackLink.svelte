<script lang="ts">
  // 상위 화면으로 돌아가는 경로 링크. 앱 라우터로 이동한다(새로고침 없음).
  import { m } from '$lib/paraglide/messages.js';
  import { router } from '$lib/router.svelte';

  interface Props {
    path: string;
    label: string;
  }

  const { path, label }: Props = $props();
</script>

<nav aria-label={m.app_breadcrumb()} class="text-sm">
  <a
    class="underline"
    href={path}
    onclick={(e: MouseEvent) => {
      e.preventDefault();
      router.navigate(path);
    }}>{label}</a
  >
</nav>
