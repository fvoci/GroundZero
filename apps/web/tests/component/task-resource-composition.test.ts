import { expect, it } from 'vitest';
import { composeTasks } from '$lib/features/tasks/board-data';
import type { TaskResource } from '$lib/features/tasks/model';

// 각각의 읽기 사이에 삭제·권한 변경이 생겨도 목록 밖 업무를 관계로 되살리지 않는다.
it('fences relations to the visible task snapshot and counts the item checklist collection', () => {
  const task: TaskResource = {
    id: 'visible',
    item_id: 'item',
    module_id: 'module',
    workspace_id: 'workspace',
    title: '보이는 업무',
    description: '',
    status: '할 일',
    status_category: 'todo',
    priority: 'none',
    milestone: false,
    tags: [],
    version: 1,
    move_version: 2,
    updated_at: '2026-10-28T00:00:00Z',
    parent_id: 'removed',
    series_id: 'missing',
  };
  const [view] = composeTasks(
    [task],
    {
      assignees: [{ task_id: 'removed', user_id: 'private', display_name: '숨김' }],
      dependencies: [{ successor_id: task.id, predecessor_id: 'removed', kind: 'FS', lag_days: 0 }],
      series: [],
      checklist: [
        { id: 'c1', task_id: task.id, checked: true, text: '완료' },
        { id: 'c2', task_id: task.id, checked: false, text: '남음' },
        { id: 'c3', task_id: 'removed', checked: true, text: '다른 업무' },
      ],
    },
    'read',
  );
  expect(view).toMatchObject({
    parent_id: null,
    predecessors: [],
    assignees: [],
    series: null,
    checklist_done: 1,
    checklist_total: 2,
    access: 'read',
  });
});
