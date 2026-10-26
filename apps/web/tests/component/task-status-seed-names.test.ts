import { afterEach, describe, expect, it, vi } from 'vitest';

import { api, parseTask } from '$lib/api/client';
import { formOf } from '$lib/features/tasks/form';

// 개정 2026-10-26 ⑪(D-116): 새 항목의 기본 상태 이름은 `할 일`·`진행 중`·`완료`다. 이 단계의 화면은 열을 안쪽 id로 다루므로
// 서버로 보낼 때 열 id를 기본 이름으로 바꾸고, 받을 때 기본 이름을 열 id로 돌린다. 항목이 정의한 다른 이름은 그대로 오간다.
afterEach(() => {
  vi.unstubAllGlobals();
});

const raw = {
  id: 't1',
  item_id: 'i1',
  module_id: 'm1',
  workspace_id: 'ws-1',
  parent_id: null,
  reference: 'KED-1',
  title: '업무',
  description: '',
  status: '진행 중',
  status_category: 'doing',
  priority: 'none',
  start_date: null,
  due_date: null,
  estimate_minutes: null,
  tags: [],
  milestone: false,
  predecessors: [],
  series: null,
  occurrence_date: null,
  assignees: [],
  checklist_done: 0,
  checklist_total: 0,
  version: 1,
  move_version: 0,
  completed_at: null,
  access: 'write',
  updated_at: '2026-10-26T00:00:00Z',
  checklist: [],
};

function capture(): { bodies: unknown[] } {
  const bodies: unknown[] = [];
  vi.stubGlobal(
    'fetch',
    vi.fn((_input: RequestInfo | URL, init?: RequestInit) => {
      bodies.push(typeof init?.body === 'string' ? JSON.parse(init.body) : undefined);
      return Promise.resolve(
        new Response(JSON.stringify(raw), {
          status: 200,
          headers: { 'content-type': 'application/json' },
        }),
      );
    }),
  );
  return { bodies };
}

function status(body: unknown): unknown {
  return typeof body === 'object' && body !== null && 'status' in body ? body.status : undefined;
}

describe('seeded status names at the API boundary', () => {
  it('maps the seeded Korean names to lanes and keeps other names', () => {
    const task = parseTask(raw);
    expect(task.status).toBe('in_progress');
    expect(task.status_name).toBe('진행 중');
    const custom = parseTask({ ...raw, status: '검토', status_category: 'doing' });
    expect(custom.status).toBe('in_progress');
    expect(custom.status_name).toBe('검토');
  });

  it('sends seeded names for creation and lane moves while preserving the edited form name', async () => {
    const { bodies } = capture();
    await api.createTask('i1', 'r1', '제목', null, {
      description: '',
      status: 'todo',
      priority: 'none',
      start_date: null,
      due_date: null,
      estimate_minutes: null,
      tags: [],
      assignee_ids: [],
      milestone: false,
    });
    await api.updateTask('t1', 1, {
      title: '제목',
      description: '',
      status: '완료',
      priority: 'none',
      start_date: null,
      due_date: null,
      estimate_minutes: null,
      tags: [],
      assignee_ids: [],
      milestone: false,
    });
    await api.moveTask('t1', 'in_progress', null);
    expect(bodies.map(status)).toEqual(['할 일', '완료', '진행 중']);
  });

  it('passes an item-defined name through unchanged', async () => {
    const { bodies } = capture();
    await api.updateTask('t1', 1, {
      title: '제목',
      description: '',
      status: '검토',
      priority: 'none',
      start_date: null,
      due_date: null,
      estimate_minutes: null,
      tags: [],
      assignee_ids: [],
      milestone: false,
    });
    expect(bodies.map(status)).toEqual(['검토']);
  });

  it.each(['todo', 'in_progress', 'done'])(
    'keeps the item-defined name %s when only the title is edited',
    async (name) => {
      const { bodies } = capture();
      const task = parseTask({ ...raw, status: name });
      await api.updateTask('t1', 1, { ...formOf({ ...task, checklist: [] }), title: '바꾼 제목' });
      expect(bodies.map(status)).toEqual([name]);
    },
  );
});
