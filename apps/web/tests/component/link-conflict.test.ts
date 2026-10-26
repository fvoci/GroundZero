import { afterEach, describe, expect, it, vi } from 'vitest';

import { api, ApiError } from '$lib/api/client';

// 관계 설정의 409는 두 모양이다(D-91, 계약 `PredecessorConflict`): 승인 목록이 맞지 않으면 TaskConflict(최신본 포함),
// 보관·동시 변경·id 부족이면 ErrorBody(`invalid_state`, 최신본 없음). 클라이언트는 앞의 것만 다시 묻기로 돌리고 뒤의
// 것은 일반 오류로 알린다(최신본이 있다고 가정하지 않는다).
const task = {
  id: 'b',
  item_id: 'i1',
  module_id: 'm1',
  workspace_id: 'ws-1',
  parent_id: null,
  reference: 'KED-b',
  title: '구현',
  description: '',
  status: 'todo',
  priority: 'none',
  start_date: null,
  due_date: null,
  estimate_minutes: null,
  tags: [],
  milestone: false,
  predecessors: [{ task_id: 'a', kind: 'FS', lag_days: 0 }],
  series: null,
  occurrence_date: null,
  assignees: [],
  status_category: 'todo',
  checklist_done: 0,
  checklist_total: 0,
  version: 3,
  move_version: 0,
  completed_at: null,
  access: 'write',
  updated_at: '2026-10-21T00:00:00Z',
  checklist: [],
};

function reply(status: number, json: object): void {
  vi.stubGlobal('fetch', () =>
    Promise.resolve(
      new Response(JSON.stringify(json), {
        status,
        headers: { 'Content-Type': 'application/json' },
      }),
    ),
  );
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('link conflict responses', () => {
  it('turns a stale approval (TaskConflict) into a shift conflict with the latest task', async () => {
    reply(409, { error: 'shift_conflict', current: task });
    const result = await api.setPredecessor('user-a', 'b', 'a', 'SS', 1, null, []);
    expect(result.kind).toBe('shift_conflict');
    expect(result.kind === 'shift_conflict' ? result.current.version : 0).toBe(3);
  });

  it('turns a changed link (TaskConflict link_conflict) into a link conflict with the latest task', async () => {
    reply(409, { error: 'link_conflict', current: task });
    const result = await api.setPredecessor(
      'user-a',
      'b',
      'a',
      'SS',
      1,
      { kind: 'FS', lag_days: 0 },
      [],
    );
    expect(result.kind).toBe('link_conflict');
    expect(result.kind === 'link_conflict' ? result.current.version : 0).toBe(3);
  });

  it('reports an archived or retried change (ErrorBody) as an ordinary 409 error', async () => {
    reply(409, { error: 'invalid_state', message: 'the item is archived', request_id: 'r1' });
    const failed = await api.setPredecessor('user-a', 'b', 'a', 'SS', 1, null, []).then(
      () => undefined,
      (e: unknown) => e,
    );
    expect(failed).toBeInstanceOf(ApiError);
    expect(failed instanceof ApiError ? [failed.status, failed.code] : []).toEqual([
      409,
      'invalid_state',
    ]);
  });
});
