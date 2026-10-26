import { afterEach, describe, expect, it, vi } from 'vitest';

import type { TaskInput } from '$lib/api/client';
import { api, ApiError, TaskRuleConflict } from '$lib/api/client';
import { accountSafety } from '$lib/features/feedback/feedback.svelte';

// 업무 고치기의 409는 세 갈래다(계약 `TaskConflict`·`ErrorBody`): 기준 버전이 지났으면 `conflict`, 승인한 후속 재배치가
// 맞지 않으면 `shift_conflict`(둘 다 최신본 포함), 보관·다시 시도 등은 최신본 없는 일반 오류다. 칸반 이동은 읽은 이동
// 판을 줄 때만 보내고, 그사이 옮겨졌으면 `move_conflict`를 일반 오류로 받는다(D-89).
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
  predecessor_ids: [],
  predecessors: [],
  series: null,
  occurrence_date: null,
  assignees: [],
  status_category: 'todo',
  checklist_done: 0,
  checklist_total: 0,
  version: 3,
  move_version: 5,
  completed_at: null,
  access: 'write',
  updated_at: '2026-10-21T00:00:00Z',
  checklist: [],
};

const input: TaskInput = {
  title: '구현',
  description: '',
  status: 'todo',
  priority: 'none',
  start_date: null,
  due_date: null,
  estimate_minutes: null,
  tags: [],
  assignee_ids: [],
  milestone: false,
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
  accountSafety.switching = false;
});

describe('task update conflicts', () => {
  it('turns a stale base version (TaskConflict `conflict`) into a conflict with the latest task', async () => {
    const seen = reply(409, { error: 'conflict', current: task });
    const result = await api.updateTask('b', 2, input);
    expect(result.kind).toBe('conflict');
    expect(result.kind === 'conflict' ? result.current.version : 0).toBe(3);
    // 기준 버전과 필드, 승인한 재배치(기본은 빈 목록)를 함께 보낸다.
    expect(seen.map((s) => [s.method, s.url])).toEqual([['PUT', '/api/tasks/b']]);
    expect(seen[0]?.body).toStrictEqual({ base_version: 2, ...input, successor_shifts: [] });
  });

  it('turns a stale approved shift (TaskConflict `shift_conflict`) into a shift conflict', async () => {
    reply(409, { error: 'shift_conflict', current: task });
    const shift = { task_id: 'c', base_version: 1, start_date: '2026-10-22', due_date: null };
    const result = await api.updateTask('b', 3, input, [shift]);
    expect(result.kind).toBe('shift_conflict');
    expect(result.kind === 'shift_conflict' ? result.current.id : '').toBe('b');
  });

  it('reports an archived or retried change (ErrorBody without latest) as an ordinary 409 error', async () => {
    reply(409, { error: 'invalid_state', message: 'the item is archived', request_id: 'r1' });
    const failed = await failure(api.updateTask('b', 3, input));
    expect(failed).toBeInstanceOf(ApiError);
    expect(
      failed instanceof ApiError ? [failed.status, failed.code, failed.requestId] : [],
    ).toEqual([409, 'invalid_state', 'r1']);
  });
});

describe('kanban move (D-89)', () => {
  it('sends base_move_version only when the read move version is given', async () => {
    const seen = reply(200, task);
    await api.moveTask('b', 'done', null);
    // 0도 읽은 판이다(빠뜨리지 않는다).
    await api.moveTask('b', 'in_progress', 'a', 0);
    expect(seen.map((s) => [s.method, s.url])).toEqual([
      ['PUT', '/api/tasks/b/position'],
      ['PUT', '/api/tasks/b/position'],
    ]);
    expect(seen[0]?.body).toStrictEqual({ status: '완료', before_id: null });
    expect(seen[1]?.body).toStrictEqual({
      status: '진행 중',
      before_id: 'a',
      base_move_version: 0,
    });
  });

  it('reports a task moved in between (`move_conflict` with latest) as an ApiError', async () => {
    reply(409, { error: 'move_conflict', current: task });
    const failed = await failure(api.moveTask('b', 'done', null, 4));
    expect(failed).toBeInstanceOf(ApiError);
    expect(failed instanceof ApiError ? [failed.status, failed.code] : []).toEqual([
      409,
      'move_conflict',
    ]);
  });
});
it('sends the recurrence base and retains the latest rule on 409 without retry', async () => {
  const base = { freq: 'daily' as const, interval: 1, weekdays: [], until: null, count: null };
  const seen = reply(409, { error: 'rule_conflict', current: task });
  const result = await failure(api.setTaskRecurrence('b', base, 'Asia/Seoul', null));
  expect(result).toBeInstanceOf(TaskRuleConflict);
  expect(seen).toHaveLength(1);
  expect(seen[0]?.body).toEqual({ rule: base, timezone: 'Asia/Seoul', base: null });
});

// account_changed는 다시 보내지 않고 폼에 전달하되, 다른 요청과 똑같이 앱 전체 계정 전환(안전 상태)을 시작한다(D-96,
// CodeRabbit #141 1e6de63c).
it('passes account_changed to the task form without retrying, and starts the account switch', async () => {
  const seen = reply(409, { error: 'account_changed' });
  expect(await failure(api.updateTask('b', 3, input))).toMatchObject({
    status: 409,
    code: 'account_changed',
  });
  expect(seen).toHaveLength(1);
  expect(accountSafety.switching).toBe(true);
});
