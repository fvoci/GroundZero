import { afterEach, describe, expect, it, vi } from 'vitest';
import { api } from '$lib/api/client';
import { me } from '$lib/me.svelte';

const file = {
  id: 'f1',
  name: 'own.txt',
  document_id: 'd1',
  item_id: null,
  current_version: 1,
  size: 3,
  content_type: 'text/plain',
  updated_at: '2026-10-28T00:00:00Z',
  can_write: true,
  can_download: true,
};
const upload = {
  id: 'up1',
  state: 'done',
  name: 'own.txt',
  size: 3,
  chunk_bytes: 5,
  parts_total: 1,
  received: [1],
  expires_at: '2026-10-29T00:00:00Z',
  file_id: 'f1',
  version: 1,
  failure: null,
};
afterEach(() => {
  vi.unstubAllGlobals();
  me.userId = '';
});

describe('file resource requests', () => {
  it('reads metadata and the version collection independently', async () => {
    const fetch = vi.fn((_input: RequestInfo | URL, _init?: RequestInit) =>
      Promise.resolve(
        new Response(JSON.stringify(String(_input).endsWith('/versions') ? [] : file), {
          status: 200,
          headers: { 'Content-Type': 'application/json' },
        }),
      ),
    );
    vi.stubGlobal('fetch', fetch);
    expect(await api.file('f1')).toEqual(file);
    expect(await api.fileVersions('f1')).toEqual([]);
    expect(fetch.mock.calls.map(([url]) => url)).toEqual([
      '/api/files/f1',
      '/api/files/f1/versions',
    ]);
  });

  it('renames with PATCH and carries the expected user', async () => {
    me.userId = 'editor';
    const fetch = vi.fn(() =>
      Promise.resolve(
        new Response(JSON.stringify(file), {
          status: 200,
          headers: { 'Content-Type': 'application/json' },
        }),
      ),
    );
    vi.stubGlobal('fetch', fetch);
    await api.renameFile('f1', 'own.txt');
    const call = fetch.mock.calls[0] as unknown as [string, RequestInit];
    expect(call[0]).toBe('/api/files/f1');
    expect(call[1].method).toBe('PATCH');
    expect(JSON.parse(String(call[1].body))).toEqual({ name: 'own.txt' });
    expect(new Headers(call[1].headers).get('x-fvoci-expected-user')).toBe('editor');
  });

  it('keeps the uploading user on parts, completion and abort after the visible account changes', async () => {
    me.userId = 'new-account';
    const seen: RequestInit[] = [];
    vi.stubGlobal('fetch', (_input: RequestInfo | URL, init: RequestInit) => {
      seen.push(init);
      return Promise.resolve(
        init.method === 'POST'
          ? new Response(JSON.stringify(upload), {
              status: 200,
              headers: { 'Content-Type': 'application/json' },
            })
          : new Response(null, { status: 204 }),
      );
    });
    await api.putUploadPart('up1', 1, new Blob(['abc']), '0'.repeat(64), 'original-uploader');
    await api.completeUpload('up1', 'original-uploader');
    await api.abortUpload('up1', 'original-uploader');
    expect(seen).toHaveLength(3);
    expect(seen.map((init) => new Headers(init.headers).get('x-fvoci-expected-user'))).toEqual([
      'original-uploader',
      'original-uploader',
      'original-uploader',
    ]);
  });
});
