<script lang="ts">
  // 워크스페이스 밖 화면(홈)의 앱 틀: 사이드바의 조직 전환기와 홈 빠른 추가를 함께 그린다.
  import AppSidebar from '$lib/features/nav/AppSidebar.svelte';
  import HomeDashboard from '$lib/features/home/HomeDashboard.svelte';
  import * as Sidebar from '$lib/ui/sidebar';
</script>

<Sidebar.Provider>
  <AppSidebar
    workspaces={[]}
    current={undefined}
    displayName="Test User"
    unreadCount={0}
    onsearch={() => undefined}
    ondisplay={() => undefined}
    onexport={() => undefined}
    onlogout={() => undefined}
  />
  <HomeDashboard workspaces={[]} />
</Sidebar.Provider>
