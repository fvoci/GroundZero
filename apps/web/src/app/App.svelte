<script lang="ts">
  // 앱 셸: 로그인 확인 → 사이드바(워크스페이스) + 경로별 화면.
  import type { Me, Workspace } from '@fvoci/api-client';

  import { api } from '$lib/api/client';
  import LoginScreen from '$lib/features/auth/LoginScreen.svelte';
  import DocumentScreen from '$lib/features/documents/DocumentScreen.svelte';
  import NotesScreen from '$lib/features/notes/NotesScreen.svelte';
  import SharedScreen from '$lib/features/notes/SharedScreen.svelte';
  import PublicView from '$lib/features/public/PublicView.svelte';
  import CalendarScreen from '$lib/features/events/CalendarScreen.svelte';
  import TrashScreen from '$lib/features/trash/TrashScreen.svelte';
  import AuditScreen from '$lib/features/audit/AuditScreen.svelte';
  import DeletedWorkspaces from '$lib/features/trash/DeletedWorkspaces.svelte';
  import MembersScreen from '$lib/features/members/MembersScreen.svelte';
  import ItemScreen from '$lib/features/modules/ItemScreen.svelte';
  import ModuleScreen from '$lib/features/modules/ModuleScreen.svelte';
  import SearchDialog from '$lib/features/search/SearchDialog.svelte';
  import { displaySettings } from '$lib/display';
  import { m } from '$lib/paraglide/messages.js';
  import { setLocale } from '$lib/paraglide/runtime.js';
  import DisplaySettingsDialog from '$lib/features/settings/DisplaySettingsDialog.svelte';
  import FeedbackHost from '$lib/features/feedback/FeedbackHost.svelte';
  import NotificationsScreen from '$lib/features/references/NotificationsScreen.svelte';
  import TimeScreen from '$lib/features/time/TimeScreen.svelte';
  import TimerWidget from '$lib/features/time/TimerWidget.svelte';
  import HomeScreen from '$lib/features/workspaces/HomeScreen.svelte';
  import WorkspaceScreen from '$lib/features/workspaces/WorkspaceScreen.svelte';
  import { me as currentUser } from '$lib/me.svelte';
  import AppSidebar from '$lib/features/nav/AppSidebar.svelte';
  import { itemPath, router, workspacePath } from '$lib/router.svelte';
  import * as Sidebar from '$lib/ui/sidebar';

  import BackLink from './BackLink.svelte';

  type Session =
    | { state: 'loading' }
    | { state: 'anonymous'; oidcEnabled: boolean }
    | { state: 'signed-in'; me: Me; workspaces: Workspace[] }
    | { state: 'error'; message: string };

  let session = $state<Session>({ state: 'loading' });
  let searchOpen = $state(false);
  let displayOpen = $state(false);

  async function load(): Promise<void> {
    try {
      const me = await api.me();
      if (me === undefined) {
        const info = await api.authInfo();
        session = { state: 'anonymous', oidcEnabled: info.oidc_enabled };
        return;
      }
      currentUser.userId = me.id;
      currentUser.timeZone = me.time_zone;
      currentUser.locale = me.locale;
      currentUser.weekStart = me.week_start;
      currentUser.hourCycle = me.hour_cycle;
      currentUser.displayName = me.display_name;
      // 저장한 표시 언어로 그린다(D-81). 화면 낭독기·브라우저가 쓰는 문서 언어도 맞춘다.
      await setLocale(displaySettings().locale, { reload: false });
      document.documentElement.lang = displaySettings().locale;
      session = { state: 'signed-in', me, workspaces: await api.workspaces() };
    } catch (e: unknown) {
      session = {
        state: 'error',
        message: e instanceof Error ? e.message : m.app_server_unreachable(),
      };
    }
  }

  void load();

  // 읽지 않은 알림 수(알림은 접근 권한을 따라 서버가 거른다). 화면을 옮길 때 다시 세되 30초 안에는
  // 다시 묻지 않는다. 알림 화면에서 읽음 처리하면 곧바로, 창으로 돌아오면 다시 센다.
  const UNREAD_FRESH_MS = 30_000;
  let unreadCount = $state(0);
  let unreadAt = 0;
  let unreadSeq = 0;
  async function refreshUnread(force = false): Promise<void> {
    if (session.state !== 'signed-in') return;
    if (!force && Date.now() - unreadAt < UNREAD_FRESH_MS) return;
    unreadAt = Date.now();
    // 늦게 도착한 이전 응답(읽음 처리 전 수)이 새 수를 덮지 않게 한다.
    const mine = ++unreadSeq;
    try {
      const count = (await api.notifications()).filter((n) => !n.read).length;
      if (mine === unreadSeq) unreadCount = count;
    } catch {
      if (mine === unreadSeq) unreadCount = 0;
    }
  }
  $effect(() => {
    if (session.state === 'signed-in' && router.path !== '') void refreshUnread();
  });
  $effect(() => {
    const onFocus = (): void => {
      void refreshUnread();
    };
    window.addEventListener('focus', onFocus);
    return () => {
      window.removeEventListener('focus', onFocus);
    };
  });

  // 데스크톱 사이드바를 접었는지(사이드바가 쿠키에 남긴 값). 처음 열 때 그대로 되살린다.
  const sidebarOpen = !document.cookie.split('; ').includes('sidebar_state=false');

  /** 멤버십이 바뀐 뒤(초대 수락·탈퇴·인계) 워크스페이스 목록을 다시 읽는다. */

  async function reloadWorkspaces(): Promise<void> {
    const workspaces = await api.workspaces();
    if (session.state === 'signed-in') session.workspaces = workspaces;
  }

  async function logout(): Promise<void> {
    const redirect = await api.logout();
    window.location.assign(redirect);
  }

  const route = $derived(router.route);
  const WORKSPACE_ROUTES = new Set([
    'workspace',
    'document',
    'members',
    'calendar',
    'trash',
    'audit',
    'module',
    'item',
    'item-document',
  ]);
  // 글을 읽고 쓰는 화면은 줄 길이를 좁게, 표·칸반·달력이 있는 화면은 넓게 둔다.
  const READING_ROUTES = new Set([
    'document',
    'item-document',
    'standalone-document',
    'notes',
    'shared',
    'notifications',
  ]);
  // 머리글 제목: 워크스페이스 안이면 그 이름, 개인 화면이면 그 화면 이름.
  const headerTitle = $derived.by(() => {
    if (currentWorkspace !== undefined) return currentWorkspace.name;
    switch (route.name) {
      case 'notes':
        return m.app_nav_notes();
      case 'shared':
        return m.app_nav_shared();
      case 'notifications':
        return m.app_nav_notifications();
      case 'time':
        return m.app_nav_time();
      case 'my-trash':
        return m.app_nav_my_trash();
      case 'my-audit':
        return m.app_nav_my_audit();
      default:
        return m.app_title_home();
    }
  });
  const currentWorkspace = $derived.by(() => {
    if (session.state !== 'signed-in') return undefined;
    if (!WORKSPACE_ROUTES.has(route.name)) return undefined;
    if (!('workspaceId' in route)) return undefined;
    const id = route.workspaceId;
    return session.workspaces.find((w) => w.id === id);
  });
</script>

{#if session.state !== 'loading'}
  <!-- 표시 언어(D-81)를 정한 뒤에 그린다(토스트 영역 라벨도 그 언어로). -->
  <FeedbackHost />
{/if}

{#if route.name === 'public'}
  <PublicView token={route.token} />
{:else if session.state === 'loading'}
  <p class="p-6" aria-live="polite">{m.app_loading()}</p>
{:else if session.state === 'error'}
  <p class="p-6 text-destructive" role="alert">{session.message}</p>
{:else if session.state === 'anonymous'}
  <LoginScreen oidcEnabled={session.oidcEnabled} returnTo={router.path} />
{:else}
  {@const signedIn = session}
  <SearchDialog
    bind:open={searchOpen}
    memberOf={(ws: string) => signedIn.workspaces.some((w) => w.id === ws)}
  />
  <DisplaySettingsDialog bind:open={displayOpen} />
  <Sidebar.Provider open={sidebarOpen}>
    <AppSidebar
      workspaces={signedIn.workspaces}
      current={currentWorkspace}
      displayName={signedIn.me.display_name}
      {unreadCount}
      onsearch={() => {
        searchOpen = true;
      }}
      ondisplay={() => {
        displayOpen = true;
      }}
      onexport={() => {
        window.location.assign('/api/me/export');
      }}
      onlogout={() => void logout()}
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

      <main
        class={`mx-auto flex w-full min-w-0 flex-col gap-8 px-4 py-6 sm:px-6 ${READING_ROUTES.has(route.name) ? 'max-w-3xl' : 'max-w-5xl'}`}
      >
        {#if route.name === 'home'}
          <HomeScreen
            workspaces={signedIn.workspaces}
            oncreated={(ws: Workspace) => {
              if (
                session.state === 'signed-in' &&
                !session.workspaces.some((w) => w.id === ws.id)
              ) {
                session.workspaces = [...session.workspaces, ws];
              }
            }}
            onjoined={reloadWorkspaces}
          />
        {:else if WORKSPACE_ROUTES.has(route.name)}
          {#if currentWorkspace === undefined}
            <p role="alert">{m.app_workspace_missing()}</p>
          {:else if route.name === 'members'}
            <MembersScreen
              workspace={currentWorkspace}
              meId={signedIn.me.id}
              onchanged={() => void reloadWorkspaces()}
            />
          {:else if route.name === 'workspace'}
            <WorkspaceScreen workspace={currentWorkspace} />
          {:else if route.name === 'trash'}
            <BackLink path={workspacePath(currentWorkspace.id)} label={currentWorkspace.name} />
            <TrashScreen workspace={currentWorkspace} />
          {:else if route.name === 'audit'}
            <BackLink path={workspacePath(currentWorkspace.id)} label={currentWorkspace.name} />
            <AuditScreen workspace={currentWorkspace} />
          {:else if route.name === 'calendar'}
            <BackLink path={workspacePath(currentWorkspace.id)} label={currentWorkspace.name} />
            <CalendarScreen workspace={currentWorkspace} />
          {:else if route.name === 'module'}
            <BackLink path={workspacePath(currentWorkspace.id)} label={currentWorkspace.name} />
            <ModuleScreen workspace={currentWorkspace} moduleId={route.moduleId} />
          {:else if route.name === 'item'}
            <ItemScreen
              workspace={currentWorkspace}
              itemId={route.itemId}
              taskId={route.taskId}
              workspaces={signedIn.workspaces}
            />
          {:else if route.name === 'item-document'}
            <BackLink
              path={itemPath(currentWorkspace.id, route.itemId)}
              label={m.app_back_to_item()}
            />
            <DocumentScreen documentId={route.documentId} workspaces={signedIn.workspaces} />
          {:else if route.name === 'document'}
            <BackLink
              path={workspacePath(currentWorkspace.id)}
              label={m.app_wiki_of({ name: currentWorkspace.name })}
            />
            <DocumentScreen documentId={route.documentId} workspaces={signedIn.workspaces} />
          {/if}
        {:else if route.name === 'notes'}
          <NotesScreen />
        {:else if route.name === 'notifications'}
          <NotificationsScreen
            workspaces={signedIn.workspaces}
            onchanged={() => void refreshUnread(true)}
          />
        {:else if route.name === 'my-audit'}
          <AuditScreen workspace={undefined} />
        {:else if route.name === 'my-trash'}
          <div class="flex flex-col gap-8">
            <TrashScreen workspace={undefined} />
            <DeletedWorkspaces onchanged={() => void reloadWorkspaces()} />
          </div>
        {:else if route.name === 'time'}
          <TimeScreen workspaces={signedIn.workspaces} />
        {:else if route.name === 'shared'}
          <SharedScreen />
        {:else if route.name === 'standalone-document'}
          <DocumentScreen documentId={route.documentId} workspaces={signedIn.workspaces} />
        {:else}
          <p role="alert">{m.app_not_found()}</p>
        {/if}
      </main>
    </Sidebar.Inset>
  </Sidebar.Provider>
{/if}
