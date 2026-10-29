import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { ApiError, type DocumentView } from '$lib/api/client';
import { documentsApi as api } from '$lib/features/documents/api';
import { me } from '$lib/me.svelte';
import { rec } from '$lib/shared/api/parse';

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
  const fetch = vi
    .fn<(_url: string, init?: RequestInit) => Promise<Response>>()
    .mockResolvedValue(new Response(JSON.stringify(body), { status }));
  vi.stubGlobal('fetch', fetch);
  return fetch;
}

function sentBody(init?: RequestInit): Record<string, unknown> {
  if (typeof init?.body !== 'string') throw new Error('missing JSON request body');
  return rec(JSON.parse(init.body), 'request');
}

describe('document command contracts (#178, PR3 #189, D-96)', () => {
  it('keeps stale title metadata and sends no body or If-Match', async () => {
    const current = { ...document, title: '최신 제목', revision: 4 };
    const fetch = reply(409, {
      error: 'conflict',
      message: 'stale',
      current,
      changed: ['title'],
      request_id: 'title-request',
    });
    await expect(
      api.patchTitle('d1', {
        expectedUser: 'editing-user',
        requestId: 'title',
        baseRevision: 3,
        title: '내 제목',
      }),
    ).resolves.toEqual({
      kind: 'conflict',
      current,
      changed: ['title'],
      requestId: 'title-request',
    });
    const init = fetch.mock.calls[0]?.[1];
    expect(sentBody(init)).toEqual({
      request_id: 'title',
      base_revision: 3,
      title: '내 제목',
    });
    expect(new Headers(init?.headers).get('x-fvoci-expected-user')).toBe('editing-user');
    expect(new Headers(init?.headers).has('If-Match')).toBe(false);
  });

  it('preserves structured title field validation errors', async () => {
    reply(400, {
      error: 'invalid',
      message: 'too long',
      reason: 'too_long',
      field: 'title',
      limit: 200,
      request_id: 'validation',
    });
    await expect(
      api.patchTitle('d1', {
        expectedUser: 'user-a',
        requestId: 'title',
        baseRevision: 3,
        title: 'x'.repeat(201),
      }),
    ).rejects.toMatchObject({
      status: 400,
      reason: 'too_long',
      field: 'title',
      limit: 200,
      requestId: 'validation',
    });
  });

  it('patches only the supplied settings with the expected user', async () => {
    const fetch = reply(200, { ...document, allow_copy: true });
    await expect(
      api.setSettings('d/1', { allow_copy: true, previous_allow_copy: false }),
    ).resolves.toEqual({ kind: 'saved', document: { ...document, allow_copy: true } });
    expect(fetch).toHaveBeenCalledWith(
      '/api/documents/d%2F1/settings',
      expect.objectContaining({ method: 'PATCH' }),
    );
    const init = fetch.mock.calls[0]?.[1];
    expect(sentBody(init)).toEqual({ allow_copy: true, previous_allow_copy: false });
    expect(new Headers(init?.headers).get('x-fvoci-expected-user')).toBe('user-a');
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
