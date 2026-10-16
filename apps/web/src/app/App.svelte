<script lang="ts">
  // 앱 셸: 로그인 확인 → 사이드바(워크스페이스) + 경로별 화면.
  import type { Me, Workspace } from '@fvoci/api-client';

  import { api } from '$lib/api/client';
  import LoginScreen from '$lib/features/auth/LoginScreen.svelte';
  import DocumentScreen from '$lib/features/documents/DocumentScreen.svelte';
  import NotesScreen from '$lib/features/notes/NotesScreen.svelte';
  import SharedScreen from '$lib/features/notes/SharedScreen.svelte';
  import PublicView from '$lib/features/public/PublicView.svelte';
  import MembersScreen from '$lib/features/members/MembersScreen.svelte';
  import ItemScreen from '$lib/features/modules/ItemScreen.svelte';
  import ModuleScreen from '$lib/features/modules/ModuleScreen.svelte';
  import NotificationsScreen from '$lib/features/references/NotificationsScreen.svelte';
  import HomeScreen from '$lib/features/workspaces/HomeScreen.svelte';
  import WorkspaceScreen from '$lib/features/workspaces/WorkspaceScreen.svelte';
  import { itemPath, membersPath, router, workspacePath } from '$lib/router.svelte';
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

  async function load(): Promise<void> {
    try {
      const me = await api.me();
      if (me === undefined) {
        const info = await api.authInfo();
        session = { state: 'anonymous', oidcEnabled: info.oidc_enabled };
        return;
      }
      session = { state: 'signed-in', me, workspaces: await api.workspaces() };
    } catch (e: unknown) {
      session = {
        state: 'error',
        message: e instanceof Error ? e.message : '서버에 연결할 수 없습니다.',
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
  <p class="p-6" aria-live="polite">불러오는 중…</p>
{:else if session.state === 'error'}
  <p class="p-6 text-destructive" role="alert">{session.message}</p>
{:else if session.state === 'anonymous'}
  <LoginScreen oidcEnabled={session.oidcEnabled} returnTo={router.path} />
{:else}
  {@const signedIn = session}
  <Sidebar.Provider>
    <Sidebar.Root>
      <Sidebar.Header>
        <span class="px-2 text-base font-semibold">fvoci</span>
      </Sidebar.Header>
      <Sidebar.Content>
        <Sidebar.Group>
          <Sidebar.GroupLabel>워크스페이스</Sidebar.GroupLabel>
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
                      }}>모든 워크스페이스</a
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
                        }}>{ws.name}</a
                      >
                    {/snippet}
                  </Sidebar.MenuButton>
                </Sidebar.MenuItem>
              {/each}
            </Sidebar.Menu>
          </Sidebar.GroupContent>
        </Sidebar.Group>
        <Sidebar.Group>
          <Sidebar.GroupLabel>개인</Sidebar.GroupLabel>
          <Sidebar.GroupContent>
            <Sidebar.Menu>
              {#each [{ path: '/notes', label: '내 노트', active: route.name === 'notes' }, { path: '/shared', label: '공유받은 문서', active: route.name === 'shared' }, { path: '/notifications', label: unreadCount > 0 ? `알림 (${String(unreadCount)})` : '알림', active: route.name === 'notifications' }] as item (item.path)}
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
              ? '내 노트'
              : route.name === 'notifications'
                ? '알림'
                : route.name === 'shared'
                  ? '공유받은 문서'
                  : '홈')}</span
        >
        <div class="ml-auto">
          <DropdownMenu.Root>
            <DropdownMenu.Trigger>
              {#snippet child({ props })}
                <Button variant="outline" {...props} data-testid="user-menu"
                  >{signedIn.me.display_name}</Button
                >
              {/snippet}
            </DropdownMenu.Trigger>
            <DropdownMenu.Content align="end">
              <DropdownMenu.Item onSelect={() => void logout()}>로그아웃</DropdownMenu.Item>
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
            <p role="alert">워크스페이스가 없거나 볼 권한이 없습니다.</p>
          {:else if route.name === 'members'}
            <MembersScreen
              workspace={currentWorkspace}
              meId={signedIn.me.id}
              onchanged={() => void reloadWorkspaces()}
            />
          {:else if route.name === 'workspace'}
            <nav aria-label="워크스페이스 메뉴" class="text-sm">
              <a
                class="underline"
                href={membersPath(currentWorkspace.id)}
                onclick={(e: MouseEvent) => {
                  e.preventDefault();
                  router.navigate(membersPath(currentWorkspace.id));
                }}>멤버</a
              >
            </nav>
            <WorkspaceScreen workspace={currentWorkspace} />
          {:else if route.name === 'module'}
            <BackLink path={workspacePath(currentWorkspace.id)} label={currentWorkspace.name} />
            <ModuleScreen workspace={currentWorkspace} moduleId={route.moduleId} />
          {:else if route.name === 'item'}
            <BackLink path={workspacePath(currentWorkspace.id)} label={currentWorkspace.name} />
            <ItemScreen
              workspace={currentWorkspace}
              itemId={route.itemId}
              workspaces={signedIn.workspaces}
            />
          {:else if route.name === 'item-document'}
            <BackLink path={itemPath(currentWorkspace.id, route.itemId)} label="항목으로" />
            <DocumentScreen documentId={route.documentId} workspaces={signedIn.workspaces} />
          {:else if route.name === 'document'}
            <BackLink
              path={workspacePath(currentWorkspace.id)}
              label={`${currentWorkspace.name} 위키`}
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
        {:else if route.name === 'shared'}
          <SharedScreen />
        {:else if route.name === 'standalone-document'}
          <DocumentScreen documentId={route.documentId} workspaces={signedIn.workspaces} />
        {:else}
          <p role="alert">페이지를 찾을 수 없습니다.</p>
        {/if}
      </main>
    </Sidebar.Inset>
  </Sidebar.Provider>
{/if}
