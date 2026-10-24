<script lang="ts">
  // 앱 셸(src/app/App.svelte)의 로그인 뒤 화면을 같은 방식으로 조립한 합성 예: 실제 사이드바(AppSidebar)와 머리글,
  // 경로별 화면(홈·모듈). App.svelte 자체는 로그인 확인·경로·알림 수 세기를 하므로 그리지 않는다. 경로는 story가
  // `router.path`로 정하고, 데이터는 story가 `api` 함수를 바꿔 끼워 준다(stories/fixtures/screens.ts).
  import type { Workspace } from '@fvoci/api-client';

  import BackLink from '../../src/app/BackLink.svelte';
  import ModuleScreen from '$lib/features/modules/ModuleScreen.svelte';
  import AppSidebar from '$lib/features/nav/AppSidebar.svelte';
  import TimerWidget from '$lib/features/time/TimerWidget.svelte';
  import HomeScreen from '$lib/features/workspaces/HomeScreen.svelte';
  import { m } from '$lib/paraglide/messages.js';
  import { router, workspacePath } from '$lib/router.svelte';
  import * as Sidebar from '$lib/ui/sidebar';

  interface Props {
    workspaces: Workspace[];
    displayName: string;
    unreadCount?: number;
    /** 데스크톱 사이드바를 펼쳐 둘지(접으면 아이콘 막대). */
    sidebarOpen?: boolean;
    onsearch: () => void;
    ondisplay: () => void;
    onexport: () => void;
    onlogout: () => void;
  }

  const {
    workspaces,
    displayName,
    unreadCount = 0,
    sidebarOpen = true,
    onsearch,
    ondisplay,
    onexport,
    onlogout,
  }: Props = $props();

  const route = $derived(router.route);
  const current = $derived(
    'workspaceId' in route ? workspaces.find((w) => w.id === route.workspaceId) : undefined,
  );
  const headerTitle = $derived(current?.name ?? m.app_title_home());
</script>

<Sidebar.Provider open={sidebarOpen}>
  <AppSidebar
    {workspaces}
    {current}
    {displayName}
    {unreadCount}
    {onsearch}
    {ondisplay}
    {onexport}
    {onlogout}
  />

  <Sidebar.Inset class="min-w-0">
    <header
      class="sticky top-0 z-10 flex h-12 items-center gap-2 border-b bg-background/95 px-3 backdrop-blur supports-[backdrop-filter]:bg-background/80"
    >
      <Sidebar.Trigger />
      <span class="min-w-0 flex-1 truncate text-base font-semibold" title={headerTitle}
        >{headerTitle}</span
      >
      <div class="flex shrink-0 items-center gap-2">
        <TimerWidget />
      </div>
    </header>

    <main class="mx-auto flex w-full max-w-5xl min-w-0 flex-col gap-8 px-4 py-6 sm:px-6">
      {#if route.name === 'module' && current !== undefined}
        <BackLink path={workspacePath(current.id)} label={current.name} />
        <ModuleScreen workspace={current} moduleId={route.moduleId} />
      {:else}
        <HomeScreen {workspaces} oncreated={() => undefined} onjoined={() => Promise.resolve()} />
      {/if}
    </main>
  </Sidebar.Inset>
</Sidebar.Provider>
