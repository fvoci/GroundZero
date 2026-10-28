import { taskData as work } from '$lib/features/tasks/detail-data';
import type { TaskStatusDefinition } from '@fvoci/api-client';
import { beforeEach, vi } from 'vitest';

export const TEST_STATUSES: TaskStatusDefinition[] = [
  { id: 's-todo', name: '할 일', color: '', position: 0, category: 'todo', task_count: 0 },
  { id: 's-doing', name: '진행 중', color: '', position: 1, category: 'doing', task_count: 0 },
  { id: 's-done', name: '완료', color: '', position: 2, category: 'done', task_count: 0 },
];

beforeEach(() => {
  vi.spyOn(work, 'taskStatuses').mockResolvedValue(TEST_STATUSES);
});
