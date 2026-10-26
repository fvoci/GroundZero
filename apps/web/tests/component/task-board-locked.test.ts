import './fixtures/task-statuses';
import { render, screen } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { ItemView } from '$lib/api/client';
import { api } from '$lib/api/client';
import TaskBoard from '$lib/features/tasks/TaskBoard.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

// CodeRabbit(#141 91b9904d): 모듈만 보관되고 항목은 보관되지 않으면 항목 화면은 편집 폼을 숨기지만 업무 보드는 쓰기 가능으로
// 남아 빠른 추가·이동을 허용했고 서버는 거부했다(app_item_archived는 모듈 보관도 본다). 항목 화면이 `locked`로 알린다.
afterEach(() => {
  vi.restoreAllMocks();
});

const item: ItemView = {
  id: 'i1',
  module_id: 'm1',
  workspace_id: 'ws-1',
  reference: 'KED-1',
  title: '항목',
  description: '',
  visibility: 'workspace',
  access: 'owner',
  archived: false,
  updated_at: '2026-10-19T00:00:00Z',
  status: 'active',
  is_template: false,
  version: 1,
  starts_on: null,
  ends_on: null,
};

async function mount(locked: boolean): Promise<void> {
  await setLocale('ko', { reload: false });
  vi.spyOn(api, 'tasks').mockResolvedValue([]);
  vi.spyOn(api, 'savedViews').mockResolvedValue([]);
  vi.spyOn(api, 'syncRecurrences').mockResolvedValue(0);
  render(TaskBoard, { props: { item, workspaces: [], openTaskId: undefined, locked } });
  await screen.findByTestId('task-board');
}

describe('task board of an item in an archived module', () => {
  it('offers no quick add or create form when locked', async () => {
    await mount(true);
    expect(screen.queryAllByRole('button', { name: /^업무 추가$/ })).toHaveLength(0);
    expect(screen.queryByLabelText('새 업무 제목', { exact: true })).toBeNull();
  });

  it('offers them when not locked', async () => {
    await mount(false);
    expect(screen.getAllByRole('button', { name: /^업무 추가$/ }).length).toBeGreaterThan(0);
    expect(screen.getByLabelText('새 업무 제목', { exact: true })).toBeInTheDocument();
  });
});
