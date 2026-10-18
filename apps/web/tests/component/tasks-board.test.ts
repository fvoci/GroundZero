import { describe, expect, it } from 'vitest';

import { ApiError, NetworkError } from '$lib/api/client';
import {
  applyMove,
  dropBefore,
  lanes,
  listOrder,
  moveErrorMessage,
  planBoardMove,
  planDrop,
} from '$lib/features/tasks/board';
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

describe('kanban drag and drop (D-79)', () => {
  const board = [
    { id: 'a', status: 'todo' as const },
    { id: 'b', status: 'todo' as const },
    { id: 'c', status: 'todo' as const },
    { id: 'd', status: 'done' as const },
  ];
  const boxes = [
    { id: 'a', top: 0, height: 40 },
    { id: 'b', top: 50, height: 40 },
    { id: 'c', top: 100, height: 40 },
  ];

  it('finds the card the pointer drops before, skipping the dragged card', () => {
    expect(dropBefore(boxes, 'c', 10)).toBe('a');
    expect(dropBefore(boxes, 'c', 30)).toBe('b');
    expect(dropBefore(boxes, 'a', 30)).toBe('b');
    expect(dropBefore(boxes, 'a', 130)).toBeNull();
    expect(dropBefore([], 'a', 5)).toBeNull();
  });

  it('turns a drop into a move and ignores drops on the same place', () => {
    expect(planDrop(board, 'c', { status: 'todo', beforeId: 'a' }, true)).toEqual({
      status: 'todo',
      beforeId: 'a',
    });
    expect(planDrop(board, 'a', { status: 'todo', beforeId: 'b' }, true)).toBeUndefined();
    expect(planDrop(board, 'a', { status: 'todo', beforeId: 'a' }, true)).toBeUndefined();
    expect(planDrop(board, 'c', { status: 'todo', beforeId: null }, true)).toBeUndefined();
    expect(planDrop(board, 'a', { status: 'done', beforeId: 'd' }, true)).toEqual({
      status: 'done',
      beforeId: 'd',
    });
    expect(planDrop(board, 'missing', { status: 'done', beforeId: null }, true)).toBeUndefined();
  });

  it('only changes the column when the board is sorted by something else', () => {
    expect(planDrop(board, 'c', { status: 'todo', beforeId: 'a' }, false)).toBeUndefined();
    expect(planDrop(board, 'c', { status: 'done', beforeId: 'd' }, false)).toEqual({
      status: 'done',
      beforeId: null,
    });
  });

  it('applies a move to the screen the way the server orders the column', () => {
    const ids = (ts: readonly { id: string; status: string }[]): string[] =>
      ts.map((t) => `${t.id}:${t.status}`);
    expect(ids(applyMove(board, 'c', { status: 'todo', beforeId: 'a' }))).toEqual([
      'c:todo',
      'a:todo',
      'b:todo',
      'd:done',
    ]);
    expect(ids(applyMove(board, 'a', { status: 'done', beforeId: null }))).toEqual([
      'b:todo',
      'c:todo',
      'd:done',
      'a:done',
    ]);
    expect(ids(applyMove(board, 'd', { status: 'in_progress', beforeId: null }))).toEqual([
      'a:todo',
      'b:todo',
      'c:todo',
      'd:in_progress',
    ]);
    // 원래 배열은 그대로(실패하면 되돌릴 수 있게).
    expect(ids(board)).toEqual(['a:todo', 'b:todo', 'c:todo', 'd:done']);
  });

  it('explains why a move was refused', () => {
    expect(moveErrorMessage(new ApiError(400, 'bad_request', 'x'))).toContain('다른 곳에서');
    expect(moveErrorMessage(new ApiError(403, 'forbidden', 'x'))).toContain('권한');
    expect(moveErrorMessage(new ApiError(404, 'not_found', 'x'))).toContain('지워졌거나');
    expect(moveErrorMessage(new ApiError(409, 'invalid_state', 'x'))).toContain('보관');
    expect(moveErrorMessage(new NetworkError('down'))).toContain('되돌렸습니다');
  });
});
