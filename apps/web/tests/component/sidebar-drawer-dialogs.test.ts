import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { setLocale } from '$lib/paraglide/runtime.js';

import MobileSidebarHost from './fixtures/MobileSidebarHost.svelte';

// 좁은 화면에서 사이드바 서랍의 계정 메뉴로 "표시 설정"을 열면 서랍이 대화상자 위에 남아 가렸다(375·500px 실제 브라우저).
// 서랍 안에서 대화상자를 여는 동작은 서랍을 먼저 닫는다.
function phoneWidth(): void {
  vi.stubGlobal('matchMedia', (query: string) => ({
    matches: query.includes('max-width'),
    media: query,
    onchange: null,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
    addListener: () => undefined,
    removeListener: () => undefined,
    dispatchEvent: () => false,
  }));
}

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  phoneWidth();
});
afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

async function openDrawer(): Promise<void> {
  await fireEvent.click(screen.getByRole('button', { name: '사이드바 열고 닫기' }));
  await screen.findByTestId('user-menu');
}

const drawerOpen = (): boolean => screen.queryByTestId('user-menu') !== null;

describe('sidebar drawer and the dialogs it opens', () => {
  it('closes the drawer when the account menu opens display settings', async () => {
    const ondisplay = vi.fn();
    render(MobileSidebarHost, { onsearch: vi.fn(), ondisplay, onexport: vi.fn() });
    await openDrawer();
    await fireEvent.click(screen.getByTestId('user-menu'));
    await fireEvent.click(await screen.findByRole('menuitem', { name: '표시 설정' }));
    expect(ondisplay).toHaveBeenCalledTimes(1);
    await waitFor(() => {
      expect(drawerOpen()).toBe(false);
    });
  });

  it('closes the drawer when search opens', async () => {
    const onsearch = vi.fn();
    render(MobileSidebarHost, { onsearch, ondisplay: vi.fn(), onexport: vi.fn() });
    await openDrawer();
    await fireEvent.click(screen.getByTestId('search-open'));
    expect(onsearch).toHaveBeenCalledTimes(1);
    await waitFor(() => {
      expect(drawerOpen()).toBe(false);
    });
  });
});
