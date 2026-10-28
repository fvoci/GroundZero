import { savedViewsApi } from '$lib/features/events/saved-views-api';
import { taskData as work } from '$lib/features/tasks/detail-data';
import './fixtures/task-statuses';
import { fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { ItemView } from '$lib/api/client';
import { ApiError } from '$lib/api/client';
import TaskBoard from '$lib/features/tasks/TaskBoard.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

// 저장한 보기 지우기(감사 W5): 첫 요청이 끝나기 전에 다시 누르면 같은 보기를 두 번 지우려 해 두 번째가 404로
// 실패하고, 이미 지워진 보기인데 "지우지 못했습니다"가 남았다. 진행 중에는 다시 보내지 않는다.
const item: ItemView = {
  id: 'i1',
  module_id: 'm1',
  workspace_id: 'ws-1',
  reference: 'KED-1',
  title: '항목',
  description: '',
  visibility: 'workspace',
  access: 'write',
  archived: false,
  updated_at: '2026-10-21T00:00:00Z',
  status: 'active',
  is_template: false,
  version: 1,
  starts_on: null,
  ends_on: null,
};

const view = {
  id: 'v1',
  name: '내 보기',
  config: { view: 'list', sort: 'position', filter: {} },
  version: 1,
};

describe('saved view delete', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
    window.sessionStorage.clear();
    vi.spyOn(work, 'tasks').mockResolvedValue([]);
    vi.spyOn(work, 'syncRecurrences').mockResolvedValue(0);
  });
  afterEach(() => {
    vi.restoreAllMocks();
    window.sessionStorage.clear();
  });

  it('sends one delete while the first is in flight and shows no failure after it succeeds', async () => {
    let views = [view];
    vi.spyOn(savedViewsApi, 'savedViews').mockImplementation(() => Promise.resolve(views));
    let finish: () => void = () => undefined;
    const deleteView = vi
      .spyOn(savedViewsApi, 'deleteView')
      .mockImplementationOnce(
        () =>
          new Promise<undefined>((resolve) => {
            finish = () => {
              views = [];
              resolve(undefined);
            };
          }),
      )
      .mockRejectedValueOnce(new ApiError(404, 'not_found', 'view not found'));

    render(TaskBoard, { props: { item, workspaces: [], openTaskId: undefined } });

    // 저장한 보기는 접힌 "필터·정렬" 영역 안에 있다.
    await fireEvent.click(screen.getByRole('button', { name: /필터·정렬/ }));
    const select = await screen.findByRole('combobox', { name: '저장한 보기' });
    await screen.findByRole('option', { name: '내 보기' });
    await fireEvent.change(select, { target: { value: 'v1' } });

    // 지우기는 한 번 더 묻는다(되돌릴 수 없다). 묻기만으로는 지우지 않고, 확인 단추를 두 번 누른다.
    await fireEvent.click(await screen.findByRole('button', { name: '보기 지우기' }));
    const remove = screen.getByRole('button', { name: '보기 지우기' });
    expect(remove).toHaveAccessibleDescription(
      '보기 "내 보기"을(를) 지울까요? 되돌릴 수 없습니다.',
    );
    expect(deleteView).not.toHaveBeenCalled();
    await fireEvent.click(remove);
    await fireEvent.click(remove);
    finish();

    await vi.waitFor(() => {
      expect(screen.queryByRole('option', { name: '내 보기' })).toBeNull();
    });
    expect(deleteView).toHaveBeenCalledTimes(1);
    expect(screen.queryByText(/보기를 지우지 못했습니다/)).toBeNull();
  });
});
