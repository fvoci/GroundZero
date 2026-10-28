<script lang="ts">
  import { activityApi } from '$lib/features/activity/api';
  // 앱 셸: 로그인 확인 → 사이드바(워크스페이스) + 경로별 화면.
  import type { Me, Workspace } from '@fvoci/api-client';

  import { api, ApiError, NetworkError } from '$lib/api/client';
  import { errorText } from '$lib/api/errors';
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
  // 계정 전환 뒤 다시 읽기 전까지 기존 화면·입력을 숨기되 메모리는 보존한다(D-96).
  import { accountSafety } from '$lib/features/feedback/feedback.svelte';
  import { connection } from '$lib/features/feedback/feedback.svelte';
  import NotificationsScreen from '$lib/features/references/NotificationsScreen.svelte';
  // 홈에서 초대를 처리한 경우도 서버 미독 수를 즉시 다시 읽는다(D-118).
  import { invitationChanges } from '$lib/features/home/invitations.svelte';
  import TimeScreen from '$lib/features/time/TimeScreen.svelte';
  import TimerWidget from '$lib/features/time/TimerWidget.svelte';
  import HomeScreen from '$lib/features/workspaces/HomeScreen.svelte';
  import WorkspaceScreen from '$lib/features/workspaces/WorkspaceScreen.svelte';
  import { me as currentUser } from '$lib/me.svelte';
  import MyTasksScreen from '$lib/features/home/MyTasksScreen.svelte';
  import AppSidebar from '$lib/features/nav/AppSidebar.svelte';
  import { createLatest } from '$lib/latest';
  import { org } from '$lib/org.svelte';
  import { itemPath, router, workspacePath } from '$lib/router.svelte';
  import { asTheme, setTheme } from '$lib/theme.svelte';
  import { Button } from '$lib/ui/button';
  import * as Sidebar from '$lib/ui/sidebar';

  import BackLink from './BackLink.svelte';

  type Session =
    | { state: 'loading' }
    | { state: 'anonymous'; oidcEnabled: boolean }
    | { state: 'signed-in'; me: Me; workspaces: Workspace[] }
    /** `network`: 서버에 닿지 않았다(연결 배너가 내려가면 다시 읽는다). */
    | { state: 'error'; message: string; network: boolean };

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
      currentUser.maxTitleChars = me.limits.max_title_chars;
      const theme = asTheme(me.theme);
      currentUser.theme = theme;
      // 화면 밝기의 정본은 계정이다. 다른 기기에서 바꿨으면 여기서 맞추고 다음 첫 그림을 위한 사본도 고친다(D-109).
      setTheme(theme);
      currentUser.displayName = me.display_name;
      // 저장한 표시 언어로 그린다(D-81). 화면 낭독기·브라우저가 쓰는 문서 언어도 맞춘다.
      await setLocale(displaySettings().locale, { reload: false });
      document.documentElement.lang = displaySettings().locale;
      // 조직 목록(D-111)은 전환기와 개인 화면의 범위다. 서버는 기본 조직을 정하지 않으므로 선택은 org 문맥이 든다.
      const [workspaces, organizations] = await Promise.all([
        api.workspaces(),
        api.myOrganizations(),
      ]);
      org.load(me.id, organizations, me.last_organization_id ?? null);
      session = { state: 'signed-in', me, workspaces };
    } catch (e: unknown) {
      // 브라우저·서버의 원문 대신 번역된 문구(D-81). 서버에 닿지 않은 경우는 다시 시도하면 된다고 알린다.
      const network =
        e instanceof NetworkError || (e instanceof ApiError && e.code === 'unreachable');
      session = {
        state: 'error',
        message: network ? m.app_server_unreachable() : errorText(e, m.app_start_failed()),
        network,
      };
    }
  }

  void load();

  async function retry(): Promise<void> {
    session = { state: 'loading' };
    await load();
  }

  // 첫 화면을 서버에 닿지 못해 그리지 못했으면, 연결 배너가 내려갈 때(서버가 다시 답함) 다시 읽는다.
  $effect(() => {
    if (session.state === 'error' && session.network && !connection.lost) void retry();
  });

  // 읽지 않은 알림 수는 서버가 현재 조직 범위로 센다(D-118; 초대는 어느 조직에서나 포함). 화면을 옮길 때 다시 세되
  // 30초 안에는 다시 묻지 않는다. 알림 화면에서 읽음 처리하면 곧바로, 창으로 돌아오거나 조직을 바꾸면 다시 센다.
  const UNREAD_FRESH_MS = 30_000;
  // 초대 수락·거절은 경로 이동의 30초 캐시를 기다리지 않는다.
  let seenInvitationVersion = invitationChanges.version;
  $effect(() => {
    const version = invitationChanges.version;
    if (version === seenInvitationVersion) return;
    seenInvitationVersion = version;
    void refreshUnread(true);
  });
  let unreadCount = $state(0);
  let unreadAt = 0;
  let unreadSeq = 0;
  /** 마지막으로 센 조직. 조직이 바뀌었을 때만 바로 다시 센다(처음 그릴 때 두 번 묻지 않게). */
  let unreadOrganization: string | undefined;
  async function refreshUnread(force = false): Promise<void> {
    if (session.state !== 'signed-in') return;
    const organizationId = org.currentId;
    if (organizationId === undefined) {
      // 선택이 풀리면 아직 안 온 이전 조직의 수가 뒤늦게 0을 덮지 않게 그 요청을 버리고, 다음 선택 때 바로 다시 센다.
      unreadSeq += 1;
      unreadAt = 0;
      unreadOrganization = undefined;
      unreadCount = 0;
      return;
    }
    if (!force && Date.now() - unreadAt < UNREAD_FRESH_MS) return;
    unreadAt = Date.now();
    unreadOrganization = organizationId;
    // 늦게 도착한 이전 응답(읽음 처리 전 수)이 새 수를 덮지 않게 한다.
    const mine = ++unreadSeq;
    try {
      const count = await activityApi.unreadCount(organizationId);
      if (mine === unreadSeq) unreadCount = count;
    } catch {
      // 읽지 못했으면 0("알림 없음")으로 보이지 않게 앞의 수를 둔다. 다음 이동·창 초점 때 바로 다시 묻는다.
      if (mine === unreadSeq) unreadAt = 0;
    }
  }
  $effect(() => {
    if (session.state === 'signed-in' && router.path !== '') void refreshUnread();
  });
  $effect(() => {
    // 조직을 바꾸면 그 조직의 수로 바로 다시 센다.
    const organizationId = org.currentId;
    if (
      session.state === 'signed-in' &&
      organizationId !== undefined &&
      organizationId !== unreadOrganization
    )
      void refreshUnread(true);
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

  // 마지막으로 보낸 목록 요청의 차례와, 그 응답을 아직 기다리는지. 늦게 도착한 이전 응답이 그 뒤에 읽거나 고친 값(실시간
  // 공동 편집 설정 등)을 덮지 않게 마지막 요청의 응답만 넣는다.
  let workspacesPending = false;
  const workspacesReload = createLatest();

  /**
   * 멤버십이 바뀐 뒤(초대 수락·탈퇴·인계) 워크스페이스·조직 목록을 다시 읽는다. 겹치면 마지막 응답만 적용하고, 먼저 부른
   * 쪽도 그 뒤에 돌아간다(초대 수락 뒤 `org.select`가 새 조직이 든 목록을 봐야 한다).
   */
  async function reloadWorkspaces(): Promise<void> {
    workspacesPending = true;
    await workspacesReload.run(async (isCurrent) => {
      try {
        const [workspaces, organizations] = await Promise.all([
          api.workspaces(),
          api.myOrganizations(),
        ]);
        if (isCurrent() && session.state === 'signed-in') {
          session.workspaces = workspaces;
          org.load(session.me.id, organizations);
        }
      } finally {
        if (isCurrent()) workspacesPending = false;
      }
    });
  }

  /** 실시간 공동 편집을 켜거나 껐다. */
  function realtimeChanged(workspaceId: string, enabled: boolean): void {
    // 다시 들어오거나 다른 화면이 읽어도 지금 값이 보이게 목록의 그 워크스페이스를 고친다.
    if (session.state === 'signed-in') {
      session.workspaces = session.workspaces.map((w) =>
        w.id === workspaceId ? { ...w, realtime_enabled: enabled } : w,
      );
    }
    // 그 전에 보낸 목록 요청을 아직 기다리면 그 응답(바꾸기 전 값일 수 있다)은 버리고 지금 다시 읽는다. 그 요청이
    // 가져올 멤버십 변화는 새 응답에 들어 있다. 다시 읽지 못하면 고친 목록을 그대로 둔다.
    if (workspacesPending) reloadWorkspaces().catch(() => undefined);
  }

  async function logout(): Promise<void> {
    const redirect = await api.logout();
    org.reset();
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
      case 'my-tasks':
        return m.app_nav_my_tasks();
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
  // 지금 보는 워크스페이스의 조직이 현재 조직이다(D-111). 워크스페이스로 들어간 것은 사용자의 선택이므로 그 조직을 기억해
  // 뒤이어 홈·알림함 같은 개인 화면도 같은 조직 범위로 본다(구현 해석; 여러 조직 중 첫 조직을 조용히 고르는 것과 다르다).
  $effect(() => {
    org.routeOrganizationId = currentWorkspace?.organization_id;
    if (currentWorkspace !== undefined) org.select(currentWorkspace.organization_id);
  });
</script>

{#if session.state !== 'loading'}
  <!-- 표시 언어(D-81)를 정한 뒤에 그린다(토스트 영역 라벨도 그 언어로). -->
  <FeedbackHost />
{/if}

<!-- 계정 전환 안전 상태. 입력 보존을 위해 조건부 제거하지 않는다. -->
<div style:display={accountSafety.switching ? 'none' : 'contents'} inert={accountSafety.switching}>
  {#if route.name === 'public'}
    <PublicView token={route.token} />
  {:else if session.state === 'loading'}
    <p class="p-6" aria-live="polite">{m.app_loading()}</p>
  {:else if session.state === 'error'}
    <div class="flex flex-col items-start gap-3 p-6">
      <p class="text-destructive" role="alert">{session.message}</p>
      <Button variant="outline" onclick={() => void retry()}>{m.common_retry()}</Button>
    </div>
  {:else if session.state === 'anonymous'}
    <LoginScreen oidcEnabled={session.oidcEnabled} returnTo={router.path} />
  {:else}
    {@const signedIn = session}
    <SearchDialog
      bind:open={searchOpen}
      memberOf={(ws: string) => signedIn.workspaces.some((w) => w.id === ws)}
      workspace={currentWorkspace}
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

        <!-- 화면 안 여러 열 배치는 창 폭(md·lg)이 아니라 이 본문 폭(@container)을 본다. 사이드바가 차지한 폭과 사용자가 키운 루트
           글꼴(컨테이너 쿼리의 rem은 루트 글꼴을 따른다)을 함께 반영한다. -->
        <main
          class={`@container mx-auto flex w-full min-w-0 flex-col gap-8 px-4 py-6 sm:px-6 ${READING_ROUTES.has(route.name) ? 'max-w-3xl' : 'max-w-5xl'}`}
        >
          {#if route.name === 'home'}
            <!-- 조직 선택 전 홈은 선택 안내만 표시(D-111·D-120). -->
            {#if org.currentId === undefined}
              <p role="status">{m.app_organization_required()}</p>
            {:else}
              <HomeScreen
                workspaces={signedIn.workspaces.filter((w) => w.organization_id === org.currentId)}
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
            {/if}
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
              <WorkspaceScreen workspace={currentWorkspace} onrealtimechanged={realtimeChanged} />
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
          {:else if route.name === 'my-tasks'}
            <MyTasksScreen />
          {:else if route.name === 'notifications'}
            <!-- 초대 수락 뒤 조직·공간 문맥을 함께 갱신(D-112·D-118). 상세 id는 router.route에서 읽는다. -->
            <NotificationsScreen
              workspaces={signedIn.workspaces}
              onchanged={() => void refreshUnread(true)}
              onaccepted={reloadWorkspaces}
            />
          {:else if route.name === 'my-audit'}
            <AuditScreen workspace={undefined} />
          {:else if route.name === 'my-trash'}
            <div class="flex flex-col gap-8">
              <TrashScreen workspace={undefined} />
              <DeletedWorkspaces onchanged={() => void reloadWorkspaces()} />
            </div>
          {:else if route.name === 'time'}
            <!-- 공유 설정 제거(D-117)로 워크스페이스 목록을 넘기지 않는다. -->
            <TimeScreen />
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
</div>
