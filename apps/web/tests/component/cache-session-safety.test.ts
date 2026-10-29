import type { Me } from '@fvoci/api-client';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { tick } from 'svelte';

import { api, ApiError } from '$lib/api/client';
import { activityApi } from '$lib/features/activity/api';
import {
  accountSafety,
  auth,
  connection,
  hideSession,
} from '$lib/features/feedback/feedback.svelte';
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
const sessionKey = 'fvoci:auth-session';

function isMarker(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function sessionMarker(): Record<string, unknown> {
  const value: unknown = JSON.parse(localStorage.getItem(sessionKey) ?? '{}');
  if (!isMarker(value)) throw new Error('invalid marker');
  return value;
}

function nextMarkerGeneration(): number {
  const generation = sessionMarker().generation;
  return (typeof generation === 'number' ? generation : 0) + 1;
}

function signalSession(userId: string | null): void {
  localStorage.setItem(
    sessionKey,
    JSON.stringify({
      state: 'settled',
      userId,
      generation: nextMarkerGeneration(),
      sequence: crypto.randomUUID(),
    }),
  );
}

function signalLogout(startedAt = Date.now()): string | null {
  localStorage.setItem(
    sessionKey,
    JSON.stringify({
      state: 'logout-pending',
      userId: me.id,
      startedAt,
      attemptId: crypto.randomUUID(),
      generation: nextMarkerGeneration(),
      sequence: crypto.randomUUID(),
    }),
  );
  return localStorage.getItem(sessionKey);
}

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
  localStorage.removeItem(sessionKey);
  auth.expired = false;
  await setLocale('ko', { reload: false });
  vi.stubGlobal('matchMedia', (query: string) => ({
    matches: false,
    media: query,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
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
  vi.useRealTimers();
  hideSession('account');
  accountSafety.switching = false;
  router.guard = undefined;
  router.navigate('/');
  currentUser.userId = '';
  connection.lost = false;
  auth.expired = false;
  localStorage.removeItem(sessionKey);
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

// SR-221-5: /api/me 시작 → 최종 익명 → 옛 A 응답. storage 배달보다 먼저 현재 세대를 읽어야 한다.
it.each(['initial', 'return', 'live', 'persisted'])(
  'does not publish an old-user response over a newer final generation during %s verification',
  async (kind) => {
    if (kind !== 'initial') await signedIn();
    const oldCheck = deferred<Me | undefined>();
    const freshCheck = deferred<Me | undefined>();
    vi.spyOn(api, 'me').mockReturnValueOnce(oldCheck.promise).mockReturnValue(freshCheck.promise);
    if (kind === 'initial') render(App, { replace, reload });
    else if (kind === 'return') window.dispatchEvent(new Event('focus'));
    else if (kind === 'persisted') pageshow();
    else {
      signalSession(null);
      window.dispatchEvent(new StorageEvent('storage', { key: sessionKey }));
    }
    signalSession(null);
    const finalMarker = localStorage.getItem(sessionKey);
    oldCheck.resolve(me);
    await oldCheck.promise;
    await tick();
    expect(localStorage.getItem(sessionKey)).toBe(finalMarker);
    const menu = screen.queryByTestId('user-menu');
    if (menu !== null) expect(menu).not.toBeVisible();
    freshCheck.resolve(undefined);
    if (kind === 'initial') await screen.findByRole('link', { name: /^로그인$/ });
    else if (kind === 'persisted')
      await waitFor(() => {
        expect(replace).toHaveBeenCalled();
      });
    else await screen.findByRole('link', { name: '새 탭에서 로그인' });
  },
);

// SR-221-6: 시도 A 시작 → 다른 탭 B의 진행/성공 → A 실패.
it.each(['pending', 'success'])(
  "does not let a failed logout replace another attempt's %s marker",
  async (other) => {
    const attempt = deferred<string>();
    vi.spyOn(api, 'logout').mockReturnValueOnce(attempt.promise);
    await signedIn();
    await logout();
    if (other === 'pending') signalLogout();
    else signalSession(null);
    const latest = localStorage.getItem(sessionKey);
    attempt.reject(new Error('old attempt failed'));
    await screen.findByRole('alert');
    expect(localStorage.getItem(sessionKey)).toBe(latest);
    expect(screen.queryByText('이전 비밀 목록')).toBeNull();
    expect(replace).not.toHaveBeenCalled();
  },
);

it('always publishes final anonymity when its logout succeeds after a newer attempt starts', async () => {
  const attempt = deferred<string>();
  vi.spyOn(api, 'logout').mockReturnValueOnce(attempt.promise);
  await signedIn();
  await logout();
  signalLogout();
  attempt.resolve('/signed-out');
  await waitFor(() => {
    expect(replace).toHaveBeenCalledWith('/signed-out');
  });
  expect(sessionMarker()).toMatchObject({ state: 'settled', userId: null });
});

// SR-221-7: 진행 표식으로 가림 → 키 소실/읽기 거부/만료 → 새 확인 실패 → 입력을 보존한 재시도.
it.each(['removed', 'denied', 'expired'])(
  'recovers a hidden pending logout after its marker is %s and preserves input across a failed check',
  async (kind) => {
    const input = await liveInput();
    vi.useFakeTimers({ toFake: ['Date', 'setTimeout', 'clearTimeout'] });
    signalLogout();
    window.dispatchEvent(new StorageEvent('storage', { key: sessionKey }));
    await tick();
    expect(input).not.toBeVisible();
    const check = deferred<Me | undefined>();
    const read = vi.spyOn(api, 'me').mockReturnValueOnce(check.promise);
    const reads = read.mock.calls.length;
    if (kind === 'removed') localStorage.removeItem(sessionKey);
    else if (kind === 'denied') {
      const original: unknown = Object.getOwnPropertyDescriptor(
        Storage.prototype,
        'getItem',
      )?.value;
      if (typeof original !== 'function') throw new Error('getItem missing');
      vi.spyOn(Storage.prototype, 'getItem').mockImplementation(function (this: Storage, key) {
        if (key === sessionKey) throw new Error('denied');
        const result: unknown = Reflect.apply(original, this, [key]);
        if (result === null || typeof result === 'string') return result;
        throw new Error('invalid storage value');
      });
    }
    await vi.advanceTimersByTimeAsync(30_000);
    vi.useRealTimers();
    if (kind === 'expired') signalLogout(Date.now() - 30_000);
    expect(read).toHaveBeenCalledTimes(reads + 1);
    expect(input).not.toBeVisible();
    check.reject(new Error('verification failed'));
    await screen.findByRole('alert');
    expect(input).toHaveValue('메모리에만 남긴 입력');
    read.mockResolvedValue(me);
    await fireEvent.click(screen.getByRole('button', { name: '다시 시도' }));
    await waitFor(() => {
      expect(input).toBeVisible();
    });
    expect(screen.getByLabelText('새 노트 제목')).toBe(input);
    expect(api.myNotes).toHaveBeenCalledTimes(1);
  },
);

it('checks immediately when storage is cleared after a pending logout', async () => {
  const input = await liveInput();
  signalLogout();
  window.dispatchEvent(new StorageEvent('storage', { key: sessionKey }));
  await tick();
  const check = deferred<Me | undefined>();
  const read = vi.spyOn(api, 'me').mockReturnValueOnce(check.promise);
  const reads = read.mock.calls.length;
  localStorage.removeItem(sessionKey);
  window.dispatchEvent(new StorageEvent('storage', { key: null }));
  expect(read).toHaveBeenCalledTimes(reads + 1);
  expect(input).not.toBeVisible();
  check.resolve(me);
  await waitFor(() => {
    expect(input).toBeVisible();
  });
});

it('keeps restored input mounted when a pending marker disappears and verification fails', async () => {
  const input = await liveInput();
  signalLogout();
  pageshow();
  await tick();
  const check = deferred<Me | undefined>();
  vi.spyOn(api, 'me').mockReturnValueOnce(check.promise);
  localStorage.removeItem(sessionKey);
  window.dispatchEvent(new StorageEvent('storage', { key: sessionKey }));
  check.reject(new Error('offline'));
  await screen.findByRole('alert');
  expect(input.isConnected).toBe(true);
  expect(input).toHaveValue('메모리에만 남긴 입력');
  expect(input).not.toBeVisible();
  expect(screen.getByRole('button', { name: '다시 시도' })).toBeVisible();
});

// SR-221-1 / D-96 / #88: 불일치 표식은 네트워크 응답보다 먼저 가린다. 순번은 비교하지 않는다.
it('publishes a fresh pending logout before each attempt and the original user after failure', async () => {
  const attempts: unknown[] = [];
  const send = vi.spyOn(api, 'logout').mockImplementation(() => {
    const marker: unknown = JSON.parse(localStorage.getItem(sessionKey) ?? '{}');
    expect(marker).toMatchObject({
      state: 'logout-pending',
      userId: me.id,
    });
    expect(marker).toHaveProperty('startedAt', expect.any(Number));
    attempts.push(marker);
    return Promise.reject(new Error('offline'));
  });
  await signedIn();
  expect(JSON.parse(localStorage.getItem(sessionKey) ?? '{}')).toMatchObject({ userId: me.id });
  await logout();
  await screen.findByRole('alert');
  expect(JSON.parse(localStorage.getItem(sessionKey) ?? '{}')).toMatchObject({
    state: 'settled',
    userId: me.id,
  });
  await fireEvent.click(screen.getByRole('button', { name: '로그아웃 다시 하기' }));
  await waitFor(() => {
    expect(send).toHaveBeenCalledTimes(2);
  });
  expect(attempts[0]).not.toEqual(attempts[1]);
  expect(JSON.parse(localStorage.getItem(sessionKey) ?? '{}')).toMatchObject({
    state: 'settled',
    userId: me.id,
  });
});

// SR-221-4: 서버가 로그아웃되기 전 A를 돌려주는 순서를 고정한다.
it.each(['live', 'persisted'])(
  'keeps the %s screen hidden when the old user responds during logout and verifies final anonymity',
  async (kind) => {
    await signedIn();
    const previous = screen.getByText('이전 비밀 목록');
    const check = deferred<Me | undefined>();
    vi.spyOn(api, 'me').mockReturnValueOnce(check.promise);
    const pending = signalLogout();
    if (kind === 'persisted') pageshow();
    else window.dispatchEvent(new StorageEvent('storage', { key: sessionKey }));
    expect(previous).not.toBeVisible();
    check.resolve(me);
    await check.promise;
    await tick();
    expect(previous).not.toBeVisible();
    expect(localStorage.getItem(sessionKey)).toBe(pending);
    expect(api.workspaces).toHaveBeenCalledTimes(1);
    vi.spyOn(api, 'me').mockResolvedValue(undefined);
    signalSession(null);
    window.dispatchEvent(new StorageEvent('storage', { key: sessionKey }));
    if (kind === 'persisted')
      await waitFor(() => {
        expect(replace).toHaveBeenCalled();
      });
    else await screen.findByRole('link', { name: '새 탭에서 로그인' });
    expect(previous).not.toBeVisible();
  },
);

it('resumes the same mounted input only after the failed logout final marker is verified', async () => {
  const input = await liveInput();
  const pending = signalLogout();
  window.dispatchEvent(new StorageEvent('storage', { key: sessionKey }));
  await tick();
  expect(input).not.toBeVisible();
  expect(localStorage.getItem(sessionKey)).toBe(pending);
  const finalCheck = deferred<Me | undefined>();
  vi.spyOn(api, 'me').mockReturnValueOnce(finalCheck.promise);
  signalSession(me.id);
  window.dispatchEvent(new StorageEvent('storage', { key: sessionKey }));
  expect(input).not.toBeVisible();
  finalCheck.resolve(me);
  await waitFor(() => expect(input).toBeVisible());
  expect(screen.getByLabelText('새 노트 제목')).toBe(input);
  expect(input).toHaveValue('메모리에만 남긴 입력');
  expect(api.myNotes).toHaveBeenCalledTimes(1);
});

it('rechecks an abandoned logout at 30 seconds without another browser event', async () => {
  const input = await liveInput();
  vi.useFakeTimers({ toFake: ['Date', 'setTimeout', 'clearTimeout'] });
  signalLogout();
  window.dispatchEvent(new StorageEvent('storage', { key: sessionKey }));
  await vi.advanceTimersByTimeAsync(29_999);
  expect(input).not.toBeVisible();
  const read = vi.spyOn(api, 'me');
  const reads = read.mock.calls.length;
  await vi.advanceTimersByTimeAsync(1);
  await tick();
  expect(read).toHaveBeenCalledTimes(reads + 1);
  expect(input).toBeVisible();
  expect(input).toHaveValue('메모리에만 남긴 입력');
  expect(api.myNotes).toHaveBeenCalledTimes(1);
});

it('rejects the expired-pending old response when a deduplicated final anonymous marker has a newer generation', async () => {
  const input = await liveInput();
  vi.useFakeTimers({ toFake: ['Date', 'setTimeout', 'clearTimeout'] });
  const oldCheck = deferred<Me | undefined>();
  const freshCheck = deferred<Me | undefined>();
  const read = vi
    .spyOn(api, 'me')
    .mockResolvedValueOnce(me)
    .mockReturnValueOnce(oldCheck.promise)
    .mockReturnValueOnce(freshCheck.promise);
  signalLogout();
  window.dispatchEvent(new StorageEvent('storage', { key: sessionKey }));
  await vi.advanceTimersByTimeAsync(30_000);
  expect(input).not.toBeVisible();
  const expiredReads = read.mock.calls.length;
  signalSession(null);
  const final = localStorage.getItem(sessionKey);
  window.dispatchEvent(new StorageEvent('storage', { key: sessionKey }));
  // 같은 null 신호는 중복 억제된다. 응답 수락 경계의 세대 검사가 이 경합을 막는다.
  expect(read).toHaveBeenCalledTimes(expiredReads);
  oldCheck.resolve(me);
  await vi.advanceTimersByTimeAsync(0);
  expect(read).toHaveBeenCalledTimes(expiredReads + 1);
  expect(input).not.toBeVisible();
  expect(localStorage.getItem(sessionKey)).toBe(final);
  freshCheck.resolve(undefined);
  await vi.advanceTimersByTimeAsync(0);
  vi.useRealTimers();
  await screen.findByRole('link', { name: '새 탭에서 로그인' });
  expect(input).not.toBeVisible();
  expect(input).toHaveValue('메모리에만 남긴 입력');
  expect(sessionMarker()).toMatchObject({ state: 'settled', userId: null });
});

it.each([true, false])(
  'discards an old-user verification after final anonymity (storage delivered: %s)',
  async (delivered) => {
    const input = await liveInput();
    const oldCheck = deferred<Me | undefined>();
    vi.spyOn(api, 'me').mockReturnValueOnce(oldCheck.promise).mockResolvedValue(undefined);
    signalLogout();
    window.dispatchEvent(new StorageEvent('storage', { key: sessionKey }));
    signalSession(null);
    if (delivered) {
      window.dispatchEvent(new StorageEvent('storage', { key: sessionKey }));
      await screen.findByRole('link', { name: '새 탭에서 로그인' });
    }
    oldCheck.resolve(me);
    await oldCheck.promise;
    await screen.findByRole('link', { name: '새 탭에서 로그인' });
    expect(input).not.toBeVisible();
    expect(input).toHaveValue('메모리에만 남긴 입력');
    expect(JSON.parse(localStorage.getItem(sessionKey) ?? '{}')).toMatchObject({ userId: null });
  },
);

it('does not let a newly opened shell overwrite a pending logout with its old user', async () => {
  const pending = signalLogout();
  render(App, { replace, reload });
  const previous = await screen.findByText('이전 비밀 목록');
  await tick();
  expect(previous).not.toBeVisible();
  expect(localStorage.getItem(sessionKey)).toBe(pending);
});

it('cancels abandoned-logout verification when the shell unmounts', async () => {
  const view = render(App, { replace, reload });
  await screen.findByText('이전 비밀 목록');
  vi.useFakeTimers({ toFake: ['Date', 'setTimeout', 'clearTimeout'] });
  signalLogout();
  window.dispatchEvent(new StorageEvent('storage', { key: sessionKey }));
  await tick();
  view.unmount();
  const read = vi.spyOn(api, 'me');
  const reads = read.mock.calls.length;
  await vi.advanceTimersByTimeAsync(30_000);
  expect(read).toHaveBeenCalledTimes(reads);
});

it('publishes an anonymous shell result without making public views publish a marker', async () => {
  vi.spyOn(api, 'me').mockResolvedValue(undefined);
  const view = render(App, { replace, reload });
  await screen.findByRole('link', { name: '로그인' });
  expect(JSON.parse(localStorage.getItem(sessionKey) ?? '{}')).toMatchObject({ userId: null });
  view.unmount();
  localStorage.removeItem(sessionKey);
  router.navigate('/p/public-token-1234');
  vi.spyOn(api, 'publicDocument').mockResolvedValue(publicDoc);
  const reads = vi.spyOn(Storage.prototype, 'getItem');
  render(App, { replace, reload });
  await screen.findByText(publicDoc.title);
  expect(reads.mock.calls.filter(([key]) => key === sessionKey)).toHaveLength(0);
  expect(localStorage.getItem(sessionKey)).toBeNull();
});

it.each(['storage', 'focus', 'visibilitychange', 'pageshow'])(
  'hides synchronously on a mismatched marker at %s and preserves the same live screen',
  async (event) => {
    await signedIn();
    const check = deferred<Me | undefined>();
    vi.spyOn(api, 'me').mockReturnValueOnce(check.promise);
    signalSession('user-b');
    if (event === 'visibilitychange') {
      Object.defineProperty(document, 'visibilityState', { configurable: true, value: 'visible' });
      document.dispatchEvent(new Event(event));
    } else if (event === 'storage') {
      window.dispatchEvent(new StorageEvent(event, { key: sessionKey }));
    } else if (event === 'pageshow') pageshow(false);
    else window.dispatchEvent(new Event(event));
    expect(screen.getByText('이전 비밀 목록')).not.toBeVisible();
    expect(accountSafety.switching).toBe(true);
    expect(api.workspaces).toHaveBeenCalledTimes(1);
    check.resolve(me);
    await waitFor(() => expect(screen.getByTestId('user-menu')).toBeVisible());
    expect(screen.getByText('이전 비밀 목록')).toBeVisible();
    expect(api.workspaces).toHaveBeenCalledTimes(1);
    expect(api.deletedWorkspaces).toHaveBeenCalledTimes(1);
  },
);

it('keeps a signed-out marker hidden through a failed verification and offers retry', async () => {
  await signedIn();
  signalSession(null);
  vi.spyOn(api, 'me').mockRejectedValueOnce(new Error('offline')).mockResolvedValue(undefined);
  window.dispatchEvent(new StorageEvent('storage', { key: sessionKey }));
  expect(screen.getByText('이전 비밀 목록')).not.toBeVisible();
  await screen.findByRole('alert');
  expect(replace).not.toHaveBeenCalled();
  await fireEvent.click(screen.getByRole('button', { name: '다시 시도' }));
  await screen.findByRole('link', { name: '새 탭에서 로그인' });
  expect(replace).not.toHaveBeenCalled();
  expect(currentUser.userId).toBe(me.id);
  expect(screen.getByText('이전 비밀 목록')).not.toBeVisible();
});

it.each(['storage', 'focus', 'visibilitychange', 'pageshow'])(
  'preserves the same user screen and inputs at %s even when the network fails',
  async (event) => {
    await signedIn();
    signalSession(me.id);
    vi.spyOn(api, 'me').mockRejectedValue(new Error('offline'));
    if (event === 'visibilitychange') document.dispatchEvent(new Event(event));
    else if (event === 'storage')
      window.dispatchEvent(new StorageEvent(event, { key: sessionKey }));
    else if (event === 'pageshow') pageshow(false);
    else window.dispatchEvent(new Event(event));
    expect(screen.getByText('이전 비밀 목록')).toBeVisible();
    expect(accountSafety.switching).toBe(false);
    expect(replace).not.toHaveBeenCalled();
  },
);

it('deduplicates return signals during verification and unsubscribes on unmount', async () => {
  const view = render(App, { replace, reload });
  await screen.findByText('이전 비밀 목록');
  const check = deferred<Me | undefined>();
  const read = vi.spyOn(api, 'me').mockReturnValue(check.promise);
  signalSession('user-b');
  window.dispatchEvent(new StorageEvent('storage', { key: sessionKey }));
  window.dispatchEvent(new Event('focus'));
  document.dispatchEvent(new Event('visibilitychange'));
  pageshow(false);
  expect(read).toHaveBeenCalledTimes(2);
  view.unmount();
  signalSession(null);
  window.dispatchEvent(new StorageEvent('storage', { key: sessionKey }));
  window.dispatchEvent(new Event('focus'));
  document.dispatchEvent(new Event('visibilitychange'));
  pageshow(false);
  expect(read).toHaveBeenCalledTimes(2);
  check.resolve(undefined);
  await Promise.resolve();
  expect(replace).not.toHaveBeenCalled();
});

it('keeps authentication usable when session marker reads and writes are denied', async () => {
  vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
    throw new Error('denied');
  });
  vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
    throw new Error('denied');
  });
  vi.spyOn(api, 'logout').mockResolvedValue('/signed-out');
  await signedIn();
  window.dispatchEvent(new Event('focus'));
  expect(screen.getByText('이전 비밀 목록')).toBeVisible();
  await logout();
  await waitFor(() => {
    expect(replace).toHaveBeenCalledWith('/signed-out');
  });
});

// #212 1~3, 부조종사 2026-10-29 00:25: 응답·이동보다 먼저 이전 화면을 가린다.
it('clears session and organization before sending logout, and replaces history', async () => {
  const pending = deferred<string>();
  const markersAtReplace: unknown[] = [];
  replace.mockImplementationOnce(() => {
    markersAtReplace.push(JSON.parse(localStorage.getItem(sessionKey) ?? '{}'));
  });
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
  expect(markersAtReplace).toEqual([expect.objectContaining({ state: 'settled', userId: null })]);
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
  vi.spyOn(api, 'me').mockReturnValueOnce(check.promise);
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

it('keeps pre-cache hiding and verifies only when the page is restored', async () => {
  await signedIn();
  const check = vi.spyOn(api, 'me');
  const previous = screen.getByText('이전 비밀 목록');
  window.dispatchEvent(new PageTransitionEvent('pagehide', { persisted: true }));
  expect(previous).not.toBeVisible();
  expect(check).toHaveBeenCalledTimes(1);
  vi.spyOn(api, 'deletedWorkspaces').mockResolvedValue([]);
  pageshow();
  await waitFor(() => {
    expect(screen.getByTestId('user-menu')).toBeVisible();
  });
  expect(check).toHaveBeenCalledTimes(2);
  expect(screen.queryByText('이전 비밀 목록')).toBeNull();
});

it('goes to login after an anonymous persisted restore without exposing the old screen', async () => {
  await signedIn();
  const previous = screen.getByText('이전 비밀 목록');
  vi.spyOn(api, 'me').mockResolvedValue(undefined);
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
  vi.spyOn(api, 'me').mockResolvedValue({ ...me, id: 'user-b' });
  pageshow();
  await waitFor(() => {
    expect(replace).toHaveBeenCalledWith('/trash');
  });
  expect(reload).not.toHaveBeenCalled();
  expect(accountSafety.switching).toBe(true);
  expect(screen.getByText('이전 비밀 목록')).not.toBeVisible();
});

it('keeps a persisted app hidden if session verification fails', async () => {
  await signedIn();
  vi.spyOn(api, 'me').mockRejectedValue(new Error('offline'));
  pageshow();
  await screen.findByRole('alert');
  expect(screen.queryByText('이전 비밀 목록')).toBeNull();
});

it.each(['focus', 'visibilitychange'])(
  'hides a signed-out live session with a reauthentication link on %s',
  async (event) => {
    currentUser.userId = me.id;
    vi.spyOn(api, 'me').mockResolvedValue(undefined);
    render(FeedbackHost, { reload });
    if (event === 'focus') window.dispatchEvent(new Event(event));
    else {
      Object.defineProperty(document, 'visibilityState', { configurable: true, value: 'visible' });
      document.dispatchEvent(new Event(event));
    }
    await screen.findByRole('link', { name: '새 탭에서 로그인' });
    expect(replace).not.toHaveBeenCalled();
    expect(accountSafety.switching).toBe(true);
  },
);

// 코디네이터 SR 추가 지시: D-27의 짧은 값·저장 실패 입력은 살아 있는 화면의 메모리에 남긴다.
async function liveInput(): Promise<HTMLElement> {
  const original: unknown = Object.getOwnPropertyDescriptor(Storage.prototype, 'setItem')?.value;
  if (typeof original !== 'function') throw new Error('setItem missing');
  vi.spyOn(Storage.prototype, 'setItem').mockImplementation(function (this: Storage, key, value) {
    if (key.includes(':form-draft:')) throw new DOMException('quota', 'QuotaExceededError');
    Reflect.apply(original, this, [key, value]);
  });
  vi.spyOn(api, 'myNotes').mockResolvedValue([]);
  router.navigate('/notes');
  render(App, { replace, reload });
  const input = await screen.findByLabelText('새 노트 제목');
  await fireEvent.input(input, { target: { value: '메모리에만 남긴 입력' } });
  return input;
}

it('unhides the same live user without remounting screen-only input when draft storage fails', async () => {
  const input = await liveInput();
  signalSession(null);
  const check = deferred<Me | undefined>();
  vi.spyOn(api, 'me').mockReturnValueOnce(check.promise);
  window.dispatchEvent(new StorageEvent('storage', { key: sessionKey }));
  expect(input).not.toBeVisible();
  check.resolve(me);
  await waitFor(() => {
    expect(input).toBeVisible();
  });
  expect(screen.getByLabelText('새 노트 제목')).toBe(input);
  expect(input).toHaveValue('메모리에만 남긴 입력');
  expect(api.myNotes).toHaveBeenCalledTimes(1);
  const checkAgain = deferred<Me | undefined>();
  vi.spyOn(api, 'me').mockReturnValue(checkAgain.promise);
  window.dispatchEvent(new Event('focus'));
  expect(input).toBeVisible();
  checkAgain.resolve(me);
});

it('keeps input mounted through anonymous reauthentication and switches only after a different user', async () => {
  const input = await liveInput();
  signalSession(null);
  vi.spyOn(api, 'me')
    .mockResolvedValueOnce(undefined)
    .mockResolvedValue({ ...me, id: 'user-b' });
  window.dispatchEvent(new StorageEvent('storage', { key: sessionKey }));
  const signIn = await screen.findByRole('link', { name: '새 탭에서 로그인' });
  expect(signIn).toHaveAttribute('target', '_blank');
  expect(input).toHaveValue('메모리에만 남긴 입력');
  expect(input).not.toBeVisible();
  expect(replace).not.toHaveBeenCalled();
  reload.mockImplementationOnce(() => {
    expect(input).toHaveValue('메모리에만 남긴 입력');
    expect(input.isConnected).toBe(true);
    expect(input).not.toBeVisible();
  });
  signalSession('user-b');
  window.dispatchEvent(new StorageEvent('storage', { key: sessionKey }));
  await waitFor(() => {
    expect(reload).toHaveBeenCalledOnce();
  });
  expect(replace).not.toHaveBeenCalled();
});

it('resumes the mounted input after anonymous reauthentication as the same user', async () => {
  const input = await liveInput();
  signalSession(null);
  vi.spyOn(api, 'me').mockResolvedValueOnce(undefined).mockResolvedValue(me);
  window.dispatchEvent(new StorageEvent('storage', { key: sessionKey }));
  await screen.findByRole('link', { name: '새 탭에서 로그인' });
  signalSession(me.id);
  window.dispatchEvent(new StorageEvent('storage', { key: sessionKey }));
  await waitFor(() => {
    expect(input).toBeVisible();
  });
  expect(input).toHaveValue('메모리에만 남긴 입력');
  expect(reload).not.toHaveBeenCalled();
  expect(replace).not.toHaveBeenCalled();
});

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
