<script lang="ts">
  // 사이드바 링크 한 줄. 접힌 사이드바에서는 아이콘과 툴팁만 보인다. 경로 이동은 앱 라우터로(미저장 보호를 거친다).
  import type { NavItem } from './nav-item';
  import { prefetchMyTasks, prefetchOnIntent } from '$lib/features/common/navigation-intent';

  import { m } from '$lib/paraglide/messages.js';
  import { router } from '$lib/router.svelte';
  import * as Sidebar from '$lib/ui/sidebar';

  interface Props {
    item: NavItem;
  }

  const { item }: Props = $props();
  const unread = $derived(item.badge ?? 0);
  // 읽지 않은 수는 링크 이름과 툴팁에 넣는다(접힌 사이드바에서는 숫자 배지가 숨는다).
  const name = $derived(
    unread > 0 ? `${item.label} · ${m.app_nav_unread({ count: unread })}` : item.label,
  );

  function go(e: MouseEvent): void {
    if (e.metaKey || e.ctrlKey || e.shiftKey || e.button !== 0) return;
    e.preventDefault();
    router.navigate(item.path);
  }

  function prefetchTasks(): void {
    if (item.path === '/tasks' && router.route.name === 'home') prefetchMyTasks();
  }
</script>

<Sidebar.MenuItem>
  <Sidebar.MenuButton isActive={item.active} tooltipContent={name} class="relative">
    {#snippet child({ props })}
      <a
        href={item.path}
        use:prefetchOnIntent={prefetchTasks}
        {...props}
        aria-label={unread > 0 ? name : undefined}
        aria-current={item.active ? 'page' : undefined}
        onclick={go}
      >
        <item.icon aria-hidden="true" />
        {#if unread > 0}
          <span
            class="absolute top-1 left-5 hidden size-2 rounded-full bg-primary group-data-[collapsible=icon]:block"
            aria-hidden="true"
          ></span>
        {/if}
        <span>{item.label}</span>
      </a>
    {/snippet}
  </Sidebar.MenuButton>
  {#if unread > 0}
    <Sidebar.MenuBadge aria-hidden="true">{unread > 99 ? '99+' : unread}</Sidebar.MenuBadge>
  {/if}
</Sidebar.MenuItem>
