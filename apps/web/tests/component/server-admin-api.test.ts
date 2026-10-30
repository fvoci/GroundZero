import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { serverAdminApi } from '$lib/features/server-admin/api';
import { parseServerAdmin, parseServerOrganization } from '$lib/features/server-admin/model';
import { me } from '$lib/me.svelte';

beforeEach(() => {
  me.userId = 'user-a';
});
afterEach(() => {
  me.userId = '';
  vi.unstubAllGlobals();
});

it('uses the three existing server operations and guards creation with the expected user', async () => {
  const organization = {
    id: 'org',
    name: '조직',
    member_count: 50,
    workspace_count: 2,
    created_at: '2026-10-01T00:00:00Z',
  };
  const admin = { user_id: 'user-a', display_name: '나', created_at: organization.created_at };
  const fetch = vi
    .fn<typeof globalThis.fetch>()
    .mockResolvedValueOnce(new Response(JSON.stringify([organization])))
    .mockResolvedValueOnce(new Response(JSON.stringify([admin])))
    .mockResolvedValueOnce(new Response(JSON.stringify({ id: 'created' }), { status: 201 }));
  vi.stubGlobal('fetch', fetch);
  expect(await serverAdminApi.organizations()).toEqual([organization]);
  expect(await serverAdminApi.admins()).toEqual([admin]);
  const body = { request_id: crypto.randomUUID(), name: '새 조직', first_admin_user_id: 'user-a' };
  expect(await serverAdminApi.createOrganization(body)).toEqual({ id: 'created' });
  expect(fetch.mock.calls.map(([path, init]) => [init?.method, path])).toEqual([
    ['GET', '/api/server/organizations'],
    ['GET', '/api/server/admins'],
    ['POST', '/api/server/organizations'],
  ]);
  const init = fetch.mock.calls[2]?.[1];
  expect(init?.body).toBe(JSON.stringify(body));
  expect(new Headers(init?.headers).get('x-fvoci-expected-user')).toBe('user-a');
});

it('rejects malformed server metadata rather than rendering unchecked values', () => {
  expect(() =>
    parseServerOrganization({
      id: 'org',
      name: '조직',
      member_count: '50',
      workspace_count: 2,
      created_at: 'today',
    }),
  ).toThrow();
  expect(() => parseServerAdmin({ user_id: 'user', display_name: '나' })).toThrow();
});
