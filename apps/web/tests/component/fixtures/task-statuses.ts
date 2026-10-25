import type { TaskStatusDefinition } from '@fvoci/api-client';
import { beforeEach, vi } from 'vitest';
import { api } from '$lib/api/client';

export const TEST_STATUSES: TaskStatusDefinition[] = [
  { id: 's-todo', name: 'todo', color: '', position: 0, category: 'todo', task_count: 0 },
  { id: 's-doing', name: 'in_progress', color: '', position: 1, category: 'doing', task_count: 0 },
  { id: 's-done', name: 'done', color: '', position: 2, category: 'done', task_count: 0 },
];

beforeEach(() => {
  vi.spyOn(api, 'taskStatuses').mockResolvedValue(TEST_STATUSES);
});
