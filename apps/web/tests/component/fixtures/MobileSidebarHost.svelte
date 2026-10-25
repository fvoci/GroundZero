<script lang="ts">
  // 좁은 화면의 앱 틀: 사이드바 서랍과 여는 단추. 서랍 안 동작이 대화상자를 열 때 서랍을 닫는지 본다.
  import AppSidebar from '$lib/features/nav/AppSidebar.svelte';
  import * as Sidebar from '$lib/ui/sidebar';

  interface Props {
    onsearch: () => void;
    ondisplay: () => void;
    onexport: () => void;
  }

  const { onsearch, ondisplay, onexport }: Props = $props();
</script>

<Sidebar.Provider>
  <AppSidebar
    workspaces={[]}
    current={undefined}
    displayName="Test User"
    unreadCount={0}
    {onsearch}
    {ondisplay}
    {onexport}
    onlogout={() => undefined}
  />
  <Sidebar.Trigger />
</Sidebar.Provider>
