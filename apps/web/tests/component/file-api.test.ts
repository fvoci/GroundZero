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

interface SeenRequest {
  url: string;
  init: RequestInit;
}
function capture(reply: (url: string, init: RequestInit) => Response): SeenRequest[] {
  const seen: SeenRequest[] = [];
  vi.stubGlobal('fetch', (input: RequestInfo | URL, init?: RequestInit) => {
    const url = typeof input === 'string' ? input : input instanceof URL ? input.href : input.url;
    const options = init ?? {};
    seen.push({ url, init: options });
    return Promise.resolve(reply(url, options));
  });
  return seen;
}
function json(value: unknown): Response {
  return new Response(JSON.stringify(value), {
    status: 200,
    headers: { 'Content-Type': 'application/json' },
  });
}

describe('file resource requests', () => {
  it('reads metadata and the version collection independently', async () => {
    const seen = capture((url) => json(url.endsWith('/versions') ? [] : file));
    expect(await api.file('f1')).toEqual(file);
    expect(await api.fileVersions('f1')).toEqual([]);
    expect(seen.map((call) => call.url)).toEqual(['/api/files/f1', '/api/files/f1/versions']);
  });

  it('renames with PATCH and carries the expected user', async () => {
    me.userId = 'editor';
    const seen = capture(() => json(file));
    await api.renameFile('f1', 'own.txt');
    const call = seen[0];
    if (call === undefined) throw new Error('missing rename request');
    expect(call.url).toBe('/api/files/f1');
    expect(call.init.method).toBe('PATCH');
    const body = call.init.body;
    if (typeof body !== 'string') throw new Error('expected JSON request body');
    const sent: unknown = JSON.parse(body);
    expect(sent).toEqual({ name: 'own.txt' });
    expect(new Headers(call.init.headers).get('x-fvoci-expected-user')).toBe('editor');
  });

  it('keeps the uploading user on parts, completion and abort after the visible account changes', async () => {
    me.userId = 'new-account';
    const seen = capture((url) =>
      url.endsWith('/complete') ? json(upload) : new Response(null, { status: 204 }),
    );
    await api.putUploadPart('up1', 1, new Blob(['abc']), '0'.repeat(64), 'original-uploader');
    await api.completeUpload('up1', 'original-uploader');
    await api.abortUpload('up1', 'original-uploader');
    expect(seen).toHaveLength(3);
    expect(seen.map((call) => new Headers(call.init.headers).get('x-fvoci-expected-user'))).toEqual(
      ['original-uploader', 'original-uploader', 'original-uploader'],
    );
  });
});
