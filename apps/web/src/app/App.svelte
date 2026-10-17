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
  import NotificationsScreen from '$lib/features/references/NotificationsScreen.svelte';
  import TimeScreen from '$lib/features/time/TimeScreen.svelte';
  import TimerWidget from '$lib/features/time/TimerWidget.svelte';
  import HomeScreen from '$lib/features/workspaces/HomeScreen.svelte';
  import WorkspaceBadge from '$lib/features/workspaces/WorkspaceBadge.svelte';
  import WorkspaceScreen from '$lib/features/workspaces/WorkspaceScreen.svelte';
  import { me as currentUser } from '$lib/me.svelte';
  import {
    calendarPath,
    itemPath,
    membersPath,
    router,
    auditPath,
    trashPath,
    workspacePath,
  } from '$lib/router.svelte';
  import { Button } from '$lib/ui/button';
  import * as DropdownMenu from '$lib/ui/dropdown-menu';
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

  /** 멤버십이 바뀐 뒤(초대 수락·탈퇴·인계) 워크스페이스 목록을 다시 읽는다. */
  // 읽지 않은 알림 수. 경로가 바뀔 때마다 다시 센다(알림은 접근 권한을 따라 서버가 거른다).
  let unreadCount = $state(0);
  async function refreshUnread(): Promise<void> {
    if (session.state !== 'signed-in') return;
    try {
      unreadCount = (await api.notifications()).filter((n) => !n.read).length;
    } catch {
      unreadCount = 0;
    }
  }
  $effect(() => {
    if (session.state === 'signed-in' && router.path !== '') void refreshUnread();
  });

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
  const currentWorkspace = $derived.by(() => {
    if (session.state !== 'signed-in') return undefined;
    if (!WORKSPACE_ROUTES.has(route.name)) return undefined;
    if (!('workspaceId' in route)) return undefined;
    const id = route.workspaceId;
    return session.workspaces.find((w) => w.id === id);
  });
</script>

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
  <Sidebar.Provider>
    <Sidebar.Root>
      <Sidebar.Header>
        <span class="px-2 text-base font-semibold">fvoci</span>
        <Button
          variant="outline"
          class="justify-between text-muted-foreground"
          onclick={() => {
            searchOpen = true;
          }}
          data-testid="search-open"
        >
          {m.app_search()}<kbd class="text-xs">Ctrl/⌘ K</kbd>
        </Button>
      </Sidebar.Header>
      <Sidebar.Content>
        <Sidebar.Group>
          <Sidebar.GroupLabel>{m.app_nav_workspaces()}</Sidebar.GroupLabel>
          <Sidebar.GroupContent>
            <Sidebar.Menu>
              <Sidebar.MenuItem>
                <Sidebar.MenuButton isActive={route.name === 'home'}>
                  {#snippet child({ props })}
                    <a
                      href="/"
                      {...props}
                      onclick={(e: MouseEvent) => {
                        e.preventDefault();
                        router.navigate('/');
                      }}>{m.app_nav_all_workspaces()}</a
                    >
                  {/snippet}
                </Sidebar.MenuButton>
              </Sidebar.MenuItem>
              {#each signedIn.workspaces as ws (ws.id)}
                <Sidebar.MenuItem>
                  <Sidebar.MenuButton isActive={currentWorkspace?.id === ws.id}>
                    {#snippet child({ props })}
                      <a
                        href={workspacePath(ws.id)}
                        {...props}
                        onclick={(e: MouseEvent) => {
                          e.preventDefault();
                          router.navigate(workspacePath(ws.id));
                        }}><WorkspaceBadge workspace={ws} all={signedIn.workspaces} /></a
                      >
                    {/snippet}
                  </Sidebar.MenuButton>
                </Sidebar.MenuItem>
              {/each}
            </Sidebar.Menu>
          </Sidebar.GroupContent>
        </Sidebar.Group>
        <Sidebar.Group>
          <Sidebar.GroupLabel>{m.app_nav_personal()}</Sidebar.GroupLabel>
          <Sidebar.GroupContent>
            <Sidebar.Menu>
              {#each [{ path: '/notes', label: m.app_nav_notes(), active: route.name === 'notes' }, { path: '/shared', label: m.app_nav_shared(), active: route.name === 'shared' }, { path: '/time', label: m.app_nav_time(), active: route.name === 'time' }, { path: '/trash', label: m.app_nav_my_trash(), active: route.name === 'my-trash' }, { path: '/audit', label: m.app_nav_my_audit(), active: route.name === 'my-audit' }, { path: '/notifications', label: unreadCount > 0 ? m.app_nav_notifications_unread( { count: unreadCount } ) : m.app_nav_notifications(), active: route.name === 'notifications' }] as item (item.path)}
                <Sidebar.MenuItem>
                  <Sidebar.MenuButton isActive={item.active}>
                    {#snippet child({ props })}
                      <a
                        href={item.path}
                        {...props}
                        onclick={(e: MouseEvent) => {
                          e.preventDefault();
                          router.navigate(item.path);
                        }}>{item.label}</a
                      >
                    {/snippet}
                  </Sidebar.MenuButton>
                </Sidebar.MenuItem>
              {/each}
            </Sidebar.Menu>
          </Sidebar.GroupContent>
        </Sidebar.Group>
      </Sidebar.Content>
    </Sidebar.Root>

    <Sidebar.Inset>
      <header class="flex items-center gap-2 border-b px-4 py-2">
        <Sidebar.Trigger />
        <span class="text-base font-semibold"
          >{currentWorkspace?.name ??
            (route.name === 'notes'
              ? m.app_nav_notes()
              : route.name === 'notifications'
                ? m.app_nav_notifications()
                : route.name === 'shared'
                  ? m.app_nav_shared()
                  : m.app_title_home())}</span
        >
        <div class="ml-auto flex items-center gap-2">
          <TimerWidget />
          <DropdownMenu.Root>
            <DropdownMenu.Trigger>
              {#snippet child({ props })}
                <Button variant="outline" {...props} data-testid="user-menu"
                  >{signedIn.me.display_name}</Button
                >
              {/snippet}
            </DropdownMenu.Trigger>
            <DropdownMenu.Content align="end">
              <DropdownMenu.Item
                onSelect={() => {
                  window.location.assign('/api/me/export');
                }}>{m.app_menu_export()}</DropdownMenu.Item
              >
              <DropdownMenu.Item onSelect={() => (displayOpen = true)}
                >{m.app_menu_display()}</DropdownMenu.Item
              >
              <DropdownMenu.Item onSelect={() => void logout()}
                >{m.app_menu_logout()}</DropdownMenu.Item
              >
            </DropdownMenu.Content>
          </DropdownMenu.Root>
        </div>
      </header>

      <main class="mx-auto flex w-full max-w-3xl flex-col gap-8 px-4 py-6">
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
            <nav aria-label={m.app_workspace_menu()} class="text-sm">
              <a
                class="underline"
                href={membersPath(currentWorkspace.id)}
                onclick={(e: MouseEvent) => {
                  e.preventDefault();
                  router.navigate(membersPath(currentWorkspace.id));
                }}>{m.app_members()}</a
              >
              <a
                class="ml-3 underline"
                href={calendarPath(currentWorkspace.id)}
                onclick={(e: MouseEvent) => {
                  e.preventDefault();
                  router.navigate(calendarPath(currentWorkspace.id));
                }}>{m.app_calendar()}</a
              >
              <a
                class="ml-3 underline"
                href={trashPath(currentWorkspace.id)}
                onclick={(e: MouseEvent) => {
                  e.preventDefault();
                  router.navigate(trashPath(currentWorkspace.id));
                }}>{m.app_trash()}</a
              >
              {#if currentWorkspace.role === 'admin'}
                <a
                  class="ml-3 underline"
                  href={auditPath(currentWorkspace.id)}
                  onclick={(e: MouseEvent) => {
                    e.preventDefault();
                    router.navigate(auditPath(currentWorkspace.id));
                  }}>{m.app_audit()}</a
                >
              {/if}
            </nav>
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
            <BackLink path={workspacePath(currentWorkspace.id)} label={currentWorkspace.name} />
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
            onchanged={() => void refreshUnread()}
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
