<script lang="ts">
  // 앱 사이드바. 맨 위: 조직 › 워크스페이스 전환기(D-121)와 검색. 개인 영역(모든 워크스페이스에 걸친 내 것)과 현재
  // 워크스페이스 영역(위키 트리, 모듈, 달력·멤버·휴지통·감사 기록)을 나눈다. 아래: 계정 메뉴(표시 설정·내려받기·로그아웃).
  // 목록은 서버가 권한으로 거른 것만 그린다. 데스크톱은 아이콘 막대로 접히고, 모바일은 서랍으로 열고 이동하면 닫힌다.
  import type { Workspace } from '@fvoci/api-client';
  import BookOpen from '@lucide/svelte/icons/book-open';
  import Bell from '@lucide/svelte/icons/bell';
  import Calendar from '@lucide/svelte/icons/calendar';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import ChevronsUpDown from '@lucide/svelte/icons/chevrons-up-down';
  import Clock from '@lucide/svelte/icons/clock';
  import Download from '@lucide/svelte/icons/download';
  import House from '@lucide/svelte/icons/house';
  import Building2 from '@lucide/svelte/icons/building-2';
  import ListChecks from '@lucide/svelte/icons/list-checks';
  import LogOut from '@lucide/svelte/icons/log-out';
  import NotebookPen from '@lucide/svelte/icons/notebook-pen';
  import Plus from '@lucide/svelte/icons/plus';
  import ScrollText from '@lucide/svelte/icons/scroll-text';
  import Search from '@lucide/svelte/icons/search';
  import Settings2 from '@lucide/svelte/icons/settings-2';
  import Share2 from '@lucide/svelte/icons/share-2';
  import Trash from '@lucide/svelte/icons/trash';
  import Users from '@lucide/svelte/icons/users';
  import X from '@lucide/svelte/icons/x';

  import type { Health } from '@fvoci/api-client';
  import { fetchHealth } from '$lib/api/client';
  import { buildLabel } from '$lib/features/health/build';
  import { kindLabel } from '$lib/features/modules/kinds';
  import ModuleKindIcon from '$lib/features/modules/ModuleKindIcon.svelte';
  import WorkspaceBadge from '$lib/features/workspaces/WorkspaceBadge.svelte';
  import { org } from '$lib/org.svelte';
  import { m } from '$lib/paraglide/messages.js';
  import { readLocal, writeLocal } from '$lib/storage';
  import {
    auditPath,
    calendarPath,
    membersPath,
    modulePath,
    organizationPath,
    router,
    trashPath,
    workspacePath,
  } from '$lib/router.svelte';
  import * as DropdownMenu from '$lib/ui/dropdown-menu';
  import * as Sidebar from '$lib/ui/sidebar';
  import { useSidebar } from '$lib/ui/sidebar';

  import type { NavItem } from './nav-item';

  import NavLink from './NavLink.svelte';
  import SidebarDocTree from './SidebarDocTree.svelte';
  import { workspaceNav } from './workspace-nav.svelte';

  interface Props {
    workspaces: Workspace[];
    /** 경로가 가리키는 워크스페이스(없으면 마지막으로 연 워크스페이스를 보인다). */
    current: Workspace | undefined;
    displayName: string;
    unreadCount: number;
    onsearch: () => void;
    ondisplay: () => void;
    onexport: () => void;
    onlogout: () => void;
  }

  const {
    workspaces,
    current,
    displayName,
    unreadCount,
    onsearch,
    ondisplay,
    onexport,
    onlogout,
  }: Props = $props();

  const sidebar = useSidebar();
  const route = $derived(router.route);

  // 계정 메뉴를 열 때 실행 중인 빌드를 서버에 묻는다(화면은 버전 문자열을 따로 들지 않는다, D-87). 받으면 다시 묻지
  // 않고, 서버에 닿지 못했으면 다음에 메뉴를 열 때 다시 묻는다.
  let build = $state<Health['build'] | undefined>(undefined);
  let buildAsking = false;
  function askBuild(open: boolean): void {
    if (!open || build !== undefined || buildAsking) return;
    buildAsking = true;
    void fetchHealth().then((r) => {
      buildAsking = false;
      if (r.kind !== 'unreachable') build = r.health.build;
    });
  }

  // 현재 조직의 워크스페이스만 전환기에 보인다(D-111). 조직을 고르지 않았으면 모두 보인다(각 조직의 워크스페이스로
  // 들어가면 그 조직이 현재 조직이 된다).
  const inOrganization = $derived(
    org.currentId === undefined
      ? workspaces
      : workspaces.filter((w) => w.organization_id === org.currentId),
  );

  // 마지막으로 연 워크스페이스(이 브라우저에만 두는 편의 기억). 키에 사용자·조직을 넣고(D-121) 지금 목록에 있을 때만 쓴다.
  const LAST = 'lastWorkspace';
  // 조직이 바뀌면 그 조직에서 마지막으로 연 워크스페이스를 되살리고, 워크스페이스를 열면 그것으로 바꾼다.
  let lastId = $derived<string | null>(readLocal(org.key(LAST)));
  $effect(() => {
    if (current === undefined) return;
    lastId = current.id;
    writeLocal(org.key(LAST), current.id);
  });
  const shown = $derived(current ?? inOrganization.find((w) => w.id === lastId));

  /**
   * 조직을 바꾼다. 다른 조직의 워크스페이스를 보고 있었으면 홈으로 간다(화면은 현재 조직 범위다). 그 이동은 저장하지 않은
   * 편집의 이탈 확인을 먼저 거치고, 떠나기로 확정된 뒤에야 선택(메모리·저장소)과 화면을 함께 바꾼다. 취소하면 현재 조직·
   * 워크스페이스·저장된 선택·화면·입력을 그대로 둔다(CodeRabbit outside-diff 1e6de63c). 워크스페이스 밖 화면(홈·내 업무·
   * 시간)도 조직 범위 입력(예: 홈 빠른 추가)이 조직과 함께 바뀌므로 같은 확인을 거친다(R5 F-6). 같은 조직이면 묻지 않는다.
   */
  function chooseOrganization(id: string): void {
    if (id === org.currentId) return;
    router.confirmLeave(() => {
      org.select(id);
      if (current !== undefined && current.organization_id !== id) router.navigate('/');
    });
  }

  // 화면을 옮길 때 오래된 목록이면 다시 받는다(다른 화면에서 바뀐 제목·이동 반영).
  $effect(() => {
    if (router.path !== '' && shown !== undefined) workspaceNav.show(shown.id);
  });

  // 모바일 서랍은 이동하면 닫는다.
  $effect(() => {
    if (router.path !== '') sidebar.setOpenMobile(false);
  });

  // 대화상자를 여는 동작(검색·표시 설정·내보내기)도 서랍을 먼저 닫는다. 서랍이 대화상자 위에 남아 가렸다.
  function closingDrawer(action: () => void): () => void {
    return () => {
      sidebar.setOpenMobile(false);
      action();
    };
  }

  function go(e: MouseEvent, path: string): void {
    if (e.metaKey || e.ctrlKey || e.shiftKey || e.button !== 0) return;
    e.preventDefault();
    router.navigate(path);
  }

  const activeDocId = $derived(
    route.name === 'document' && route.workspaceId === shown?.id ? route.documentId : undefined,
  );
  const activeModuleId = $derived.by(() => {
    if (shown === undefined || !('workspaceId' in route) || route.workspaceId !== shown.id)
      return undefined;
    if (route.name === 'module') return route.moduleId;
    if (route.name === 'item' || route.name === 'item-document')
      return workspaceNav.currentItem?.itemId === route.itemId
        ? workspaceNav.currentItem.moduleId
        : undefined;
    return undefined;
  });
  // 위키 트리: 위키 화면에서는 펼치고, 사용자가 접거나 펼친 뒤로는 그 선택을 따른다.
  let wikiToggled = $state<boolean | undefined>(undefined);
  const wikiOpen = $derived(
    wikiToggled ?? (route.name === 'workspace' || route.name === 'document'),
  );

  const inShown = (name: string): boolean =>
    shown !== undefined &&
    'workspaceId' in route &&
    route.workspaceId === shown.id &&
    route.name === name;

  const personal = $derived<NavItem[]>([
    { path: '/', label: m.app_nav_home(), icon: House, active: route.name === 'home' },
    {
      path: '/tasks',
      label: m.app_nav_my_tasks(),
      icon: ListChecks,
      active: route.name === 'my-tasks',
    },
    {
      path: '/notifications',
      label: m.app_nav_notifications(),
      icon: Bell,
      active: route.name === 'notifications',
      badge: unreadCount,
    },
    { path: '/notes', label: m.app_nav_notes(), icon: NotebookPen, active: route.name === 'notes' },
    { path: '/shared', label: m.app_nav_shared(), icon: Share2, active: route.name === 'shared' },
    { path: '/time', label: m.app_nav_time(), icon: Clock, active: route.name === 'time' },
    { path: '/trash', label: m.app_nav_my_trash(), icon: Trash, active: route.name === 'my-trash' },
    {
      path: '/audit',
      label: m.app_nav_my_audit(),
      icon: ScrollText,
      active: route.name === 'my-audit',
    },
  ]);

  const workspaceLinks = $derived.by(() => {
    if (shown === undefined) return [];
    const links: NavItem[] = [
      {
        path: calendarPath(shown.id),
        label: m.app_calendar(),
        icon: Calendar,
        active: inShown('calendar'),
      },
      {
        path: membersPath(shown.id),
        label: m.app_members(),
        icon: Users,
        active: inShown('members'),
      },
      { path: trashPath(shown.id), label: m.app_trash(), icon: Trash, active: inShown('trash') },
    ];
    if (shown.access === 'owner')
      links.push({
        path: auditPath(shown.id),
        label: m.app_audit(),
        icon: ScrollText,
        active: inShown('audit'),
      });
    return links;
  });
</script>

<Sidebar.Root collapsible="icon">
  <Sidebar.Header>
    <!-- 사이드바 맨 위에 조직 › 워크스페이스 전환기(D-111·D-121). 조직은 사용자가 고르고 서버는 기본 조직을 정하지 않는다.
         모바일 서랍의 닫기 단추는 같은 줄 끝에 둔다. -->
    <div class="flex items-start gap-1">
      <Sidebar.Menu class="min-w-0 flex-1">
        <Sidebar.MenuItem>
          <DropdownMenu.Root>
            <DropdownMenu.Trigger>
              {#snippet child({ props })}
                <Sidebar.MenuButton
                  {...props}
                  size="lg"
                  tooltipContent={org.current?.name ?? m.app_nav_organization_choose()}
                  aria-label={`${m.app_nav_organization_switch()}: ${org.current?.name ?? m.app_nav_organization_choose()}`}
                  data-testid="organization-switcher"
                  class="border border-sidebar-border bg-sidebar"
                >
                  <Building2 aria-hidden="true" />
                  {#if org.current !== undefined}
                    <span class="min-w-0 flex-1 truncate group-data-[collapsible=icon]:hidden"
                      >{org.current.name}</span
                    >
                  {:else}
                    <span class="min-w-0 flex-1 truncate text-muted-foreground"
                      >{m.app_nav_organization_choose()}</span
                    >
                  {/if}
                  <ChevronsUpDown class="ml-auto" aria-hidden="true" />
                </Sidebar.MenuButton>
              {/snippet}
            </DropdownMenu.Trigger>
            <DropdownMenu.Content
              align="start"
              class="w-(--bits-dropdown-menu-anchor-width) min-w-56 max-w-[calc(100vw-2rem)]"
            >
              <DropdownMenu.Label>{m.app_nav_organization_switch()}</DropdownMenu.Label>
              {#if org.organizations.length === 0}
                <p class="px-2 py-1.5 text-sm text-muted-foreground">
                  {m.app_nav_organization_none()}
                </p>
              {/if}
              {#each org.organizations as o (o.id)}
                <DropdownMenu.Item
                  onSelect={() => {
                    chooseOrganization(o.id);
                  }}
                  aria-current={o.id === org.currentId ? 'true' : undefined}
                  class={o.id === org.currentId ? 'font-medium' : ''}
                >
                  <span class="min-w-0 flex-1 truncate">{o.name}</span>
                  <span class="text-xs text-muted-foreground"
                    >{o.role === 'admin'
                      ? m.app_nav_organization_role_admin()
                      : m.app_nav_organization_role_member()}</span
                  >
                </DropdownMenu.Item>
              {/each}
              {#if org.current !== undefined}
                <DropdownMenu.Separator />
                <DropdownMenu.Item
                  onSelect={() => {
                    if (org.currentId !== undefined) {
                      sidebar.setOpenMobile(false);
                      router.navigate(organizationPath(org.currentId));
                    }
                  }}
                >
                  {org.current.role === 'admin'
                    ? m.organizations_manage()
                    : m.organizations_member_entry()}
                </DropdownMenu.Item>
              {/if}
            </DropdownMenu.Content>
          </DropdownMenu.Root>
        </Sidebar.MenuItem>
        <Sidebar.MenuItem>
          <DropdownMenu.Root>
            <DropdownMenu.Trigger>
              {#snippet child({ props })}
                <Sidebar.MenuButton
                  {...props}
                  size="lg"
                  tooltipContent={shown?.name ?? m.app_nav_workspace_choose()}
                  aria-label={`${m.app_nav_workspace_switch()}: ${shown?.name ?? m.app_nav_workspace_choose()}`}
                  data-testid="workspace-switcher"
                  class="border border-sidebar-border bg-sidebar"
                >
                  {#if shown !== undefined}
                    <span
                      class="flex min-w-0 flex-1 overflow-hidden group-data-[collapsible=icon]:hidden"
                      ><WorkspaceBadge workspace={shown} all={workspaces} /></span
                    >
                  {:else}
                    <span class="min-w-0 flex-1 truncate text-muted-foreground"
                      >{m.app_nav_workspace_choose()}</span
                    >
                  {/if}
                  <ChevronsUpDown class="ml-auto" aria-hidden="true" />
                </Sidebar.MenuButton>
              {/snippet}
            </DropdownMenu.Trigger>
            <DropdownMenu.Content
              align="start"
              class="w-(--bits-dropdown-menu-anchor-width) min-w-56 max-w-[calc(100vw-2rem)]"
            >
              <DropdownMenu.Label>{m.app_nav_workspace_switch()}</DropdownMenu.Label>
              {#each inOrganization as ws (ws.id)}
                <DropdownMenu.Item
                  onSelect={() => {
                    router.navigate(workspacePath(ws.id));
                  }}
                  aria-current={ws.id === shown?.id ? 'true' : undefined}
                  class={ws.id === shown?.id ? 'font-medium' : ''}
                >
                  <WorkspaceBadge workspace={ws} all={inOrganization} />
                </DropdownMenu.Item>
              {/each}
              <DropdownMenu.Separator />
              <DropdownMenu.Item
                onSelect={() => {
                  router.navigate('/');
                }}
              >
                <Plus aria-hidden="true" />{m.app_nav_workspace_manage()}
              </DropdownMenu.Item>
            </DropdownMenu.Content>
          </DropdownMenu.Root>
        </Sidebar.MenuItem>
      </Sidebar.Menu>
      {#if sidebar.isMobile}
        <button
          type="button"
          class="flex size-8 shrink-0 items-center justify-center rounded-md hover:bg-sidebar-accent focus-visible:ring-2 focus-visible:ring-sidebar-ring focus-visible:outline-none"
          aria-label={m.app_nav_close()}
          onclick={() => {
            sidebar.setOpenMobile(false);
          }}
        >
          <X class="size-4" aria-hidden="true" />
        </button>
      {/if}
    </div>
    <Sidebar.Menu>
      <Sidebar.MenuItem>
        <Sidebar.MenuButton
          variant="outline"
          tooltipContent={m.app_search()}
          onclick={closingDrawer(onsearch)}
          data-testid="search-open"
          class="text-muted-foreground"
        >
          <Search aria-hidden="true" />
          <span>{m.app_search()}</span>
          <kbd class="ml-auto font-mono text-xs group-data-[collapsible=icon]:hidden">Ctrl/⌘ K</kbd>
        </Sidebar.MenuButton>
      </Sidebar.MenuItem>
    </Sidebar.Menu>
  </Sidebar.Header>

  <Sidebar.Content>
    <Sidebar.Group>
      <Sidebar.GroupLabel>{m.app_nav_personal()}</Sidebar.GroupLabel>
      <Sidebar.GroupContent>
        <Sidebar.Menu>
          {#each personal as item (item.path)}
            <NavLink {item} />
          {/each}
        </Sidebar.Menu>
      </Sidebar.GroupContent>
    </Sidebar.Group>

    {#if shown !== undefined}
      <Sidebar.Separator />

      <Sidebar.Group>
        <Sidebar.GroupLabel>{m.app_nav_workspaces()}</Sidebar.GroupLabel>
        <Sidebar.GroupContent class="flex flex-col gap-1">
          <Sidebar.Menu>
            <Sidebar.MenuItem>
              <Sidebar.MenuButton isActive={inShown('workspace')} tooltipContent={m.app_nav_wiki()}>
                {#snippet child({ props })}
                  <a
                    href={workspacePath(shown.id)}
                    {...props}
                    aria-current={inShown('workspace') ? 'page' : undefined}
                    onclick={(e: MouseEvent) => {
                      go(e, workspacePath(shown.id));
                    }}
                  >
                    <BookOpen aria-hidden="true" />
                    <span>{m.app_nav_wiki()}</span>
                  </a>
                {/snippet}
              </Sidebar.MenuButton>
              <Sidebar.MenuAction
                aria-expanded={wikiOpen}
                aria-controls="sidebar-wiki-tree"
                aria-label={wikiOpen
                  ? m.app_nav_collapse({ title: m.app_nav_wiki() })
                  : m.app_nav_expand({ title: m.app_nav_wiki() })}
                class="md:opacity-100"
                onclick={() => {
                  wikiToggled = !wikiOpen;
                }}
              >
                <ChevronRight
                  class={`transition-transform ${wikiOpen ? 'rotate-90' : ''}`}
                  aria-hidden="true"
                />
              </Sidebar.MenuAction>
            </Sidebar.MenuItem>
          </Sidebar.Menu>
          {#if wikiOpen}
            <div
              id="sidebar-wiki-tree"
              class="group-data-[collapsible=icon]:hidden"
              data-testid="sidebar-wiki-tree"
            >
              {#if workspaceNav.docs.state === 'error'}
                <p class="flex items-center gap-2 px-2 py-1 text-xs text-destructive" role="alert">
                  {m.app_nav_load_failed()}
                  <button
                    type="button"
                    class="rounded-sm underline underline-offset-2 focus-visible:ring-2 focus-visible:ring-sidebar-ring focus-visible:outline-none"
                    onclick={() => {
                      workspaceNav.refreshDocs();
                    }}>{m.common_retry()}</button
                  >
                </p>
              {:else if workspaceNav.docs.state === 'ready'}
                {#if workspaceNav.docs.list.length === 0}
                  <p class="px-2 py-1 text-xs text-muted-foreground">{m.app_nav_docs_empty()}</p>
                {:else}
                  <nav aria-label={m.app_nav_wiki()} class="pl-2">
                    <SidebarDocTree
                      workspaceId={shown.id}
                      docs={workspaceNav.docs.list}
                      activeId={activeDocId}
                    />
                  </nav>
                {/if}
              {/if}
            </div>
          {/if}
          <Sidebar.Menu>
            {#each workspaceLinks as item (item.path)}
              <NavLink {item} />
            {/each}
          </Sidebar.Menu>
        </Sidebar.GroupContent>
      </Sidebar.Group>
    {/if}

    {#if shown !== undefined}
      <Sidebar.Group class="group-data-[collapsible=icon]:hidden">
        <Sidebar.GroupLabel>{m.app_nav_modules()}</Sidebar.GroupLabel>
        <Sidebar.GroupContent>
          {#if workspaceNav.modules.state === 'error'}
            <p class="flex items-center gap-2 px-2 py-1 text-xs text-destructive" role="alert">
              {m.app_nav_load_failed()}
              <button
                type="button"
                class="rounded-sm underline underline-offset-2 focus-visible:ring-2 focus-visible:ring-sidebar-ring focus-visible:outline-none"
                onclick={() => {
                  workspaceNav.refreshModules();
                }}>{m.common_retry()}</button
              >
            </p>
          {:else if workspaceNav.modules.state === 'ready'}
            {#if workspaceNav.modules.list.filter((mod) => !mod.archived).length === 0}
              <p class="px-2 py-1 text-xs text-muted-foreground">{m.app_nav_modules_empty()}</p>
            {:else}
              <Sidebar.Menu data-testid="sidebar-modules">
                {#each workspaceNav.modules.list.filter((mod) => !mod.archived) as mod (mod.id)}
                  {@const active = mod.id === activeModuleId}
                  <Sidebar.MenuItem>
                    <Sidebar.MenuButton isActive={active}>
                      {#snippet child({ props })}
                        <a
                          href={modulePath(shown.id, mod.id)}
                          {...props}
                          aria-current={active ? 'page' : undefined}
                          title={`${mod.name} · ${kindLabel(mod.kind)} · ${mod.key}`}
                          onclick={(e: MouseEvent) => {
                            go(e, modulePath(shown.id, mod.id));
                          }}
                        >
                          <ModuleKindIcon kind={mod.kind} />
                          <span class="min-w-0 flex-1 truncate">{mod.name}</span>
                          <span class="shrink-0 font-mono text-xs text-muted-foreground"
                            >{mod.key}</span
                          >
                        </a>
                      {/snippet}
                    </Sidebar.MenuButton>
                  </Sidebar.MenuItem>
                {/each}
              </Sidebar.Menu>
            {/if}
          {/if}
        </Sidebar.GroupContent>
      </Sidebar.Group>
    {/if}
  </Sidebar.Content>

  <Sidebar.Footer>
    <Sidebar.Menu>
      <Sidebar.MenuItem>
        <DropdownMenu.Root onOpenChange={askBuild}>
          <DropdownMenu.Trigger>
            {#snippet child({ props })}
              <Sidebar.MenuButton
                {...props}
                tooltipContent={displayName}
                aria-label={`${m.app_nav_account()}: ${displayName}`}
                data-testid="user-menu"
              >
                <Settings2 aria-hidden="true" />
                <span class="min-w-0 flex-1 truncate">{displayName}</span>
                <ChevronsUpDown class="ml-auto" aria-hidden="true" />
              </Sidebar.MenuButton>
            {/snippet}
          </DropdownMenu.Trigger>
          <DropdownMenu.Content side="top" align="start" class="min-w-56">
            <DropdownMenu.Item onSelect={closingDrawer(ondisplay)}>
              <Settings2 aria-hidden="true" />{m.app_menu_display()}
            </DropdownMenu.Item>
            <DropdownMenu.Item onSelect={closingDrawer(onexport)}>
              <Download aria-hidden="true" />{m.app_menu_export()}
            </DropdownMenu.Item>
            <DropdownMenu.Separator />
            <DropdownMenu.Item onSelect={onlogout}>
              <LogOut aria-hidden="true" />{m.app_menu_logout()}
            </DropdownMenu.Item>
            <DropdownMenu.Separator />
            <DropdownMenu.Label
              class="text-xs font-normal text-muted-foreground"
              title={build?.source}
              data-testid="build-version">{buildLabel(build)}</DropdownMenu.Label
            >
          </DropdownMenu.Content>
        </DropdownMenu.Root>
      </Sidebar.MenuItem>
    </Sidebar.Menu>
  </Sidebar.Footer>
  <Sidebar.Rail />
</Sidebar.Root>
