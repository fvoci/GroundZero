import { workApi as work } from '$lib/features/tasks/api';
import './fixtures/task-statuses';
import { fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { ItemView } from '$lib/api/client';
import { api } from '$lib/api/client';
import TaskBoard from '$lib/features/tasks/TaskBoard.svelte';
import { router } from '$lib/router.svelte';
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
  vi.spyOn(work, 'tasks').mockResolvedValue([]);
  vi.spyOn(api, 'savedViews').mockResolvedValue([]);
  vi.spyOn(work, 'syncRecurrences').mockResolvedValue(0);
  render(TaskBoard, { props: { item, workspaces: [], openTaskId: undefined, locked } });
  await screen.findByTestId('task-board');
}

describe('task board of an item in an archived module', () => {
  it('offers no quick add or create form when locked', async () => {
    await mount(true);
    expect(screen.queryAllByRole('button', { name: /^업무 추가$/ })).toHaveLength(0);
    expect(screen.queryByLabelText('새 업무 제목', { exact: true })).toBeNull();
    expect(screen.queryByRole('button', { name: '상태 편집' })).toBeNull();
  });

  it('offers them when not locked', async () => {
    await mount(false);
    expect(screen.getAllByRole('button', { name: /^업무 추가$/ }).length).toBeGreaterThan(0);
    expect(screen.getByLabelText('새 업무 제목', { exact: true })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '상태 편집' })).toBeInTheDocument();
  });
});

// R5 C-1·F-2: 쓰기를 잃어 CSV 가져오기 칸만 먼저 사라지면 그 가드가 이동 연쇄에서 빠져야 한다. 남으면 사라진 칸의 글자가
// 확인창 없이 이동을 조용히 막았다.
describe('task board losing write access with CSV input', () => {
  it('does not keep the removed CSV guard in the navigation chain', async () => {
    await setLocale('ko', { reload: false });
    vi.spyOn(work, 'tasks').mockResolvedValue([]);
    vi.spyOn(api, 'savedViews').mockResolvedValue([]);
    vi.spyOn(work, 'syncRecurrences').mockResolvedValue(0);
    vi.spyOn(work, 'assignable').mockResolvedValue([]);
    const view = render(TaskBoard, {
      props: { item, workspaces: [], openTaskId: undefined, locked: false },
    });
    await screen.findByTestId('task-board');
    await fireEvent.input(screen.getByLabelText(/한 줄에 한 업무/), { target: { value: 'A' } });
    await view.rerender({ item, workspaces: [], openTaskId: undefined, locked: true });
    expect(screen.queryByLabelText(/한 줄에 한 업무/)).toBeNull();
    router.navigate('/csv-guard-released');
    expect(router.path).toBe('/csv-guard-released');
    expect(screen.queryByRole('alertdialog')).toBeNull();
  });
});
