import { afterEach, describe, expect, it, vi } from 'vitest';

import { api, ApiError } from '$lib/api/client';
import { systemApi } from '$lib/features/trash/api';

// API 클라이언트가 실제로 보내는 요청(메서드·주소·본문)을 서버 대역으로 기록해 확인한다. 범위·커서 쿼리,
// 개인 영역과 워크스페이스 경로 전환, 빈 본문과 값이 있는 본문, 같은 주소의 PUT·DELETE 구분을 본다.
interface Seen {
  method: string;
  url: string;
  /** JSON 본문은 읽은 값, 그 밖(Blob 등)은 보낸 그대로, 본문이 없으면 undefined. */
  body: unknown;
  headers: Headers;
}

/** 서버 대역: 받은 요청을 기록하고, 부를 때마다 새 응답을 만든다(본문은 한 번만 읽힌다). */
function stub(respond: () => Response): Seen[] {
  const seen: Seen[] = [];
  vi.stubGlobal('fetch', (target: RequestInfo | URL, init?: RequestInit) => {
    const body: unknown = typeof init?.body === 'string' ? JSON.parse(init.body) : init?.body;
    const url =
      typeof target === 'string' ? target : target instanceof URL ? target.href : target.url;
    seen.push({ method: init?.method ?? 'GET', url, body, headers: new Headers(init?.headers) });
    return Promise.resolve(respond());
  });
  return seen;
}

function json(status: number, value: unknown): Response {
  return new Response(JSON.stringify(value), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });
}

/** 거부된 이유(성공하면 undefined). */
function failure(p: Promise<unknown>): Promise<unknown> {
  return p.then(
    () => undefined,
    (e: unknown) => e,
  );
}

afterEach(() => {
  vi.unstubAllGlobals();
});

const calendar = {
  tasks: [],
  projected_tasks: [],
  events: [],
  occurrences: [],
  dates: [],
  items: [],
};
const range = 'from=2026-10-01&to=2026-10-31&tz=Asia%2FSeoul';

interface Case {
  name: string;
  call: () => Promise<unknown>;
  /** 응답. 204면 본문 없음. */
  status: number;
  reply?: unknown;
  method: string;
  url: string;
  /** 보낸 JSON 본문. 없으면 본문을 보내지 않아야 한다. */
  body?: unknown;
}

const cases: Case[] = [
  {
    name: 'calendar without a scope sends only the range',
    call: () => api.calendar('ws-1', '2026-10-01', '2026-10-31', 'Asia/Seoul'),
    status: 200,
    reply: calendar,
    method: 'GET',
    url: `/api/workspaces/ws-1/calendar?${range}`,
  },
  {
    name: 'calendar narrowed to a module sends module_id',
    call: () => api.calendar('ws-1', '2026-10-01', '2026-10-31', 'Asia/Seoul', { moduleId: 'm 1' }),
    status: 200,
    reply: calendar,
    method: 'GET',
    url: `/api/workspaces/ws-1/calendar?${range}&module_id=m+1`,
  },
  {
    name: 'calendar narrowed to an item sends module_id and item_id',
    call: () =>
      api.calendar('ws-1', '2026-10-01', '2026-10-31', 'Asia/Seoul', {
        moduleId: 'm1',
        itemId: 'i1',
      }),
    status: 200,
    reply: calendar,
    method: 'GET',
    url: `/api/workspaces/ws-1/calendar?${range}&module_id=m1&item_id=i1`,
  },
  {
    name: 'calendar leaves out a scope key given as undefined',
    call: () =>
      api.calendar('ws-1', '2026-10-01', '2026-10-31', 'Asia/Seoul', {
        moduleId: undefined,
        itemId: 'i1',
      }),
    status: 200,
    reply: calendar,
    method: 'GET',
    url: `/api/workspaces/ws-1/calendar?${range}&item_id=i1`,
  },
  {
    name: 'public link without expiry sends an empty body',
    call: () => api.createPublicLink('d1', null),
    status: 201,
    reply: { id: 'l1', url: 'https://example.test/p/t1' },
    method: 'POST',
    url: '/api/documents/d1/public-links',
    body: {},
  },
  {
    name: 'public link with expiry sends expires_at',
    call: () => api.createPublicLink('d1', '2026-11-01T00:00:00Z'),
    status: 201,
    reply: { id: 'l1', url: 'https://example.test/p/t1' },
    method: 'POST',
    url: '/api/documents/d1/public-links',
    body: { expires_at: '2026-11-01T00:00:00Z' },
  },
  {
    name: 'adding a group member is a PUT without a body',
    call: () => api.setGroupMember('g1', 'u1', true),
    status: 204,
    method: 'PUT',
    url: '/api/groups/g1/members/u1',
  },
  {
    name: 'removing a group member is a DELETE without a body',
    call: () => api.setGroupMember('g1', 'u1', false),
    status: 204,
    method: 'DELETE',
    url: '/api/groups/g1/members/u1',
  },
  {
    name: 'workspace audit',
    call: () => api.audit('ws-1'),
    status: 200,
    reply: [],
    method: 'GET',
    url: '/api/workspaces/ws-1/audit',
  },
  {
    name: 'personal audit',
    call: () => api.audit(null),
    status: 200,
    reply: [],
    method: 'GET',
    url: '/api/me/audit',
  },
  {
    name: 'workspace audit with a before cursor',
    call: () => api.audit('ws-1', 'a/1'),
    status: 200,
    reply: [],
    method: 'GET',
    url: '/api/workspaces/ws-1/audit?before=a%2F1',
  },
  {
    name: 'personal audit with a before cursor',
    call: () => api.audit(null, 'a1'),
    status: 200,
    reply: [],
    method: 'GET',
    url: '/api/me/audit?before=a1',
  },
  {
    name: 'workspace trash',
    call: () => systemApi.batches('ws-1'),
    status: 200,
    reply: [],
    method: 'GET',
    url: '/api/trash-batches?workspace_id=ws-1',
  },
  {
    name: 'personal trash',
    call: () => systemApi.batches(null),
    status: 200,
    reply: [],
    method: 'GET',
    url: '/api/trash-batches',
  },
  {
    name: 'purging expired workspace trash',
    call: () => api.purgeExpiredTrash('ws-1'),
    status: 200,
    reply: { purged: 0 },
    method: 'POST',
    url: '/api/trash-batches/purge-expired?workspace_id=ws-1',
  },
  {
    name: 'purging expired personal trash',
    call: () => api.purgeExpiredTrash(null),
    status: 200,
    reply: { purged: 0 },
    method: 'POST',
    url: '/api/trash-batches/purge-expired',
  },
  {
    name: 'document revisions from the newest',
    call: () => api.revisions('d1', null),
    status: 200,
    reply: [],
    method: 'GET',
    url: '/api/documents/d1/revisions',
  },
  {
    name: 'document revisions before a cursor',
    call: () => api.revisions('d1', 7),
    status: 200,
    reply: [],
    method: 'GET',
    url: '/api/documents/d1/revisions?before=7',
  },
];

describe('API request shape', () => {
  it('composes workspace trash from the batch and retention resources', async () => {
    const replies = [[], { retention_days: 30, version_retention_days: 90 }];
    const seen = stub(() => json(200, replies.shift()));
    await expect(api.trash('ws-1')).resolves.toEqual({
      entries: [],
      retention_days: 30,
      version_retention_days: 90,
    });
    expect(seen.map(({ method, url }) => ({ method, url }))).toEqual([
      { method: 'GET', url: '/api/trash-batches?workspace_id=ws-1' },
      { method: 'GET', url: '/api/workspaces/ws-1/retention' },
    ]);
  });

  it.each(cases)('$name', async (c) => {
    const seen = stub(() =>
      c.status === 204 ? new Response(null, { status: 204 }) : json(c.status, c.reply),
    );
    await c.call();
    expect(seen.map(({ method, url, body }) => ({ method, url, body }))).toStrictEqual([
      { method: c.method, url: c.url, body: c.body },
    ]);
  });
});

describe('upload part', () => {
  it('sends the raw chunk with its SHA-256', async () => {
    const seen = stub(() => new Response(null, { status: 204 }));
    const chunk = new Blob(['abc']);
    await expect(api.putUploadPart('u1', 3, chunk, 'ba7816bf')).resolves.toBeUndefined();
    expect(seen.map((s) => [s.method, s.url])).toEqual([['PUT', '/api/uploads/u1/parts/3']]);
    expect(seen[0]?.body).toBe(chunk);
    expect(seen[0]?.headers.get('Content-Type')).toBe('application/octet-stream');
    expect(seen[0]?.headers.get('x-content-sha256')).toBe('ba7816bf');
  });

  // 앞단 프록시가 대신 답한 HTML·빈 본문도 JSON 해석 오류가 아니라 ApiError로 알린다.
  it.each([
    {
      name: 'a proxy HTML page for a too-large body',
      status: 413,
      type: 'text/html',
      text: '<html>413 Request Entity Too Large</html>',
      code: 'unexpected',
    },
    {
      name: 'a proxy HTML gateway error',
      status: 502,
      type: 'text/html',
      text: '<html>502 Bad Gateway</html>',
      code: 'unreachable',
    },
    { name: 'an empty error body', status: 400, type: 'text/plain', text: '', code: 'unexpected' },
    {
      name: 'an ErrorBody',
      status: 409,
      type: 'application/json',
      text: JSON.stringify({ error: 'invalid_state', message: 'the upload is no longer open' }),
      code: 'invalid_state',
    },
  ])('reports $name as an ApiError', async ({ status, type, text, code }) => {
    stub(() => new Response(text, { status, headers: { 'Content-Type': type } }));
    const failed = await failure(api.putUploadPart('u1', 1, new Blob(['a']), 'ca978112'));
    expect(failed).toBeInstanceOf(ApiError);
    expect(failed instanceof ApiError ? [failed.status, failed.code] : []).toEqual([status, code]);
  });
});
