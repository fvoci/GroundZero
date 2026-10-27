import './fixtures/task-statuses';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { api, parseTask } from '$lib/api/client';
import { displaySettings } from '$lib/display';
import { dayIn } from '$lib/features/events/time';
import TaskBoard from '$lib/features/tasks/TaskBoard.svelte';
import TaskCalendar from '$lib/features/tasks/TaskCalendar.svelte';
import { me } from '$lib/me.svelte';
import { org } from '$lib/org.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';
import { itemPath, itemViewPath, router } from '$lib/router.svelte';
import { TASK_ITEM, taskViews, WORKSPACE } from '../../stories/fixtures/synthetic';

const originalTimeZone = me.timeZone;

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  window.sessionStorage.clear();
  router.guard = undefined;
  router.path = itemPath(WORKSPACE.id, TASK_ITEM.id);
  window.history.replaceState(null, '', router.path);
  vi.spyOn(api, 'syncRecurrences').mockResolvedValue(0);
  vi.spyOn(api, 'tasks').mockResolvedValue(taskViews());
  vi.spyOn(api, 'savedViews').mockResolvedValue([]);
});
afterEach(() => {
  vi.restoreAllMocks();
  vi.useRealTimers();
  me.timeZone = originalTimeZone;
  router.guard = undefined;
});

it('accepts custom status names and requires a valid completion category', () => {
  const first = taskViews()[0];
  expect(parseTask({ ...first, status: '출시 승인', status_category: 'doing' })).toMatchObject({
    status: '출시 승인',
    status_category: 'doing',
  });
  expect(() => parseTask({ ...first, status_category: 'in_progress' })).toThrow();
});

it('orders columns by definitions and only the owner sees the status editor', async () => {
  vi.mocked(api.taskStatuses).mockResolvedValue([
    { id: 'b', name: '검토', color: '', position: 3, category: 'doing', task_count: 0 },
    { id: 'a', name: '접수', color: '', position: 1, category: 'todo', task_count: 0 },
  ]);
  const view = render(TaskBoard, {
    item: { ...TASK_ITEM, access: 'owner' },
    workspaces: [],
    openTaskId: undefined,
  });
  await screen.findByTestId('task-board');
  expect(
    [...document.querySelectorAll<HTMLElement>('[data-lane]')].map((el) => el.dataset.lane),
  ).toEqual(['접수', '검토']);
  expect(screen.getByRole('button', { name: '상태 편집' })).toBeVisible();
  await view.rerender({
    item: { ...TASK_ITEM, access: 'write' },
    workspaces: [],
    openTaskId: undefined,
  });
  expect(screen.queryByRole('button', { name: '상태 편집' })).toBeNull();
});

it('URL view and query override the scoped session and switching pushes a view route', async () => {
  window.sessionStorage.setItem(
    org.key(`board:${TASK_ITEM.id}`),
    JSON.stringify({ view: 'gantt', filter: { status: 'todo' }, sort: 'due' }),
  );
  router.path = itemViewPath(WORKSPACE.id, TASK_ITEM.id, 'list');
  window.history.replaceState(null, '', `${router.path}?status=in_progress&sort=title`);
  vi.mocked(api.savedViews).mockResolvedValue([
    {
      id: 'saved',
      name: '기본',
      version: 1,
      config: { default: true, view: 'calendar', filter: { status: 'done' } },
    },
  ]);
  const push = vi.spyOn(window.history, 'pushState');
  render(TaskBoard, { item: TASK_ITEM, workspaces: [], openTaskId: undefined });
  await waitFor(() =>
    expect(screen.getByRole('button', { name: '목록' })).toHaveAttribute('aria-pressed', 'true'),
  );
  expect(screen.getByLabelText('상태')).toHaveValue('in_progress');
  await fireEvent.click(screen.getByRole('button', { name: '칸반' }));
  expect(push).toHaveBeenLastCalledWith(
    null,
    '',
    itemViewPath(WORKSPACE.id, TASK_ITEM.id, 'board'),
  );
});

it('uses the saved default before the session without rewriting the URL', async () => {
  window.sessionStorage.setItem(
    org.key(`board:${TASK_ITEM.id}`),
    JSON.stringify({ view: 'gantt', sort: 'due' }),
  );
  vi.mocked(api.savedViews).mockResolvedValue([
    {
      id: 'saved',
      name: '기본',
      version: 1,
      config: { default: true, view: 'list', filter: { status: 'done' }, sort: 'title' },
    },
  ]);
  const replace = vi.spyOn(window.history, 'replaceState');
  render(TaskBoard, { item: TASK_ITEM, workspaces: [], openTaskId: undefined });
  await waitFor(() => {
    expect(screen.getByRole('button', { name: '목록' })).toHaveAttribute('aria-pressed', 'true');
  });
  expect(screen.getByLabelText('상태')).toHaveValue('done');
  expect(replace).not.toHaveBeenCalled();
});

it('does not choose arbitrarily when multiple saved views claim to be default', async () => {
  window.sessionStorage.setItem(
    org.key(`board:${TASK_ITEM.id}`),
    JSON.stringify({ view: 'list', filter: { status: 'in_progress' } }),
  );
  // R5 최종 A FA-8(3): 세션 보기는 첫 그리기에서 동기로 적용되므로, 늦게 온 기본 보기 둘을 어떻게 다루는지 보려면 그
  // 응답이 처리된 뒤에 본다.
  let release: () => void = () => undefined;
  const listed = vi.mocked(api.savedViews).mockImplementation(
    () =>
      new Promise((resolve) => {
        release = () => {
          resolve([
            {
              id: 'one',
              name: '하나',
              version: 1,
              config: { default: true, view: 'gantt', filter: { status: 'done' } },
            },
            { id: 'two', name: '둘', version: 1, config: { default: true, view: 'calendar' } },
          ]);
        };
      }),
  );
  render(TaskBoard, { item: TASK_ITEM, workspaces: [], openTaskId: undefined });
  await waitFor(() => {
    expect(listed).toHaveBeenCalled();
  });
  release();
  for (let i = 0; i < 3; i += 1) await new Promise((r) => setTimeout(r, 0));
  expect(screen.getByRole('button', { name: '목록' })).toHaveAttribute('aria-pressed', 'true');
  expect(screen.getByLabelText('상태')).toHaveValue('in_progress');
});

it('restores the scoped session without rewriting a path that has no view', async () => {
  window.sessionStorage.setItem(org.key(`board:${TASK_ITEM.id}`), JSON.stringify({ view: 'list' }));
  const replace = vi.spyOn(window.history, 'replaceState');
  render(TaskBoard, { item: TASK_ITEM, workspaces: [], openTaskId: undefined });
  await waitFor(() =>
    expect(screen.getByRole('button', { name: '목록' })).toHaveAttribute('aria-pressed', 'true'),
  );
  expect(replace).not.toHaveBeenCalled();
});

it.each([
  ['2026-10-31T15:30:00Z', 'Asia/Seoul'],
  ['2026-11-01T00:30:00Z', 'America/Los_Angeles'],
])('renders periods and points across the month boundary at %s in %s', (now, zone) => {
  vi.useFakeTimers({ toFake: ['Date'] });
  vi.setSystemTime(new Date(now));
  me.timeZone = zone;
  const first = taskViews()[0];
  if (first === undefined) throw new Error('missing fixture');
  const month = dayIn(new Date().toISOString(), displaySettings().timeZone).slice(0, 7);
  render(TaskCalendar, {
    tasks: [
      { ...first, start_date: `${month}-10`, due_date: `${month}-12` },
      { ...first, id: 'point', start_date: null, due_date: `${month}-11` },
    ],
    onopen: vi.fn(),
  });
  expect(document.querySelectorAll('[data-span="range"]')).toHaveLength(3);
  expect(document.querySelectorAll('[data-span="point"]')).toHaveLength(1);
});
