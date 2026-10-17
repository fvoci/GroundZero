import { afterEach, describe, expect, it, vi } from 'vitest';

import type { DocNode } from '$lib/api/client';
import { api, isStrongEtag } from '$lib/api/client';

const id = '0198f1f2-0000-7000-8000-000000000001';
const body: DocNode = { type: 'doc', content: [] };

function doc(revision: number): object {
  return {
    id,
    workspace_id: '0198f1f2-0000-7000-8000-000000000002',
    reference: 'WIKI-1',
    title: '회의록',
    body,
    schema_version: 1,
    revision,
    updated_at: '2026-10-17T00:00:00Z',
    placement: 'wiki',
    access: 'write',
    can_edit: true,
    visibility: 'workspace',
    allow_copy: false,
    can_manage: false,
    can_delete: false,
    is_template: false,
    realtime: false,
    can_export: true,
  };
}

interface Seen {
  method: string;
  ifMatch: string | null;
}

/** 서버 대역: 응답과 함께 받은 요청의 메서드·If-Match를 기록한다. */
function stub(status: number, json: object, etag?: string): Seen[] {
  const seen: Seen[] = [];
  vi.stubGlobal('fetch', (_input: RequestInfo | URL, init?: RequestInit) => {
    const headers = new Headers(init?.headers);
    seen.push({ method: init?.method ?? 'GET', ifMatch: headers.get('If-Match') });
    const out = new Headers({ 'Content-Type': 'application/json' });
    if (etag !== undefined) out.set('ETag', etag);
    return Promise.resolve(new Response(JSON.stringify(json), { status, headers: out }));
  });
  return seen;
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('document ETag and If-Match (D-77)', () => {
  it('tells strong validators from weak ones', () => {
    expect(isStrongEtag('"abc"')).toBe(true);
    expect(isStrongEtag('W/"abc"')).toBe(false);
    expect(isStrongEtag('abc')).toBe(false);
    expect(isStrongEtag(undefined)).toBe(false);
  });

  it('keeps the ETag of a single document read', async () => {
    stub(200, doc(2), '"e2"');
    const d = await api.document(id);
    expect(d.etag).toBe('"e2"');
  });

  it('sends If-Match only for a strong ETag and keeps the new one after saving', async () => {
    const input = { requestId: 'r', baseRevision: 2, title: '회의록', body };
    let seen = stub(200, doc(3), '"e3"');
    const saved = await api.saveDocument(id, { ...input, etag: '"e2"' });
    expect(seen[0]).toEqual({ method: 'PUT', ifMatch: '"e2"' });
    expect(saved.kind === 'saved' && saved.document.etag).toBe('"e3"');

    // CDN이 약하게 바꾼 값이나 없는 값은 보내지 않는다(기준 리비전만으로 판단).
    seen = stub(200, doc(3), '"e3"');
    await api.saveDocument(id, { ...input, etag: 'W/"e2"' });
    await api.saveDocument(id, input);
    expect(seen.map((s) => s.ifMatch)).toEqual([null, null]);
  });

  it('treats 412 like a conflict and carries the current ETag', async () => {
    stub(412, { error: 'precondition_failed', current: doc(3), etag: '"e3"' });
    const r = await api.saveDocument(id, {
      requestId: 'r',
      baseRevision: 2,
      title: '회의록',
      body,
      etag: '"e2"',
    });
    expect(r.kind).toBe('conflict');
    if (r.kind === 'conflict') {
      expect(r.current.revision).toBe(3);
      expect(r.current.etag).toBe('"e3"');
    }
  });

  it('keeps the 409 conflict path and its ETag', async () => {
    stub(409, { error: 'conflict', current: doc(4), etag: '"e4"' });
    const r = await api.saveDocument(id, { requestId: 'r', baseRevision: 2, title: 't', body });
    expect(r.kind === 'conflict' && r.current.etag).toBe('"e4"');
  });
});
