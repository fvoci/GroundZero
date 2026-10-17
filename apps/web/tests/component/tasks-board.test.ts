import { describe, expect, it } from 'vitest';

import { lanes, listOrder, planBoardMove } from '$lib/features/tasks/board';
import { dayNumber, fromDayNumber, ganttLayout, MAX_GANTT_DAYS } from '$lib/features/tasks/gantt';

const tasks = [
  { id: 'a', status: 'todo' as const, parent_id: null },
  { id: 'b', status: 'todo' as const, parent_id: null },
  { id: 'a1', status: 'done' as const, parent_id: 'a' },
  { id: 'c', status: 'in_progress' as const, parent_id: null },
  { id: 'x1', status: 'todo' as const, parent_id: 'hidden' },
];

describe('task board (D-31)', () => {
  it('splits tasks into status lanes keeping server order', () => {
    const board = lanes(tasks);
    expect(board.todo.map((t) => t.id)).toEqual(['a', 'b', 'x1']);
    expect(board.in_progress.map((t) => t.id)).toEqual(['c']);
    expect(board.done.map((t) => t.id)).toEqual(['a1']);
  });

  it('plans keyboard moves as (lane, next task)', () => {
    expect(planBoardMove(tasks, 'a', 'up')).toBeUndefined();
    expect(planBoardMove(tasks, 'b', 'up')).toEqual({ status: 'todo', beforeId: 'a' });
    expect(planBoardMove(tasks, 'a', 'down')).toEqual({ status: 'todo', beforeId: 'x1' });
    expect(planBoardMove(tasks, 'b', 'down')).toEqual({ status: 'todo', beforeId: null });
    expect(planBoardMove(tasks, 'x1', 'down')).toBeUndefined();
    expect(planBoardMove(tasks, 'a', 'left')).toBeUndefined();
    expect(planBoardMove(tasks, 'c', 'right')).toEqual({ status: 'done', beforeId: null });
  });

  it('lists subtasks right after their parent and orphans last', () => {
    expect(listOrder(tasks).map((t) => t.id)).toEqual(['a', 'a1', 'b', 'c', 'x1']);
  });
});

describe('gantt layout (D-31, D-50)', () => {
  const base = { status: 'todo' as const, milestone: false };
  it('places dated tasks by day and keeps subtasks under their parent', () => {
    const layout = ganttLayout([
      { ...base, id: 'b', parent_id: null, start_date: '2026-10-30', due_date: '2026-11-02' },
      { ...base, id: 'a', parent_id: null, start_date: null, due_date: null },
      { ...base, id: 'b1', parent_id: 'b', start_date: null, due_date: '2026-10-28' },
      {
        ...base,
        id: 'm',
        parent_id: null,
        start_date: null,
        due_date: '2026-11-05',
        milestone: true,
      },
    ]);
    expect(layout.first).toBe('2026-10-28');
    expect(layout.days).toBe(9);
    expect(layout.rows.map((r) => [r.task.id, r.from, r.to])).toEqual([
      ['b', 2, 5],
      ['b1', 0, 0],
      ['m', 8, 8],
    ]);
    expect(layout.undated.map((t) => t.id)).toEqual(['a']);
  });

  it('converts dates without time zone drift', () => {
    expect(fromDayNumber(dayNumber('2026-03-29'))).toBe('2026-03-29');
    expect(dayNumber('2026-01-01') - dayNumber('2025-12-31')).toBe(1);
  });
});

describe('gantt range cap', () => {
  it('draws at most MAX_GANTT_DAYS columns and lists later tasks separately', () => {
    const base = { status: 'todo' as const, milestone: false, parent_id: null, start_date: null };
    const layout = ganttLayout([
      { ...base, id: 'a', due_date: '2026-01-01' },
      { ...base, id: 'b', start_date: '2026-12-01', due_date: '2027-03-01' },
      { ...base, id: 'c', due_date: '2029-01-01' },
    ]);
    expect(layout.days).toBe(MAX_GANTT_DAYS);
    expect(layout.rows.map((r) => r.task.id)).toEqual(['a', 'b']);
    expect(layout.rows[1]?.to).toBe(MAX_GANTT_DAYS - 1);
    expect(layout.later.map((t) => t.id)).toEqual(['c']);
  });
});
