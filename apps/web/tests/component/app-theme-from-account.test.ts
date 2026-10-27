import type { Me } from '@fvoci/api-client';
import { cleanup, render, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { api } from '$lib/api/client';
import { me as currentUser } from '$lib/me.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';
import { router } from '$lib/router.svelte';
import { applyTheme, THEME_KEY } from '$lib/theme.svelte';

import { TEST_ORGANIZATION } from './fixtures/organization';

import App from '../../src/app/App.svelte';

// 화면 밝기의 정본은 계정(D-109)이다. 다른 기기에서 어둡게로 바꿨으면, 이 브라우저의 사본(첫 그림용)이 밝게여도 로그인하면
// 계정 값으로 맞추고 사본도 고쳐 다음 첫 그림이 맞게 한다.
const me: Me = {
  id: 'user-a',
  display_name: '나',
  hour_cycle: '',
  locale: 'ko',
  time_zone: '',
  week_start: '',
  theme: 'dark',
  limits: {
    max_title_chars: 200,
    max_document_bytes: 2_000_000,
    max_references_per_document: 500,
    max_document_depth: 10,
  },
};

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  vi.stubGlobal('matchMedia', (query: string) => ({
    matches: false,
    media: query,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
  }));
  vi.spyOn(globalThis, 'fetch').mockImplementation(() =>
    Promise.resolve(
      new Response(JSON.stringify({ error: 'not_found', message: 'not found' }), {
        status: 404,
        headers: { 'Content-Type': 'application/json' },
      }),
    ),
  );
  vi.spyOn(api, 'me').mockResolvedValue(me);
  vi.spyOn(api, 'workspaces').mockResolvedValue([]);
  vi.spyOn(api, 'myOrganizations').mockResolvedValue([TEST_ORGANIZATION]);
  vi.spyOn(api, 'unreadCount').mockResolvedValue(0);
  vi.spyOn(api, 'notifications').mockResolvedValue({ notifications: [], next: null });
  vi.spyOn(api, 'currentTimer').mockResolvedValue(null);
  vi.spyOn(api, 'deletedWorkspaces').mockResolvedValue([]);
});

afterEach(() => {
  // 테마를 되돌리면 알림이 다시 그려지며 matchMedia를 부르므로, 먼저 내리고 되돌린다.
  cleanup();
  applyTheme('light');
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  window.localStorage.clear();
  currentUser.theme = 'system';
  router.navigate('/');
});

describe('theme from the account', () => {
  it('applies the account theme on sign-in and updates this browser copy', async () => {
    window.localStorage.setItem(THEME_KEY, 'light');
    applyTheme('light');
    render(App);
    await waitFor(() => {
      expect(document.documentElement).toHaveClass('dark');
    });
    expect(currentUser.theme).toBe('dark');
    expect(window.localStorage.getItem(THEME_KEY)).toBe('dark');
  });
});
