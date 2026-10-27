import './fixtures/task-statuses';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { ItemView, TaskDetailView } from '$lib/api/client';
import { api } from '$lib/api/client';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';
import { itemTaskPath, itemViewPath, router } from '$lib/router.svelte';
import TaskBoardRouteHost from './fixtures/TaskBoardRouteHost.svelte';

// 수정 차분 검수 RA-1: 보드에서 연 상세는 `router.back()`으로 닫는다(FA-4). 닫기 확인은 그 호출에서 이미 끝났으므로 뒤따르는
// popstate가 보드 입력이나 방금 버린 상세 입력을 다시 묻지 않아야 한다. 다른 시험은 `history.back`을 막아 popstate가
// 일어나지 않으므로, 여기서는 jsdom의 실제 기록 이동과 주소를 따르는 부모(TaskBoardRouteHost)로 닫는다.

// 라우터는 UUID 경로만 해석한다.
const WS = '11111111-1111-1111-1111-111111111111';
const ITEM = '22222222-2222-2222-2222-222222222222';
const TASK = '33333333-3333-3333-3333-333333333333';
const board = itemViewPath(WS, ITEM, 'board');

const item: ItemView = {
  id: ITEM,
  module_id: 'm1',
  workspace_id: WS,
  reference: 'KED',
  title: '항목',
  description: '',
  visibility: 'workspace',
  access: 'write',
  archived: false,
  updated_at: '2026-10-19T00:00:00Z',
  status: 'active',
  is_template: false,
  version: 1,
  starts_on: null,
  ends_on: null,
};

const detail: TaskDetailView = {
  id: TASK,
  item_id: ITEM,
  module_id: 'm1',
  workspace_id: WS,
  parent_id: null,
  reference: 'KED-1',
  title: '옛 제목',
  description: '',
  status: '할 일',
  priority: 'none',
  start_date: null,
  due_date: null,
  estimate_minutes: 30,
  tags: [],
  milestone: false,
  predecessors: [],
  series: null,
  occurrence_date: null,
  assignees: [],
  status_category: 'todo',
  checklist_done: 0,
  checklist_total: 0,
  version: 2,
  move_version: 0,
  completed_at: null,
  access: 'write',
  updated_at: '2026-10-21T00:00:00Z',
  checklist: [],
};

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  me.userId = 'user-a';
  window.localStorage.clear();
  window.sessionStorage.clear();
  window.history.replaceState(null, '', board);
  router.path = board;
  router.hash = '';
});
afterEach(() => {
  vi.restoreAllMocks();
  me.userId = '';
  window.localStorage.clear();
});

const card = (): HTMLElement | null => document.querySelector(`[data-task-card="${TASK}"]`);
const confirmBox = (): HTMLElement | null =>
  screen.queryByRole('alertdialog', { name: m.common_discard_title() });
const dialog = (): HTMLElement | null =>
  document.querySelector<HTMLElement>('[data-slot="dialog-content"]');
const title = (): HTMLElement => {
  const el = document.getElementById('task-title');
  if (el === null) throw new Error('task-title');
  return el;
};

/** 보드를 그리고 `beforeOpen` 뒤 카드를 눌러 상세를 연다. */
async function openFromBoard(beforeOpen?: () => Promise<void>) {
  vi.spyOn(api, 'tasks').mockResolvedValue([detail]);
  vi.spyOn(api, 'savedViews').mockResolvedValue([]);
  vi.spyOn(api, 'syncRecurrences').mockResolvedValue(0);
  vi.spyOn(api, 'task').mockResolvedValue(detail);
  vi.spyOn(api, 'assignable').mockResolvedValue([]);
  vi.spyOn(api, 'taskBacklinks').mockResolvedValue([]);
  vi.spyOn(api, 'comments').mockResolvedValue([]);
  const update = vi.spyOn(api, 'updateTask');
  render(TaskBoardRouteHost, { props: { item } });
  await beforeOpen?.();
  const push = vi.spyOn(window.history, 'pushState');
  await fireEvent.click(await screen.findByRole('button', { name: /^KED-1\s*옛 제목$/ }));
  expect(router.path).toBe(itemTaskPath(WS, ITEM, TASK));
  await screen.findByRole('dialog');
  await waitFor(() => {
    expect(title()).toHaveValue('옛 제목');
  });
  return { push, update };
}

async function typeBoardInput(): Promise<void> {
  await fireEvent.input(await screen.findByLabelText(m.tasks_new_title()), {
    target: { value: 'Keep this new title' },
  });
  await fireEvent.click(screen.getByTestId('task-filter-toggle'));
  await fireEvent.input(await screen.findByLabelText(m.tasks_view_name_aria()), {
    target: { value: 'Keep this view name' },
  });
}

/** 상세가 닫혀 주소가 보드로 돌아갈 때까지 기다리고, 그동안 확인을 열지 않았는지 본다. */
async function closedToBoard(): Promise<void> {
  await waitFor(() => {
    expect(confirmBox()).toBeNull();
    expect(router.path).toBe(board);
    expect(dialog()).toBeNull();
  });
  // popstate 가드가 막았다면 상세 주소를 다시 쌓는다. 몇 틱 더 기다려도 보드에 머무는지 본다.
  await new Promise((r) => setTimeout(r, 30));
  expect(window.location.pathname).toBe(board);
  expect(router.path).toBe(board);
  expect(confirmBox()).toBeNull();
}

describe('closing a task opened from the board with the real popstate', () => {
  it.each(['close button', 'Escape'] as const)(
    'keeps board input without asking when closing by the %s',
    async (how) => {
      const { push } = await openFromBoard(typeBoardInput);
      if (how === 'Escape') await fireEvent.keyDown(title(), { key: 'Escape' });
      else
        await fireEvent.click(
          within(dialog() ?? document.body).getByRole('button', { name: m.common_close() }),
        );
      expect(confirmBox()).toBeNull();
      await closedToBoard();
      // 닫기는 기록을 쌓지 않는다(카드를 연 한 번뿐).
      expect(push).toHaveBeenCalledTimes(1);
      expect(screen.getByLabelText(m.tasks_new_title())).toHaveValue('Keep this new title');
      expect(screen.getByLabelText(m.tasks_view_name_aria())).toHaveValue('Keep this view name');
      // 실제로 보드를 떠나는 이동은 그대로 보드 입력을 묻는다(D-121).
      router.navigate('/tasks');
      expect(
        await screen.findByRole('alertdialog', { name: m.common_discard_title() }),
      ).toBeTruthy();
      expect(router.path).toBe(board);
    },
  );

  it('asks once about detail changes only and keeps board input after discarding them', async () => {
    const { push, update } = await openFromBoard(typeBoardInput);
    await fireEvent.input(document.getElementById('task-description') ?? document.body, {
      target: { value: 'Discard only this detail' },
    });
    await fireEvent.click(
      within(dialog() ?? document.body).getByRole('button', { name: m.common_close() }),
    );
    const box = await screen.findByRole('alertdialog', { name: m.common_discard_title() });
    await fireEvent.click(within(box).getByRole('button', { name: m.common_discard_confirm() }));
    await closedToBoard();
    expect(push).toHaveBeenCalledTimes(1);
    expect(update).not.toHaveBeenCalled();
    expect(screen.getByLabelText(m.tasks_new_title())).toHaveValue('Keep this new title');
    expect(screen.getByLabelText(m.tasks_view_name_aria())).toHaveValue('Keep this view name');
  });

  it('asks once for a changed title: keep editing keeps input and focus, discard closes to the card', async () => {
    const { push, update } = await openFromBoard();
    await fireEvent.input(title(), { target: { value: '새 제목' } });
    title().focus();
    await fireEvent.keyDown(title(), { key: 'Escape' });
    const first = await screen.findByRole('alertdialog', { name: m.common_discard_title() });
    await fireEvent.click(within(first).getByRole('button', { name: m.common_discard_keep() }));
    await waitFor(() => {
      expect(confirmBox()).toBeNull();
      expect(document.activeElement).toBe(title());
    });
    expect(title()).toHaveValue('새 제목');
    expect(router.path).toBe(itemTaskPath(WS, ITEM, TASK));

    await fireEvent.keyDown(title(), { key: 'Escape' });
    const second = await screen.findByRole('alertdialog', { name: m.common_discard_title() });
    await fireEvent.click(within(second).getByRole('button', { name: m.common_discard_confirm() }));
    await closedToBoard();
    expect(push).toHaveBeenCalledTimes(1);
    expect(update).not.toHaveBeenCalled();
    await waitFor(() => {
      expect(document.activeElement).toBe(card());
    });
  });
});
