import { fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { setLocale } from '$lib/paraglide/runtime.js';
import { parseRoute, router } from '$lib/router.svelte';
import MobileSidebarHost from './fixtures/MobileSidebarHost.svelte';

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  vi.stubGlobal('matchMedia', (query: string) => ({
    matches: false,
    media: query,
    onchange: null,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
    addListener: () => undefined,
    removeListener: () => undefined,
    dispatchEvent: () => false,
  }));
  router.replace('/');
});
afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

it('recognizes both server management tabs and defaults the root to organizations', () => {
  for (const path of ['/admin', '/admin/', '/admin/organizations', '/admin/organizations/'])
    expect(parseRoute(path)).toEqual({ name: 'server-admin', tab: 'organizations' });
  expect(parseRoute('/admin/admins')).toEqual({ name: 'server-admin', tab: 'admins' });
  for (const path of ['/admin/users', '/admin/admins/extra'])
    expect(parseRoute(path)).toEqual({ name: 'not-found' });
});

it.each([false, true])(
  'shows the personal menu entry only with is_server_admin=%s',
  async (isServerAdmin) => {
    render(MobileSidebarHost, {
      isServerAdmin,
      onsearch: vi.fn(),
      ondisplay: vi.fn(),
      onexport: vi.fn(),
    });
    await fireEvent.click(screen.getByTestId('user-menu'));
    await screen.findByRole('menuitem', { name: '표시 설정' });
    const entry = screen.queryByRole('menuitem', { name: '서버 관리' });
    if (isServerAdmin) {
      expect(entry).not.toBeNull();
      if (entry !== null) await fireEvent.click(entry);
      expect(router.path).toBe('/admin/organizations');
    } else expect(entry).toBeNull();
  },
);
