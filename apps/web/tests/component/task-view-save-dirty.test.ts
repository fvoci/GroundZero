import { type SavedViewView } from '$lib/features/events/saved-views-model';
import { savedViewsApi } from '$lib/features/events/saved-views-api';
import { taskData as work } from '$lib/features/tasks/detail-data';
import './fixtures/task-statuses';

import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { ItemView } from '$lib/api/client';

import TaskBoard from '$lib/features/tasks/TaskBoard.svelte';
import { me } from '$lib/me.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

// R5 최종 A FA-3: 저장 보기를 저장하는 사이 이름·검색어·필터가 바뀌면 `complete`가 거짓이라 보낸 요청이 남아
// `viewDirty`가 참으로 굳었다. 저장에 성공한 요청은 소진된 것으로 비워, 떠나기 확인이 뜨지 않는다(재현 R-view).

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
  updated_at: '2026-10-23T00:00:00Z',
  status: 'active',
  is_template: false,
  version: 1,
  starts_on: null,
  ends_on: null,
};

beforeEach(async () => {
  me.userId = 'user-a';
  await setLocale('ko', { reload: false });
});
afterEach(() => {
  vi.restoreAllMocks();
  window.sessionStorage.clear();
  window.localStorage.clear();
});

/** 보기 이름을 넣고 저장을 보낸 뒤, 응답을 붙잡은 채 `during`을 하고 응답을 푼다. */
async function saveViewWhile(during: () => Promise<void>): Promise<void> {
  vi.spyOn(work, 'tasks').mockResolvedValue([]);
  vi.spyOn(work, 'syncRecurrences').mockResolvedValue(0);
  const saved: SavedViewView[] = [];
  vi.spyOn(savedViewsApi, 'savedViews').mockImplementation(() => Promise.resolve([...saved]));
  let release: () => void = () => undefined;
  const create = vi.spyOn(savedViewsApi, 'createView').mockImplementation(
    (_item, _id, name, config) =>
      new Promise((resolve) => {
        release = () => {
          const v: SavedViewView = { id: 'v1', name, config: { ...config }, version: 1 };
          saved.push(v);
          resolve({ kind: 'saved', view: v });
        };
      }),
  );
  render(TaskBoard, { item, workspaces: [], openTaskId: undefined });
  await fireEvent.click(screen.getByRole('button', { name: /필터·정렬/ }));
  await fireEvent.input(screen.getByRole('textbox', { name: '보기 이름' }), {
    target: { value: '내 보기' },
  });
  await fireEvent.click(screen.getByRole('button', { name: '지금 보기 저장' }));
  await waitFor(() => {
    expect(create).toHaveBeenCalledOnce();
  });
  await during();
  release();
  await waitFor(() => {
    expect(screen.getByRole('combobox', { name: /저장한 보기/ })).toHaveValue('v1');
  });
}

function leaveIsBlocked(): boolean {
  const ev = new Event('beforeunload', { cancelable: true });
  window.dispatchEvent(ev);
  return ev.defaultPrevented;
}

describe('saving a view while the board changes', () => {
  it('does not stay unsaved after success when the search changed during the save', async () => {
    await saveViewWhile(async () => {
      await fireEvent.input(screen.getByRole('searchbox'), { target: { value: 'x' } });
    });
    expect(leaveIsBlocked()).toBe(false);
  });

  it('does not stay unsaved after success when nothing changed during the save', async () => {
    await saveViewWhile(() => Promise.resolve());
    expect(leaveIsBlocked()).toBe(false);
  });

  it('still asks before leaving when the view name changed during the save', async () => {
    // 저장한 뒤에도 남은 진짜 입력(바뀐 이름)은 지키되, 그것은 보낸 요청이 아니라 지금 입력 때문이다.
    await saveViewWhile(async () => {
      await fireEvent.input(screen.getByRole('textbox', { name: '보기 이름' }), {
        target: { value: '내 보기 2' },
      });
    });
    expect(leaveIsBlocked()).toBe(true);
  });
});
