import { type EventInput } from '$lib/features/events/model';
import { eventsApi } from '$lib/features/events/api';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { MovePreviewView, TimeEntryInput } from '$lib/api/client';
import { api, ApiError } from '$lib/api/client';

// 문서 이동의 409는 두 모양이다: 그사이 접근 변화가 바뀌었으면 새 미리보기(`MovePreview`, 다시 확인을 받는다),
// 그 밖의 충돌은 `ErrorBody`(일반 오류). 일정·시간 기록 고치기의 409는 기준 버전이 지난 것이며 서버 최신본이 온다.
const preview: MovePreviewView = {
  changes: [
    {
      change: 'gain',
      user_id: 'u-kim',
      display_name: '김연구',
      from_grade: null,
      to_grade: 'read',
    },
    {
      change: 'lose',
      user_id: 'u-lee',
      display_name: '이조교',
      from_grade: 'read',
      to_grade: null,
    },
  ],
  history_moves_with_document: true,
  reference_changes: true,
  active_public_links: 1,
  children_left_behind: 2,
};

const event = {
  id: 'e1',
  item_id: 'i1',
  workspace_id: 'ws-1',
  kind: 'meeting',
  title: '주간 회의',
  description: '',
  all_day: false,
  start_date: null,
  end_date: null,
  starts_at: '2026-10-22T01:00:00Z',
  ends_at: '2026-10-22T02:00:00Z',
  timezone: 'Asia/Seoul',
  recurrence: null,
  excluded_dates: [],
  version: 4,
  access: 'write',
};

const eventInput: EventInput = {
  kind: 'meeting',
  title: '주간 회의',
  description: '안건 정리',
  all_day: false,
  start_date: null,
  end_date: null,
  starts_at: '2026-10-22T01:00:00Z',
  ends_at: '2026-10-22T02:00:00Z',
  timezone: 'Asia/Seoul',
};

const entry = {
  id: 't1',
  started_at: '2026-10-22T00:00:00Z',
  ended_at: '2026-10-22T01:00:00Z',
  paused_at: null,
  paused_seconds: 0,
  seconds: 3600,
  kind: 'work',
  memo: '',
  target: null,
  version: 2,
};

const entryInput: TimeEntryInput = {
  started_at: '2026-10-22T00:00:00Z',
  ended_at: '2026-10-22T01:30:00Z',
  paused_seconds: 0,
  kind: 'work',
  memo: '정리',
  target: null,
};

interface Seen {
  method: string;
  url: string;
  body: unknown;
}

/** 서버 대역: 받은 요청(메서드·주소·JSON 본문)을 기록하고 정해 둔 응답을 돌려준다. */
function reply(status: number, json: object): Seen[] {
  const seen: Seen[] = [];
  vi.stubGlobal('fetch', (target: RequestInfo | URL, init?: RequestInit) => {
    const body: unknown = typeof init?.body === 'string' ? JSON.parse(init.body) : init?.body;
    const url =
      typeof target === 'string' ? target : target instanceof URL ? target.href : target.url;
    seen.push({ method: init?.method ?? 'GET', url, body });
    return Promise.resolve(
      new Response(JSON.stringify(json), {
        status,
        headers: { 'Content-Type': 'application/json' },
      }),
    );
  });
  return seen;
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

describe('document move', () => {
  it('reads a 200 move preview', async () => {
    const seen = reply(200, preview);
    const result = await api.movePreview('d1', { workspaceId: 'ws-2', itemId: null });
    expect(result).toEqual(preview);
    expect(seen).toStrictEqual([
      {
        method: 'POST',
        url: '/api/documents/d1/move/preview',
        body: { target_workspace_id: 'ws-2', target_item_id: null },
      },
    ]);
  });

  it('returns the new preview when the access change differs (409 MovePreview)', async () => {
    const changed = {
      ...preview,
      changes: [
        ...preview.changes,
        {
          change: 'lose',
          user_id: 'u-park',
          display_name: '박학생',
          from_grade: 'read',
          to_grade: null,
        },
      ],
    };
    const seen = reply(409, changed);
    const result = await api.moveDocument('d1', { workspaceId: 'ws-2', itemId: 'i2' }, preview);
    expect(result.kind).toBe('changed');
    expect(
      result.kind === 'changed' ? result.preview.changes.map((c) => c.display_name) : [],
    ).toEqual(['김연구', '이조교', '박학생']);
    // 확인한 것은 접근이 생기고 사라지는 사람뿐이다.
    expect(seen).toStrictEqual([
      {
        method: 'POST',
        url: '/api/documents/d1/move',
        body: {
          target_workspace_id: 'ws-2',
          target_item_id: 'i2',
          acknowledged: { changes: preview.changes },
        },
      },
    ]);
  });

  it('reports any other move conflict (409 ErrorBody) as an ApiError', async () => {
    reply(409, { error: 'conflict', message: 'the document moved in between' });
    const failed = await failure(
      api.moveDocument('d1', { workspaceId: 'ws-2', itemId: null }, preview),
    );
    expect(failed).toBeInstanceOf(ApiError);
    expect(failed instanceof ApiError ? [failed.status, failed.code, failed.message] : []).toEqual([
      409,
      'conflict',
      'the document moved in between',
    ]);
  });
});

describe('event and time entry conflicts', () => {
  it('turns a stale event version (409 EventConflict) into a conflict with the latest event', async () => {
    const seen = reply(409, { error: 'conflict', current: event });
    const result = await eventsApi.updateEvent('e1', 3, eventInput);
    expect(result.kind).toBe('conflict');
    expect(result.kind === 'conflict' ? result.current.version : 0).toBe(4);
    expect(seen).toStrictEqual([
      { method: 'PUT', url: '/api/events/e1', body: { base_version: 3, ...eventInput } },
    ]);
  });

  it('turns a stale time entry version (409 TimeEntryConflict) into a conflict with the latest entry', async () => {
    const seen = reply(409, { error: 'conflict', current: entry });
    const result = await api.updateTimeEntry('t1', 1, entryInput);
    expect(result.kind).toBe('conflict');
    expect(result.kind === 'conflict' ? result.current.version : 0).toBe(2);
    // 대상이 없으면 칸을 빼지 않고 `target: null`로 보낸다.
    expect(seen).toStrictEqual([
      { method: 'PUT', url: '/api/time-entries/t1', body: { base_version: 1, ...entryInput } },
    ]);
    expect(seen[0]?.body).toHaveProperty('target', null);
  });

  it('sends the time entry target as given', async () => {
    const seen = reply(200, { ...entry, target: { kind: 'task', id: 'b', label: '구현' } });
    const result = await api.updateTimeEntry('t1', 2, {
      ...entryInput,
      target: { kind: 'task', id: 'b' },
    });
    expect(result.kind === 'updated' ? result.entry.target?.label : null).toBe('구현');
    expect(seen[0]?.body).toHaveProperty('target', { kind: 'task', id: 'b' });
  });

  it('reports a 409 without the latest copy (ErrorBody) as an ApiError', async () => {
    reply(409, { error: 'invalid_state', message: 'archived' });
    const failed = [
      await failure(eventsApi.updateEvent('e1', 3, eventInput)),
      await failure(api.updateTimeEntry('t1', 1, entryInput)),
    ];
    expect(failed.map((e) => (e instanceof ApiError ? [e.status, e.code] : e))).toEqual([
      [409, 'invalid_state'],
      [409, 'invalid_state'],
    ]);
  });
});
