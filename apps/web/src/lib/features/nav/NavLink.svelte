<script lang="ts">
  // 사이드바 링크 한 줄. 접힌 사이드바에서는 아이콘과 툴팁만 보인다. 경로 이동은 앱 라우터로(미저장 보호를 거친다).
  import type { NavItem } from './nav-item';

  import { m } from '$lib/paraglide/messages.js';
  import { router } from '$lib/router.svelte';
  import * as Sidebar from '$lib/ui/sidebar';

  interface Props {
    item: NavItem;
  }

  const { item }: Props = $props();

  function go(e: MouseEvent): void {
    if (e.metaKey || e.ctrlKey || e.shiftKey || e.button !== 0) return;
    e.preventDefault();
    router.navigate(item.path);
  }
</script>

<Sidebar.MenuItem>
  <Sidebar.MenuButton isActive={item.active} tooltipContent={item.label}>
    {#snippet child({ props })}
      <a href={item.path} {...props} aria-current={item.active ? 'page' : undefined} onclick={go}>
        <item.icon aria-hidden="true" />
        <span>{item.label}</span>
      </a>
    {/snippet}
  </Sidebar.MenuButton>
  {#if item.badge !== undefined && item.badge > 0}
    <Sidebar.MenuBadge>
      <span aria-hidden="true">{item.badge > 99 ? '99+' : item.badge}</span>
      <span class="sr-only">{m.app_nav_unread({ count: item.badge })}</span>
    </Sidebar.MenuBadge>
  {/if}
</Sidebar.MenuItem>
