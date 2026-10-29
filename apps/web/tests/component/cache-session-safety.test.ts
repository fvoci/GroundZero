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

function returnEvent(event: string): void {
  if (event === 'visibilitychange') {
    Object.defineProperty(document, 'visibilityState', { configurable: true, value: 'visible' });
    document.dispatchEvent(new Event(event));
  } else if (event === 'pageshow') pageshow(false);
  else window.dispatchEvent(new Event(event));
}

beforeEach(async () => {
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

// SR-221-10: A 응답 생성 → 서버 세션 변경 → 복귀 이벤트 → 늦은 A 수신 → 후속 자기 응답.
it.each(
  (['initial', 'live', 'persisted'] as const).flatMap((phase) =>
    (
      ['focus', 'visibilitychange', 'pageshow', 'persisted-pageshow', 'storage', 'mixed'] as const
    ).flatMap((event) =>
      (['anonymous', 'other'] as const).map((identity) => ({ phase, event, identity })),
    ),
  ),
)(
  'SR-221-10 queues one follow-up for $event during $phase and accepts its own $identity result',
  async ({ phase, event, identity }) => {
    if (phase !== 'initial') await signedIn();
    const first = deferred<Me | undefined>();
    const next = deferred<Me | undefined>();
    const read = vi
      .spyOn(api, 'me')
      .mockReset()
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(next.promise);
    if (phase === 'initial') render(App, { replace, reload });
    else if (phase === 'persisted') pageshow();
    else window.dispatchEvent(new Event('focus'));
    expect(read).toHaveBeenCalledOnce();
    for (let count = 0; count < 3; count += 1) {
      if (event === 'storage') {
        window.dispatchEvent(new StorageEvent('storage', { key: 'fvoci:auth-session' }));
      } else if (event === 'visibilitychange') {
        Object.defineProperty(document, 'visibilityState', {
          configurable: true,
          value: 'visible',
        });
        document.dispatchEvent(new Event(event));
      } else if (event === 'persisted-pageshow') pageshow();
      else if (event === 'mixed') {
        returnEvent('focus');
        returnEvent('visibilitychange');
        pageshow(false);
      } else if (event === 'pageshow') pageshow(false);
      else window.dispatchEvent(new Event(event));
    }
    expect(read).toHaveBeenCalledOnce();
    first.resolve(me);
    await waitFor(() => {
      expect(read).toHaveBeenCalledTimes(2);
    });
    if (event === 'persisted-pageshow') {
      const previous = screen.queryByTestId('user-menu');
      if (previous !== null) expect(previous).not.toBeVisible();
    } else
      await waitFor(() => {
        expect(screen.getByTestId('user-menu')).toBeVisible();
      });
    next.resolve(identity === 'anonymous' ? undefined : { ...me, id: 'user-b' });
    if (event === 'persisted-pageshow')
      await waitFor(() => {
        expect(replace).toHaveBeenCalled();
      });
    else if (identity === 'anonymous')
      await screen.findByRole('link', { name: '새 탭에서 로그인' });
    else
      await waitFor(() => {
        expect(reload).toHaveBeenCalledOnce();
      });
    if (event === 'persisted-pageshow' && identity === 'anonymous')
      expect(screen.queryByText('이전 비밀 목록')).toBeNull();
    else {
      const previous = screen.queryByText('이전 비밀 목록');
      if (previous !== null) expect(previous).not.toBeVisible();
    }
    await tick();
    expect(read).toHaveBeenCalledTimes(2);
  },
);

it.each([
  ['return', 'other'],
  ['live', 'other'],
  ['persisted', 'other'],
  ['return', 'anonymous'],
  ['live', 'anonymous'],
  ['persisted', 'anonymous'],
] as const)('uses its own identity during %s verification (%s)', async (mode, identity) => {
  const input = await liveInput();
  const own = deferred<Me | undefined>();
  const ignored = deferred<Me | undefined>();
  const read = vi
    .spyOn(api, 'me')
    .mockReturnValueOnce(own.promise)
    .mockReturnValue(ignored.promise);
  const before = read.mock.calls.length;
  if (mode === 'return') returnEvent('visibilitychange');
  else if (mode === 'persisted') pageshow();
  else window.dispatchEvent(new Event('focus'));

  own.resolve(identity === 'anonymous' ? undefined : { ...me, id: 'user-b' });
  if (mode === 'persisted')
    await waitFor(() => {
      expect(replace).toHaveBeenCalled();
    });
  else if (identity === 'other')
    await waitFor(() => {
      expect(reload).toHaveBeenCalledOnce();
    });
  else await screen.findByRole('link', { name: '새 탭에서 로그인' });
  expect(read).toHaveBeenCalledTimes(before + 1);
  expect(input).not.toBeVisible();
  ignored.resolve(undefined);
});

it.each(['live', 'persisted'])(
  'bounds automatic safety-generation verification to three requests during %s and preserves input on failure',
  async (mode) => {
    const input = await liveInput();
    const read = vi.spyOn(api, 'me').mockImplementation(() => {
      accountSafety.generation += 1;
      return Promise.resolve(me);
    });
    const before = read.mock.calls.length;
    if (mode === 'persisted') pageshow();
    else {
      window.dispatchEvent(new Event('focus'));
    }
    await screen.findByRole('alert');
    expect(read).toHaveBeenCalledTimes(before + 3);
    expect(input.isConnected).toBe(true);
    expect(input).not.toBeVisible();
    expect(input).toHaveValue('메모리에만 남긴 입력');
    read.mockResolvedValue(me);
    await fireEvent.click(screen.getByRole('button', { name: '다시 시도' }));
    await waitFor(() => {
      expect(screen.getByLabelText('새 노트 제목')).toBeVisible();
    });
    expect(read).toHaveBeenCalledTimes(before + 4);
  },
);

// 자기 확인 실패 뒤 입력과 직접 재시도를 보존한다.
it.each(['focus', 'visibilitychange', 'pageshow'])(
  'preserves input and retry after its own check fails on %s',
  async (kind) => {
    const input = await liveInput();
    const check = deferred<Me | undefined>();
    const read = vi.spyOn(api, 'me').mockReturnValueOnce(check.promise);
    const reads = read.mock.calls.length;
    returnEvent(kind);
    expect(read).toHaveBeenCalledTimes(reads + 1);
    expect(input).toBeVisible();
    check.reject(new Error('verification failed'));
    await screen.findByRole('alert');
    expect(input).not.toBeVisible();
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

it('keeps restored input mounted when verification fails', async () => {
  const input = await liveInput();

  const check = deferred<Me | undefined>();
  vi.spyOn(api, 'me').mockReturnValueOnce(check.promise);
  pageshow();
  check.reject(new Error('offline'));
  await screen.findByRole('alert');
  expect(input.isConnected).toBe(true);
  expect(input).toHaveValue('메모리에만 남긴 입력');
  expect(input).not.toBeVisible();
  expect(screen.getByRole('button', { name: '다시 시도' })).toBeVisible();
});

it.each(['focus', 'visibilitychange', 'pageshow'])(
  'requests its own identity at %s and preserves the last verified screen',
  async (event) => {
    await signedIn();
    const check = deferred<Me | undefined>();
    vi.spyOn(api, 'me').mockReturnValueOnce(check.promise);

    if (event === 'visibilitychange') {
      Object.defineProperty(document, 'visibilityState', { configurable: true, value: 'visible' });
      document.dispatchEvent(new Event(event));
    } else if (event === 'pageshow') pageshow(false);
    else window.dispatchEvent(new Event(event));
    expect(screen.getByText('이전 비밀 목록')).toBeVisible();
    expect(accountSafety.switching).toBe(false);
    expect(api.workspaces).toHaveBeenCalledTimes(1);
    check.resolve(me);
    await waitFor(() => expect(screen.getByTestId('user-menu')).toBeVisible());
    expect(screen.getByText('이전 비밀 목록')).toBeVisible();
    expect(api.workspaces).toHaveBeenCalledTimes(1);
    expect(api.deletedWorkspaces).toHaveBeenCalledTimes(1);
  },
);

it('offers retry after its own return verification fails', async () => {
  await signedIn();

  vi.spyOn(api, 'me').mockRejectedValueOnce(new Error('offline')).mockResolvedValue(undefined);
  window.dispatchEvent(new Event('focus'));
  expect(screen.getByText('이전 비밀 목록')).toBeVisible();
  await screen.findByRole('alert');
  expect(replace).not.toHaveBeenCalled();
  await fireEvent.click(screen.getByRole('button', { name: '다시 시도' }));
  await screen.findByRole('link', { name: '새 탭에서 로그인' });
  expect(replace).not.toHaveBeenCalled();
  expect(currentUser.userId).toBe(me.id);
  expect(screen.getByText('이전 비밀 목록')).not.toBeVisible();
});

it.each(['focus', 'visibilitychange', 'pageshow'])(
  'preserves the same user screen and inputs at %s even when the network fails',
  async (event) => {
    await signedIn();

    vi.spyOn(api, 'me').mockRejectedValue(new Error('offline'));
    if (event === 'visibilitychange') document.dispatchEvent(new Event(event));
    else if (event === 'pageshow') pageshow(false);
    else window.dispatchEvent(new Event(event));
    expect(screen.getByText('이전 비밀 목록')).toBeVisible();
    expect(accountSafety.switching).toBe(false);
    expect(replace).not.toHaveBeenCalled();
  },
);

it('queues return events during verification and unsubscribes on unmount', async () => {
  const view = render(App, { replace, reload });
  await screen.findByText('이전 비밀 목록');
  const check = deferred<Me | undefined>();
  const read = vi.spyOn(api, 'me').mockReturnValue(check.promise);

  window.dispatchEvent(new Event('focus'));
  document.dispatchEvent(new Event('visibilitychange'));
  pageshow(false);
  expect(read).toHaveBeenCalledTimes(2);
  view.unmount();

  window.dispatchEvent(new Event('focus'));
  window.dispatchEvent(new Event('focus'));
  document.dispatchEvent(new Event('visibilitychange'));
  pageshow(false);
  expect(read).toHaveBeenCalledTimes(2);
  check.resolve(undefined);
  await Promise.resolve();
  expect(replace).not.toHaveBeenCalled();
});

it('keeps authentication usable when browser storage is denied', async () => {
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
  expect(screen.getByText('이전 비밀 목록')).not.toBeVisible();
  expect(screen.getByRole('button', { name: '다시 시도' })).toBeVisible();
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

it('keeps the same live user mounted while verifying when draft storage fails', async () => {
  const input = await liveInput();

  const check = deferred<Me | undefined>();
  vi.spyOn(api, 'me').mockReturnValueOnce(check.promise);
  window.dispatchEvent(new Event('focus'));
  expect(input).toBeVisible();
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

  vi.spyOn(api, 'me')
    .mockResolvedValueOnce(undefined)
    .mockResolvedValue({ ...me, id: 'user-b' });
  window.dispatchEvent(new Event('focus'));
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

  window.dispatchEvent(new Event('focus'));
  await waitFor(() => {
    expect(reload).toHaveBeenCalledOnce();
  });
  expect(replace).not.toHaveBeenCalled();
});

it('resumes the mounted input after anonymous reauthentication as the same user', async () => {
  const input = await liveInput();

  vi.spyOn(api, 'me').mockResolvedValueOnce(undefined).mockResolvedValue(me);
  window.dispatchEvent(new Event('focus'));
  await screen.findByRole('link', { name: '새 탭에서 로그인' });

  window.dispatchEvent(new Event('focus'));
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
