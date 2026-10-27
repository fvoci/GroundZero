<script lang="ts">
  // 항목 화면처럼 주소의 업무를 보드에 넘긴다. 뒤로 가기(popstate)로 주소가 보드로 돌아가면 상세가 닫힌다.
  import type { ItemView } from '$lib/api/client';
  import TaskBoard from '$lib/features/tasks/TaskBoard.svelte';
  import { router } from '$lib/router.svelte';

  const { item }: { item: ItemView } = $props();
  const route = $derived(router.route);
</script>

<TaskBoard
  {item}
  workspaces={[]}
  routeView={route.name === 'item' ? route.view : undefined}
  openTaskId={route.name === 'item' ? route.taskId : undefined}
/>
