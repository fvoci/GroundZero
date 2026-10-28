import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { api, ApiError, type DocumentView } from '$lib/api/client';
import { me } from '$lib/me.svelte';

const document: DocumentView = {
  id: 'd1',
  workspace_id: 'ws-1',
  placement: 'wiki',
  item_id: null,
  parent_id: null,
  reference: 'WIKI-1',
  title: '문서',
  body: { type: 'doc' },
  schema_version: 1,
  revision: 3,
  updated_at: '2026-10-19T00:00:00Z',
  access: 'write',
  can_edit: true,
  visibility: 'workspace',
  allow_copy: false,
  can_manage: false,
  can_delete: false,
  is_template: false,
  realtime: true,
  can_export: true,
};

beforeEach(() => {
  me.userId = 'user-a';
});
afterEach(() => {
  vi.unstubAllGlobals();
  me.userId = '';
});

function reply(status: number, body: object) {
  const fetch = vi.fn().mockResolvedValue(new Response(JSON.stringify(body), { status }));
  vi.stubGlobal('fetch', fetch);
  return fetch;
}

describe('document command contracts (#178, PR3 #189, D-96)', () => {
  it('patches only the supplied settings with the expected user', async () => {
    const fetch = reply(200, { ...document, allow_copy: true });
    await api.setSettings('d/1', { allow_copy: true });
    expect(fetch).toHaveBeenCalledWith(
      '/api/documents/d%2F1/settings',
      expect.objectContaining({ method: 'PATCH' }),
    );
    const init = fetch.mock.calls[0]?.[1] as RequestInit;
    expect(JSON.parse(String(init.body))).toEqual({ allow_copy: true });
    expect(new Headers(init.headers).get('x-fvoci-expected-user')).toBe('user-a');
  });

  it('keeps the full stale restore envelope and does not treat it as a body PUT conflict', async () => {
    const current = { ...document, title: '최신 제목', revision: 4 };
    reply(409, {
      error: 'conflict',
      message: 'stale',
      current,
      changed: ['body', 'title'],
      request_id: 'server-request',
    });
    await expect(
      api.restoreRevision('d1', 1, { requestId: 'restore', baseRevision: 3 }),
    ).resolves.toEqual({
      kind: 'conflict',
      current,
      changed: ['body', 'title'],
      requestId: 'server-request',
    });
  });

  it('leaves a room persistence failure as ErrorBody', async () => {
    reply(409, { error: 'invalid_state', message: 'room busy', request_id: 'room-request' });
    await expect(
      api.restoreRevision('d1', 1, { requestId: 'restore', baseRevision: 3 }),
    ).rejects.toMatchObject({
      status: 409,
      code: 'invalid_state',
      requestId: 'room-request',
    } satisfies Partial<ApiError>);
  });
});
