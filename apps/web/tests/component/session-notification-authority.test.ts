import type { Me } from '@fvoci/api-client';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { tick } from 'svelte';

import { api } from '$lib/api/client';
import { activityApi } from '$lib/features/activity/api';
import {
  accountSafety,
  auth,
  connection,
  hideSession,
} from '$lib/features/feedback/feedback.svelte';
import { me as currentUser } from '$lib/me.svelte';
import { org } from '$lib/org.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';
import { router } from '$lib/router.svelte';

import App from '../../src/app/App.svelte';
import FeedbackHost from '$lib/features/feedback/FeedbackHost.svelte';
import { TEST_ORGANIZATION } from './fixtures/organization';

const identity: Me = {
  id: 'user-a',
  display_name: 'A',
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
const key = 'fvoci:auth-session';
const replace = vi.fn();
const reload = vi.fn();
let originalLocks: PropertyDescriptor | undefined;
let notification = 0;

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

function hint(kind: string): void {
  notification += 1;
  if (kind === 'missing') localStorage.removeItem(key);
  else if (kind === 'malformed') localStorage.setItem(key, '{broken');
  else
    localStorage.setItem(
      key,
      JSON.stringify({
        state: kind === 'pending' || kind === 'expired' ? 'logout-pending' : 'settled',
        userId: kind === 'anonymous' ? null : kind === 'other' ? 'user-b' : identity.id,
        generation: notification,
        sequence: String(notification),
        attemptId: String(notification),
        startedAt: Date.now() - (kind === 'expired' ? 60_000 : 0),
      }),
    );
}

beforeEach(async () => {
  localStorage.clear();
  originalLocks = Object.getOwnPropertyDescriptor(navigator, 'locks');
  Object.defineProperty(navigator, 'locks', {
    configurable: true,
    value: { request: (_name: string, update: () => unknown) => Promise.resolve(update()) },
  });
  await setLocale('ko', { reload: false });
  vi.stubGlobal('matchMedia', (query: string) => ({
    matches: false,
    media: query,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
  }));
  vi.spyOn(globalThis, 'fetch').mockResolvedValue(new Response('{}', { status: 404 }));
  vi.spyOn(api, 'me').mockResolvedValue(identity);
  vi.spyOn(api, 'authInfo').mockResolvedValue({ oidc_enabled: true });
  vi.spyOn(api, 'workspaces').mockResolvedValue([]);
  vi.spyOn(api, 'myOrganizations').mockResolvedValue([TEST_ORGANIZATION]);
  vi.spyOn(api, 'myNotes').mockResolvedValue([]);
  vi.spyOn(api, 'currentTimer').mockResolvedValue(null);
  vi.spyOn(activityApi, 'unreadCount').mockResolvedValue(0);
  currentUser.userId = '';
  auth.expired = false;
  replace.mockClear();
  reload.mockClear();
  router.guard = undefined;
  router.navigate('/notes');
});

afterEach(() => {
  cleanup();
  hideSession('account');
  accountSafety.switching = false;
  currentUser.userId = '';
  auth.expired = false;
  connection.lost = false;
  router.guard = undefined;
  router.navigate('/');
  org.reset();
  localStorage.clear();
  if (originalLocks === undefined) Reflect.deleteProperty(navigator, 'locks');
  else Object.defineProperty(navigator, 'locks', originalLocks);
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

async function signedIn(): Promise<HTMLElement> {
  render(App, { replace, reload });
  const input = await screen.findByLabelText('새 노트 제목');
  await fireEvent.input(input, { target: { value: '이 탭의 입력' } });
  return input;
}

it.each(['same', 'other', 'anonymous', 'pending', 'expired', 'missing', 'malformed'])(
  'a %s notification only requests verification and never masks the last verified live input',
  async (kind) => {
    const input = await signedIn();
    const own = deferred<Me | undefined>();
    const read = vi.spyOn(api, 'me').mockReturnValueOnce(own.promise).mockResolvedValue(identity);
    const before = read.mock.calls.length;
    hint(kind);
    window.dispatchEvent(new StorageEvent('storage', { key }));
    expect(read).toHaveBeenCalledTimes(before + 1);
    expect(input).toBeVisible();
    own.resolve(identity);
    await own.promise;
    await tick();
    await waitFor(() => {
      expect(input).toBeVisible();
    });
    expect(input).toHaveValue('이 탭의 입력');
    expect(screen.getByLabelText('새 노트 제목')).toBe(input);
    expect(api.myNotes).toHaveBeenCalledTimes(1);
  },
);

it.each(['initial', 'live', 'persisted'])(
  'notification churn cannot reject a same-user response or create a retry error in %s verification',
  async (mode) => {
    if (mode !== 'initial') await signedIn();
    const own = deferred<Me | undefined>();
    vi.spyOn(api, 'me').mockReturnValueOnce(own.promise).mockResolvedValue(identity);
    if (mode === 'initial') render(App, { replace, reload });
    else if (mode === 'persisted')
      window.dispatchEvent(new PageTransitionEvent('pageshow', { persisted: true }));
    else {
      hint('other');
      window.dispatchEvent(new StorageEvent('storage', { key }));
    }
    hint('pending');
    own.resolve(identity);
    await own.promise;
    await tick();
    await waitFor(() => {
      expect(screen.getByLabelText('새 노트 제목')).toBeVisible();
    });
    expect(screen.queryByRole('alert')).toBeNull();
    expect(replace).not.toHaveBeenCalled();
    expect(reload).not.toHaveBeenCalled();
  },
);

it.each(['initial', 'live', 'persisted'])(
  'a never-granted publication lock cannot block its own verified screen during %s verification',
  async (mode) => {
    if (mode !== 'initial') await signedIn();
    Object.defineProperty(navigator, 'locks', {
      configurable: true,
      value: { request: () => new Promise<never>(() => undefined) },
    });
    if (mode === 'initial') render(App, { replace, reload });
    else if (mode === 'persisted')
      window.dispatchEvent(new PageTransitionEvent('pageshow', { persisted: true }));
    else {
      hint('other');
      window.dispatchEvent(new StorageEvent('storage', { key }));
    }
    await waitFor(() => {
      expect(screen.getByLabelText('새 노트 제목')).toBeVisible();
    });
  },
);

it.each(['success', 'failure'])(
  'a never-granted notification lock cannot block local logout %s',
  async (outcome) => {
    await signedIn();
    Object.defineProperty(navigator, 'locks', {
      configurable: true,
      value: { request: () => new Promise<never>(() => undefined) },
    });
    const send = vi.spyOn(api, 'logout');
    if (outcome === 'success') send.mockResolvedValue('/signed-out');
    else send.mockRejectedValue(new Error('offline'));
    await fireEvent.click(screen.getByTestId('user-menu'));
    await fireEvent.click(await screen.findByRole('menuitem', { name: '로그아웃' }));
    await waitFor(() => {
      expect(send).toHaveBeenCalledOnce();
    });
    if (outcome === 'success')
      await waitFor(() => {
        expect(replace).toHaveBeenCalledWith('/signed-out');
      });
    else await screen.findByRole('button', { name: '로그아웃 다시 하기' });
    expect(screen.queryByLabelText('새 노트 제목')).toBeNull();
  },
);

it.each([
  ['live', 'anonymous'],
  ['live', 'other'],
  ['persisted', 'anonymous'],
  ['persisted', 'other'],
])('a pending publication lock cannot veto its own %s %s response', async (mode, kind) => {
  const input = await signedIn();
  Object.defineProperty(navigator, 'locks', {
    configurable: true,
    value: { request: () => new Promise<never>(() => undefined) },
  });
  vi.spyOn(api, 'me').mockResolvedValue(
    kind === 'anonymous' ? undefined : { ...identity, id: 'user-b' },
  );
  if (mode === 'persisted')
    window.dispatchEvent(new PageTransitionEvent('pageshow', { persisted: true }));
  else {
    hint('same');
    window.dispatchEvent(new StorageEvent('storage', { key }));
  }
  if (mode === 'persisted')
    await waitFor(() => {
      expect(replace).toHaveBeenCalledWith('/notes');
    });
  else if (kind === 'other')
    await waitFor(() => {
      expect(reload).toHaveBeenCalledOnce();
    });
  else await screen.findByRole('link', { name: '새 탭에서 로그인' });
  expect(input).not.toBeVisible();
});

it('coalesces in-flight notifications into one follow-up without withholding either own response', async () => {
  const input = await signedIn();
  const first = deferred<Me | undefined>();
  const next = deferred<Me | undefined>();
  const read = vi
    .spyOn(api, 'me')
    .mockReturnValueOnce(first.promise)
    .mockReturnValueOnce(next.promise);
  const before = read.mock.calls.length;
  hint('other');
  window.dispatchEvent(new StorageEvent('storage', { key }));
  for (const kind of ['pending', 'anonymous', 'same']) {
    hint(kind);
    window.dispatchEvent(new StorageEvent('storage', { key }));
  }
  expect(input).toBeVisible();
  expect(read).toHaveBeenCalledTimes(before + 1);
  first.resolve(identity);
  await waitFor(() => {
    expect(read).toHaveBeenCalledTimes(before + 2);
  });
  expect(input).toBeVisible();
  next.resolve(undefined);
  await screen.findByRole('link', { name: '새 탭에서 로그인' });
  expect(input).not.toBeVisible();
  expect(input).toHaveValue('이 탭의 입력');
});

it.each(['same', 'other', 'anonymous'])(
  'standalone feedback uses its own %s response without waiting for publication',
  async (kind) => {
    currentUser.userId = identity.id;
    hideSession('reauth');
    Object.defineProperty(navigator, 'locks', {
      configurable: true,
      value: { request: () => new Promise<never>(() => undefined) },
    });
    const read = vi
      .spyOn(api, 'me')
      .mockResolvedValue(
        kind === 'anonymous'
          ? undefined
          : kind === 'other'
            ? { ...identity, id: 'user-b' }
            : identity,
      );
    // 위임은 요청 시작 때만 한다. 응답 처리 도중 새 표식으로 수락을 취소할 수 없다.
    const delegate = vi.fn().mockReturnValueOnce(false).mockReturnValue(true);
    render(FeedbackHost, { reload, syncSessionSignal: delegate });
    window.dispatchEvent(new Event('focus'));
    if (kind === 'same') {
      await waitFor(() => {
        expect(accountSafety.switching || accountSafety.restoring).toBe(false);
      });
    } else if (kind === 'other')
      await waitFor(() => {
        expect(reload).toHaveBeenCalledOnce();
      });
    else await screen.findByRole('link', { name: '새 탭에서 로그인' });
    expect(delegate).toHaveBeenCalledOnce();
    expect(read).toHaveBeenCalledOnce();
  },
);
