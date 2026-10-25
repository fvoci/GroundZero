<script lang="ts">
  // 통합 검색 창을 앱처럼 연다: 앱은 사이드바의 검색 단추(또는 Ctrl/⌘+K)로 `open`을 켠다(src/app/App.svelte). 닫으면
  // 초점이 그 단추로 돌아오는지 보려고 같은 자리에 단추 하나를 둔다.
  import SearchDialog from '$lib/features/search/SearchDialog.svelte';
  import { Button } from '$lib/ui/button';

  const { workspace }: { workspace?: { id: string; name: string } | undefined } = $props();

  let open = $state(false);
</script>

<Button variant="outline" onclick={() => (open = true)}>검색 열기</Button>
<SearchDialog bind:open memberOf={() => true} {workspace} />
