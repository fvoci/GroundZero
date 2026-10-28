import { afterEach, expect, it, vi } from 'vitest';

import { api } from '$lib/api/client';
import { request } from '$lib/shared/api';
import { accountSafety, hideSession } from '$lib/features/feedback/feedback.svelte';
import { me } from '$lib/me.svelte';

afterEach(() => {
  vi.restoreAllMocks();
  me.userId = '';
  hideSession('account');
  accountSafety.switching = false;
});

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
