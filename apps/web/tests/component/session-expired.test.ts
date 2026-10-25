import { fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { api } from '$lib/api/client';
import FeedbackHost from '$lib/features/feedback/FeedbackHost.svelte';
import { auth, connection } from '$lib/features/feedback/feedback.svelte';
import { me as currentUser } from '$lib/me.svelte';
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

// `/api/me` 계약의 서버 입력 한도(#79): 필수 칸이라 계정 응답 mock에도 넣는다(서버 기본값).
const limits = {
  max_title_chars: 200,
  max_document_bytes: 2_000_000,
  max_references_per_document: 500,
  max_document_depth: 10,
};

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
    currentUser.userId = '';
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

  /** 401로 배너를 띄운 뒤, 다른 탭에서 `id`로 로그인하고 이 창으로 돌아온다. */
  async function signInElsewhereAs(id: string): Promise<void> {
    vi.stubGlobal('fetch', () => Promise.resolve(json(401, { error: 'unauthorized' })));
    await expect(api.workspaces()).rejects.toThrow();
    await vi.waitFor(() => {
      expect(signInLink()).not.toBeNull();
    });

    vi.stubGlobal('fetch', (input: RequestInfo | URL) =>
      Promise.resolve(
        input === '/api/me'
          ? json(200, { id, display_name: '나', email: null, limits })
          : json(404, { error: 'not_found' }),
      ),
    );
    await fireEvent.focus(window);
  }

  it('lowers the banner after signing in elsewhere and coming back to this window', async () => {
    currentUser.userId = 'u1';
    const reload = vi.fn();
    render(FeedbackHost, { props: { reload } });
    await signInElsewhereAs('u1');
    await vi.waitFor(() => {
      expect(signInLink()).toBeNull();
    });
    // 같은 계정이면 이 창의 화면과 입력을 그대로 둔다.
    expect(reload).not.toHaveBeenCalled();
  });

  // 리뷰 지적: 새 탭에서 다른 계정으로 로그인하고 돌아오면 배너만 내려가고 앞 계정의 화면·입력이 그대로 남았다.
  it('reloads the page when the new tab signed in as another account', async () => {
    currentUser.userId = 'u1';
    const reload = vi.fn();
    render(FeedbackHost, { props: { reload } });
    await signInElsewhereAs('u2');
    await vi.waitFor(() => {
      expect(reload).toHaveBeenCalledOnce();
    });
  });

  // 리뷰 지적: 세션이 끝나지 않은 채(이 창이 아직 401을 받기 전) 다른 탭에서 로그아웃하고 다른 계정으로 로그인하면,
  // 쿠키는 이미 바뀌었는데 이 창은 돌아와도 확인하지 않아 앞 계정의 화면과 입력이 새 계정의 세션으로 남았다.
  it('reloads on return when another account signed in without an expired session', async () => {
    currentUser.userId = 'u1';
    const reload = vi.fn();
    const me = (id: string) => (input: RequestInfo | URL) =>
      Promise.resolve(
        input === '/api/me'
          ? json(200, { id, display_name: '나', email: null, limits })
          : json(404, { error: 'not_found' }),
      );
    render(FeedbackHost, { props: { reload } });
    expect(auth.expired).toBe(false);

    // 같은 계정이면 그대로 둔다.
    vi.stubGlobal('fetch', me('u1'));
    await fireEvent.focus(window);
    await new Promise((r) => setTimeout(r, 0));
    expect(reload).not.toHaveBeenCalled();

    vi.stubGlobal('fetch', me('u2'));
    await fireEvent.focus(window);
    await vi.waitFor(() => {
      expect(reload).toHaveBeenCalledOnce();
    });
  });

  // 리뷰 지적: 401보다 먼저 시작한 요청의 성공이 늦게 오면, 세션이 돌아오지 않았는데도 배너가 내려갔다.
  it('keeps the banner when a request started before the 401 succeeds late', async () => {
    let release: (response: Response) => void = () => undefined;
    vi.stubGlobal('fetch', (input: RequestInfo | URL) =>
      input === '/api/workspaces'
        ? new Promise<Response>((resolve) => {
            release = resolve;
          })
        : Promise.resolve(json(401, { error: 'unauthorized' })),
    );
    render(FeedbackHost);
    const older = api.workspaces();
    await expect(api.notifications('org-1')).rejects.toThrow();
    await vi.waitFor(() => {
      expect(signInLink()).not.toBeNull();
    });

    release(json(200, []));
    await expect(older).resolves.toEqual([]);
    expect(auth.expired).toBe(true);
    expect(signInLink()).not.toBeNull();
  });
});
