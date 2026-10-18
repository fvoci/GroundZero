<script lang="ts">
  // 상위 화면으로 돌아가는 경로 링크. 앱 라우터로 이동한다(새로고침 없음). 긴 이름은 한 줄로 줄인다.
  import ChevronLeft from '@lucide/svelte/icons/chevron-left';

  import { m } from '$lib/paraglide/messages.js';
  import { router } from '$lib/router.svelte';

  interface Props {
    path: string;
    label: string;
  }

  const { path, label }: Props = $props();
</script>

<nav aria-label={m.app_breadcrumb()} class="-mb-4 min-w-0 text-sm">
  <a
    class="inline-flex max-w-full items-center gap-1 rounded-sm text-muted-foreground underline-offset-4 hover:text-foreground hover:underline focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
    href={path}
    title={label}
    onclick={(e: MouseEvent) => {
      if (e.metaKey || e.ctrlKey || e.shiftKey || e.button !== 0) return;
      e.preventDefault();
      router.navigate(path);
    }}
    ><ChevronLeft class="size-4 shrink-0" aria-hidden="true" /><span class="truncate">{label}</span
    ></a
  >
</nav>
