import type { Me } from '@fvoci/api-client';
import { cleanup, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, expect, it, vi } from 'vitest';

import { api } from '$lib/api/client';
import { request } from '$lib/shared/api';
import {
  accountSafety,
  accountSwitching,
  hideSession,
} from '$lib/features/feedback/feedback.svelte';
import { me } from '$lib/me.svelte';
import { router } from '$lib/router.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

import { TEST_ORGANIZATION } from './fixtures/organization';
import App from '../../src/app/App.svelte';

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  router.guard = undefined;
  router.navigate('/');
  me.userId = '';
  hideSession('account');
  accountSafety.switching = false;
});

const signedIn: Me = {
  id: 'user-a',
  display_name: '이전 사용자',
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

/** App도 실제 api.me/workspaces/request를 쓴다. HTTP 응답만 컴포넌트 fixture로 대신한다. */
async function appRequests() {
  await setLocale('ko', { reload: false });
  vi.stubGlobal('matchMedia', (query: string) => ({
    matches: false,
    media: query,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
  }));
  let now: Me | undefined = signedIn;
  let reads = 0;
  let hold = false;
  let finish!: (response: Response) => void;
  const fresh = new Promise<Response>((resolve) => {
    finish = resolve;
  });
  const json = (value: unknown, status = 200) => new Response(JSON.stringify(value), { status });
  vi.spyOn(globalThis, 'fetch').mockImplementation((input) => {
    const path = typeof input === 'string' ? input : input instanceof URL ? input.href : input.url;
    if (path === '/api/me')
      return Promise.resolve(now === undefined ? new Response('', { status: 401 }) : json(now));
    if (path === '/api/workspaces') {
      reads += 1;
      return hold ? fresh : Promise.resolve(json([]));
    }
    if (path === '/api/me/organizations') return Promise.resolve(json([TEST_ORGANIZATION]));
    if (path === '/api/me/timer') return Promise.resolve(json(null));
    return Promise.resolve(json([]));
  });
  const replace = vi.fn();
  const reload = vi.fn();
  router.navigate('/trash');
  render(App, { replace, reload });
  await screen.findByTestId('user-menu');
  return {
    replace,
    reload,
    setUser(value: Me | undefined) {
      now = value;
    },
    reads: () => reads,
    holdFresh() {
      hold = true;
    },
    finishFresh() {
      finish(json([]));
    },
  };
}

it('opens fresh reads only for the same user and keeps the screen hidden until they finish', async () => {
  const app = await appRequests();
  const previous = screen.getByTestId('user-menu');
  app.holdFresh();
  window.dispatchEvent(new PageTransitionEvent('pageshow', { persisted: true }));
  expect(previous).not.toBeVisible();
  await waitFor(() => {
    expect(app.reads()).toBe(2);
  });
  expect(accountSwitching()).toBe(true);
  await expect(request('PUT', '/api/me/settings', {})).rejects.toMatchObject({
    code: 'account_changed',
  });
  app.finishFresh();
  await waitFor(() => {
    expect(screen.getByTestId('user-menu')).toBeVisible();
  });
  expect(accountSwitching()).toBe(false);
  expect(app.replace).not.toHaveBeenCalled();
});

it.each(['other', 'anonymous'])(
  'never opens fresh reads for a restored %s session',
  async (kind) => {
    const app = await appRequests();
    app.setUser(kind === 'other' ? { ...signedIn, id: 'user-b' } : undefined);
    window.dispatchEvent(new PageTransitionEvent('pageshow', { persisted: true }));
    await waitFor(() => {
      expect(app.replace).toHaveBeenCalledWith('/trash');
    });
    expect(app.reads()).toBe(1);
    expect(accountSwitching()).toBe(true);
    await expect(request('GET', '/api/workspaces')).rejects.toMatchObject({
      code: 'account_changed',
    });
  },
);

// #212 1행·00:25: 먼저 가린 실제 api.logout도 서버에 도달하고 응답을 읽어야 한다.
it('allows only logout through the logout safety state, including its response', async () => {
  const fetch = vi
    .spyOn(globalThis, 'fetch')
    .mockResolvedValue(new Response(JSON.stringify({ redirect_to: '/signed-out' })));
  hideSession('logout');
  me.userId = '';
  await expect(api.logout()).resolves.toBe('/signed-out');
  expect(fetch).toHaveBeenCalledOnce();
  expect(accountSafety.switching).toBe(true);
  await expect(request('PUT', '/api/me/settings', {})).rejects.toMatchObject({
    code: 'account_changed',
  });
  await expect(request('GET', '/api/me')).rejects.toMatchObject({ code: 'account_changed' });
  expect(fetch).toHaveBeenCalledOnce();
});

it('allows only the session check while a restored session is unverified', async () => {
  const fetch = vi.spyOn(globalThis, 'fetch').mockResolvedValue(new Response('', { status: 401 }));
  hideSession('restore');
  await expect(api.me()).resolves.toBeUndefined();
  await expect(request('GET', '/api/workspaces')).rejects.toMatchObject({
    code: 'account_changed',
  });
  await expect(request('POST', '/auth/logout')).rejects.toMatchObject({ code: 'account_changed' });
  expect(fetch).toHaveBeenCalledOnce();
  expect(accountSafety.switching).toBe(true);
});

it('keeps every request blocked during an account switch', async () => {
  const fetch = vi.spyOn(globalThis, 'fetch');
  hideSession('account');
  await expect(api.logout()).rejects.toMatchObject({ code: 'account_changed' });
  await expect(api.me()).rejects.toMatchObject({ code: 'account_changed' });
  expect(fetch).not.toHaveBeenCalled();
});

it('does not widen either control request to a path prefix or a different method', async () => {
  const fetch = vi.spyOn(globalThis, 'fetch');
  hideSession('logout');
  await expect(request('POST', '/auth/logout/extra')).rejects.toMatchObject({
    code: 'account_changed',
  });
  await expect(request('GET', '/auth/logout')).rejects.toMatchObject({ code: 'account_changed' });
  hideSession('restore');
  await expect(request('GET', '/api/me/organizations')).rejects.toMatchObject({
    code: 'account_changed',
  });
  await expect(request('GET', '/api/me?extra=1')).rejects.toMatchObject({
    code: 'account_changed',
  });
  await expect(request('POST', '/api/me')).rejects.toMatchObject({ code: 'account_changed' });
  expect(fetch).not.toHaveBeenCalled();
});

it('still discards a late ordinary response after safety hiding begins', async () => {
  let finish!: (response: Response) => void;
  vi.spyOn(globalThis, 'fetch').mockReturnValue(
    new Promise<Response>((resolve) => {
      finish = resolve;
    }),
  );
  accountSafety.switching = false;
  const pending = request('GET', '/api/workspaces');
  hideSession('restore');
  finish(new Response('[]'));
  await expect(pending).rejects.toMatchObject({ code: 'account_changed' });
});

it('discards an old response even after reads reopen for the verified user', async () => {
  let finish!: (response: Response) => void;
  vi.spyOn(globalThis, 'fetch').mockReturnValue(
    new Promise<Response>((resolve) => {
      finish = resolve;
    }),
  );
  accountSafety.switching = false;
  const pending = request('GET', '/api/workspaces');
  hideSession('restore');
  accountSafety.switching = false;
  finish(new Response('[]'));
  await expect(pending).rejects.toMatchObject({ code: 'account_changed' });
});

it('allows only exact identity verification during live reauthentication safety', async () => {
  const send = vi.spyOn(globalThis, 'fetch').mockResolvedValue(new Response('{}'));
  hideSession('reauth');
  await expect(request('GET', '/api/me')).resolves.toMatchObject({ status: 200 });
  for (const [method, path] of [
    ['GET', '/api/workspaces'],
    ['GET', '/api/me?extra=1'],
    ['POST', '/api/me'],
    ['POST', '/auth/logout'],
  ]) {
    if (method === undefined || path === undefined) throw new Error('missing request fixture');
    await expect(request(method, path)).rejects.toMatchObject({ code: 'account_changed' });
  }
  expect(send).toHaveBeenCalledOnce();
});
