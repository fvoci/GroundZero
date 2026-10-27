import { describe, expect, it } from 'vitest';

import type { TaskView } from '$lib/api/client';
import {
  applyFilter,
  EMPTY_FILTER,
  isFiltered,
  readConfig,
  sortTasks,
  tasksByDay,
} from '$lib/features/tasks/filters';

const task = (n: number, over: Partial<TaskView>): TaskView => ({
  item_id: 'i',
  module_id: 'm',
  workspace_id: 'w',
  parent_id: null,
  estimate_minutes: null,
  milestone: false,
  predecessors: [],
  series: null,
  occurrence_date: null,
  status_category: 'todo',
  checklist_done: 0,
  checklist_total: 0,
  version: 1,
  move_version: 0,
  completed_at: null,
  access: 'write',
  id: `t${String(n)}`,
  reference: `DEV-${String(n)}`,
  title: `업무 ${String(n)}`,
  description: '',
  status: 'todo',
  priority: 'none',
  tags: [],
  assignees: [],
  due_date: null,
  start_date: null,
  updated_at: `2026-10-1${String(n)}T00:00:00Z`,
  ...over,
});

const tasks = [
  task(1, {
    title: '로그인 화면',
    priority: 'high',
    due_date: '2026-10-20',
    tags: ['UI'],
    assignees: [{ user_id: 'me', display_name: '나' }],
  }),
  task(2, { status: 'done', due_date: '2026-10-05' }),
  task(3, { priority: 'urgent', assignees: [{ user_id: 'u2', display_name: '김' }] }),
];

describe('task filters', () => {
  it('filters by text, status, assignee, priority, tag and due range', () => {
    const ids = (f: Partial<typeof EMPTY_FILTER>): string[] =>
      applyFilter(tasks, { ...EMPTY_FILTER, ...f }, 'me').map((t) => t.id);
    expect(ids({ text: '로그인' })).toEqual(['t1']);
    expect(ids({ text: 'dev-3' })).toEqual(['t3']);
    expect(ids({ status: 'done' })).toEqual(['t2']);
    expect(ids({ assignee: 'me' })).toEqual(['t1']);
    expect(ids({ assignee: 'none' })).toEqual(['t2']);
    expect(ids({ assignee: 'u2' })).toEqual(['t3']);
    expect(ids({ priority: 'urgent' })).toEqual(['t3']);
    expect(ids({ tag: 'ui' })).toEqual(['t1']);
    expect(ids({ dueFrom: '2026-10-10' })).toEqual(['t1']);
    expect(ids({ dueTo: '2026-10-10' })).toEqual(['t2']);
    expect(isFiltered(EMPTY_FILTER)).toBe(false);
  });

  it('sorts and reads saved views defensively', () => {
    expect(sortTasks(tasks, 'due').map((t) => t.id)).toEqual(['t2', 't1', 't3']);
    expect(sortTasks(tasks, 'priority').map((t) => t.id)).toEqual(['t3', 't1', 't2']);
    expect(sortTasks(tasks, 'updated').map((t) => t.id)).toEqual(['t3', 't2', 't1']);
    expect(
      readConfig({ view: 'calendar', sort: 'nope', filter: { status: 'done', tag: 3 } }),
    ).toEqual({
      view: 'calendar',
      sort: 'position',
      filter: { ...EMPTY_FILTER, status: 'done' },
    });
    expect(
      tasksByDay(tasks)
        .get('2026-10-20')
        ?.map((t) => t.id),
    ).toEqual(['t1']);
  });
});
