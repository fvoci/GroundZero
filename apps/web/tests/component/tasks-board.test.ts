import { describe, expect, it } from 'vitest';

import { lanes, listOrder, planBoardMove } from '$lib/features/tasks/board';

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
