import { fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { api } from '$lib/api/client';
import FeedbackHost from '$lib/features/feedback/FeedbackHost.svelte';
import { auth, connection } from '$lib/features/feedback/feedback.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';
import { router } from '$lib/router.svelte';

// 쓰는 중에 세션이 끝나면(감사 W31) 앱 전체에 남는 배너와 새 탭에서 여는 로그인 링크를 보인다. 이 창의 입력은
// 그대로 두고, 다른 탭에서 로그인한 뒤 돌아오면 배너를 내린다.
function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });
}

const signInLink = (): HTMLAnchorElement | null =>
  document.querySelector<HTMLAnchorElement>('a[href^="/auth/login"]');

describe('session expired mid-use', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
    vi.stubGlobal('matchMedia', (query: string) => ({
      matches: false,
      media: query,
      addEventListener: () => undefined,
      removeEventListener: () => undefined,
    }));
    router.navigate('/w/ws-1/items/it-1#task-3');
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
    auth.expired = false;
    connection.lost = false;
  });

  it('shows a banner with a sign-in link that opens in a new tab and returns to this page', async () => {
    vi.stubGlobal('fetch', () =>
      Promise.resolve(json(401, { error: 'unauthorized', message: 'Unauthorized' })),
    );
    render(FeedbackHost);
    await expect(api.workspaces()).rejects.toThrow();

    await vi.waitFor(() => {
      expect(signInLink()).not.toBeNull();
    });
    const link = signInLink();
    if (link === null) throw new Error('link');
    expect(link.getAttribute('href')).toBe(
      `/auth/login?return_to=${encodeURIComponent('/w/ws-1/items/it-1#task-3')}`,
    );
    expect(link).toHaveAttribute('target', '_blank');
    expect(link.getAttribute('rel')).toContain('noopener');
    expect(screen.getByTestId('session-banner')).toHaveTextContent('입력한 내용은 이 창에 그대로');
  });

  it('does not treat the start-up sign-in check as an expired session', async () => {
    vi.stubGlobal('fetch', () => Promise.resolve(json(401, { error: 'unauthorized' })));
    render(FeedbackHost);
    expect(await api.me()).toBeUndefined();
    expect(auth.expired).toBe(false);
    expect(signInLink()).toBeNull();
  });

  it('lowers the banner after signing in elsewhere and coming back to this window', async () => {
    vi.stubGlobal('fetch', () => Promise.resolve(json(401, { error: 'unauthorized' })));
    render(FeedbackHost);
    await expect(api.workspaces()).rejects.toThrow();
    await vi.waitFor(() => {
      expect(signInLink()).not.toBeNull();
    });

    vi.stubGlobal('fetch', (input: RequestInfo | URL) =>
      Promise.resolve(
        input === '/api/me'
          ? json(200, { id: 'u1', display_name: '나', email: null })
          : json(404, { error: 'not_found' }),
      ),
    );
    await fireEvent.focus(window);
    await vi.waitFor(() => {
      expect(signInLink()).toBeNull();
    });
  });
});
