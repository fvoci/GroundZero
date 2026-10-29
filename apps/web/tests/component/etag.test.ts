import { afterEach, describe, expect, it, vi } from 'vitest';

import type { DocNode } from '$lib/api/client';
import { api } from '$lib/api/client';

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

describe('document GET ETag (D-77)', () => {
  it('keeps the ETag of a single document read', async () => {
    stub(200, doc(2), '"e2"');
    const d = await api.document(id);
    expect(d.etag).toBe('"e2"');
  });
});
