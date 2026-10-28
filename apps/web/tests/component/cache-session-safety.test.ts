import type { Me } from '@fvoci/api-client';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { api, ApiError } from '$lib/api/client';
import { activityApi } from '$lib/features/activity/api';
import { accountSafety, connection } from '$lib/features/feedback/feedback.svelte';
import FeedbackHost from '$lib/features/feedback/FeedbackHost.svelte';
import PublicView from '$lib/features/public/PublicView.svelte';
import { me as currentUser } from '$lib/me.svelte';
import { org } from '$lib/org.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';
import { router } from '$lib/router.svelte';

import { TEST_ORGANIZATION } from './fixtures/organization';
import App from '../../src/app/App.svelte';

const me: Me = {
  id: 'user-a',
  display_name: '민감한 사용자',
  hour_cycle: '',
  locale: 'ko',
  time_zone: '',
  week_start: '',
  theme: 'system',
  limits: {
    max_title_chars: 200,
    max_document_bytes: 2_000_000,
    max_references_per_document: 500,
    max_document_depth: 10,
  },
};
const replace = vi.fn();
const reload = vi.fn();

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}

function pageshow(persisted = true): void {
  window.dispatchEvent(new PageTransitionEvent('pageshow', { persisted }));
}

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  vi.stubGlobal('matchMedia', (query: string) => ({
    matches: false,
    media: query,
    addEventListener() {},
    removeEventListener() {},
  }));
  vi.spyOn(globalThis, 'fetch').mockResolvedValue(new Response('{}', { status: 404 }));
  vi.spyOn(api, 'me').mockResolvedValue(me);
  vi.spyOn(api, 'authInfo').mockResolvedValue({ oidc_enabled: true });
  vi.spyOn(api, 'workspaces').mockResolvedValue([]);
  vi.spyOn(api, 'myOrganizations').mockResolvedValue([TEST_ORGANIZATION]);
  vi.spyOn(api, 'deletedWorkspaces').mockResolvedValue([
    {
      id: 'deleted',
      name: '이전 비밀 목록',
      deleted_at: '2026-10-19T00:00:00Z',
      expires_at: '2026-11-19T00:00:00Z',
      expired: false,
    },
  ]);
  vi.spyOn(api, 'currentTimer').mockResolvedValue(null);
  vi.spyOn(activityApi, 'unreadCount').mockResolvedValue(0);
  replace.mockClear();
  reload.mockClear();
  currentUser.userId = '';
  router.navigate('/trash');
});

afterEach(() => {
  cleanup();
  router.guard = undefined;
  router.navigate('/');
  currentUser.userId = '';
  connection.lost = false;
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

async function signedIn(): Promise<void> {
  render(App, { replace, reload });
  await screen.findByText('이전 비밀 목록');
}

async function logout(): Promise<void> {
  await fireEvent.click(screen.getByTestId('user-menu'));
  await fireEvent.click(await screen.findByRole('menuitem', { name: '로그아웃' }));
}

// #212 1~3, 부조종사 2026-10-29 00:25: 응답·이동보다 먼저 이전 화면을 가린다.
it('clears session and organization before sending logout, and replaces history', async () => {
  const pending = deferred<string>();
  const send = vi.spyOn(api, 'logout').mockImplementation(() => {
    expect(accountSafety.switching).toBe(true);
    expect(currentUser.userId).toBe('');
    expect(org.currentId).toBeUndefined();
    expect(screen.queryByText('이전 비밀 목록')).toBeNull();
    return pending.promise;
  });
  await signedIn();
  await logout();
  expect(send).toHaveBeenCalledOnce();
  expect(screen.queryByText('이전 비밀 목록')).toBeNull();
  pending.resolve('/signed-out');
  await waitFor(() => {
    expect(replace).toHaveBeenCalledWith('/signed-out');
  });
});

it('keeps old data hidden and explains a failed logout', async () => {
  const pending = deferred<string>();
  vi.spyOn(api, 'logout').mockReturnValueOnce(pending.promise).mockResolvedValue('/signed-out');
  await signedIn();
  await logout();
  pending.reject(new Error('failed'));
  await waitFor(() => {
    expect(screen.getByRole('alert')).toHaveTextContent(/로그아웃/);
  });
  expect(accountSafety.switching).toBe(true);
  expect(screen.queryByText('이전 비밀 목록')).toBeNull();
  expect(replace).not.toHaveBeenCalled();
  await fireEvent.click(screen.getByRole('button', { name: '로그아웃 다시 하기' }));
  await waitFor(() => {
    expect(replace).toHaveBeenCalledWith('/signed-out');
  });
  expect(currentUser.userId).toBe('');
  expect(screen.queryByText('이전 비밀 목록')).toBeNull();
});

it('hides a persisted app synchronously until the same user and fresh data are loaded', async () => {
  await signedIn();
  const check = deferred<Me | undefined>();
  vi.mocked(api.me).mockReturnValueOnce(check.promise);
  vi.mocked(api.deletedWorkspaces).mockResolvedValue([]);
  pageshow();
  expect(screen.getByText('이전 비밀 목록')).not.toBeVisible();
  check.resolve(me);
  await waitFor(() => {
    expect(screen.queryByText('이전 비밀 목록')).toBeNull();
  });
  await waitFor(() => {
    expect(screen.getByTestId('user-menu')).toBeVisible();
  });
  expect(api.deletedWorkspaces).toHaveBeenCalledTimes(2);
});

it('goes to login after an anonymous persisted restore without exposing the old screen', async () => {
  await signedIn();
  const previous = screen.getByText('이전 비밀 목록');
  vi.mocked(api.me).mockResolvedValue(undefined);
  pageshow();
  expect(previous).not.toBeVisible();
  await waitFor(() => {
    expect(replace).toHaveBeenCalled();
  });
  expect(accountSafety.switching).toBe(true);
  expect(currentUser.userId).toBe('');
  await waitFor(() => {
    expect(screen.queryByText('이전 비밀 목록')).toBeNull();
  });
});

it('uses account switching when a persisted session belongs to someone else', async () => {
  await signedIn();
  vi.mocked(api.me).mockResolvedValue({ ...me, id: 'user-b' });
  pageshow();
  await waitFor(() => {
    expect(reload).toHaveBeenCalledOnce();
  });
  expect(accountSafety.switching).toBe(true);
  expect(screen.getByText('이전 비밀 목록')).not.toBeVisible();
});

it('keeps a persisted app hidden if session verification fails', async () => {
  await signedIn();
  vi.mocked(api.me).mockRejectedValue(new Error('offline'));
  pageshow();
  await screen.findByRole('alert');
  expect(screen.queryByText('이전 비밀 목록')).toBeNull();
});

it.each(['focus', 'visibilitychange'])(
  'hides and navigates a signed-out session on %s',
  async (event) => {
    currentUser.userId = me.id;
    vi.mocked(api.me).mockResolvedValue(undefined);
    render(FeedbackHost, { replace, reload });
    if (event === 'focus') window.dispatchEvent(new Event(event));
    else {
      Object.defineProperty(document, 'visibilityState', { configurable: true, value: 'visible' });
      document.dispatchEvent(new Event(event));
    }
    await waitFor(() => {
      expect(replace).toHaveBeenCalled();
    });
    expect(accountSafety.switching).toBe(true);
  },
);

const publicDoc = {
  title: '철회 전 공개 비밀',
  updated_at: '2026-10-19T00:00:00Z',
  body: {
    type: 'doc',
    content: [{ type: 'paragraph', content: [{ type: 'text', text: '공개 본문 비밀' }] }],
  },
};

it('clears a persisted public document before rechecking a revoked link', async () => {
  const read = vi.spyOn(api, 'publicDocument').mockResolvedValue(publicDoc);
  render(PublicView, { token: 'public-token' });
  await screen.findByText(publicDoc.title);
  const pending = deferred<typeof publicDoc>();
  read.mockReturnValueOnce(pending.promise);
  pageshow();
  expect(screen.queryByText(publicDoc.title)).toBeNull();
  expect(screen.queryByText('공개 본문 비밀')).toBeNull();
  pending.reject(new ApiError(404, 'not_found', 'gone'));
  expect(await screen.findByRole('alert')).toHaveTextContent(/만료|철회/);
  expect(screen.queryByText(publicDoc.title)).toBeNull();
});

it('reloads fresh public content on restore and ignores a nonpersisted pageshow', async () => {
  const read = vi.spyOn(api, 'publicDocument').mockResolvedValue(publicDoc);
  render(PublicView, { token: 'public-token' });
  await screen.findByText(publicDoc.title);
  pageshow(false);
  expect(read).toHaveBeenCalledTimes(1);
  read.mockResolvedValue({ ...publicDoc, title: '새 공개 제목' });
  pageshow();
  await screen.findByText('새 공개 제목');
  expect(screen.queryByText(publicDoc.title)).toBeNull();
});
