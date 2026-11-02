import { activityApi } from '$lib/features/activity/api';
import { taskData as work } from '$lib/features/tasks/detail-data';
import './fixtures/task-statuses';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import type { AssigneeView, TaskDetailView } from '$lib/api/client';
import { api, ApiError, TaskRuleConflict } from '$lib/api/client';
import TaskDetail from '$lib/features/tasks/TaskDetail.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import { describeRule } from '$lib/features/events/labels';

// 업무 계정 전환과 별도 반복 규칙 명령의 회귀. 필드 충돌은 task-field-conflict.test.ts가 새 선택 흐름으로 검증한다.
beforeEach(() => {
  me.userId = 'user-a';
});

afterEach(() => {
  me.userId = '';
  vi.restoreAllMocks();
});

const v2: TaskDetailView = {
  id: 't1',
  item_id: 'i1',
  module_id: 'm1',
  workspace_id: 'ws-1',
  parent_id: null,
  reference: 'KED-1',
  title: '옛 제목',
  description: '',
  status: '할 일',
  priority: 'none',
  start_date: null,
  due_date: null,
  estimate_minutes: null,
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

function mount(current: TaskDetailView = v2, people: AssigneeView[] = []) {
  vi.spyOn(work, 'task').mockResolvedValue(current);
  vi.spyOn(work, 'assignable').mockResolvedValue(people);
  vi.spyOn(api, 'taskBacklinks').mockResolvedValue([]);
  vi.spyOn(activityApi, 'comments').mockResolvedValue([]);
  return render(TaskDetail, {
    props: {
      taskId: current.id,
      tasks: [current],
      workspaces: [],
      onchanged: () => undefined,
      onopen: () => undefined,
    },
  });
}

function field(id: string): HTMLInputElement | HTMLTextAreaElement {
  const el = document.getElementById(id);
  if (!(el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement)) throw new Error(id);
  return el;
}

async function type(id: string, value: string): Promise<void> {
  await fireEvent.input(field(id), { target: { value } });
}

it('preserves and hides task input after account_changed without another save', async () => {
  await setLocale('ko', { reload: false });
  mount();
  const update = vi
    .spyOn(work, 'updateTask')
    .mockRejectedValue(new ApiError(409, 'account_changed', 'account_changed'));
  await screen.findByLabelText('제목');
  await type('task-title', '계정 A의 비공개 입력');
  await fireEvent.click(screen.getByRole('button', { name: '저장' }));
  await screen.findByText(/계정이 바뀌어 입력을 숨겼습니다/);
  expect(field('task-title')).toHaveValue('계정 A의 비공개 입력');
  expect(field('task-title')).not.toBeVisible();
  expect(update).toHaveBeenCalledTimes(1);
});

it('keeps recurrence input on 409, shows the latest rule and explicitly resaves once', async () => {
  await setLocale('ko', { reload: false });
  const base = { freq: 'daily' as const, interval: 1, weekdays: [], until: null, count: null };
  const latest = { ...base, interval: 3 };
  const initial = {
    ...v2,
    due_date: '2026-10-26',
    series: { id: 'series', root_task_id: v2.id, timezone: 'Asia/Seoul', rule: base },
  };
  mount(initial);
  const save = vi
    .spyOn(work, 'setTaskRecurrence')
    .mockRejectedValueOnce(
      new TaskRuleConflict({ ...initial, series: { ...initial.series, rule: latest } }),
    )
    .mockReturnValueOnce(new Promise(() => undefined));
  await screen.findByLabelText('간격');
  await fireEvent.input(screen.getByLabelText('간격'), { target: { value: '2' } });
  await fireEvent.click(screen.getByRole('button', { name: '반복 저장' }));
  await screen.findByText(/그사이 다른 사람이 반복 규칙/);
  expect(screen.getByLabelText('간격')).toHaveValue('2');
  expect(save).toHaveBeenCalledTimes(1);
  expect(save.mock.calls[0]?.[4]).toEqual(base);
  await fireEvent.click(screen.getByRole('button', { name: '반복 규칙: 내 입력으로 다시 저장' }));
  await fireEvent.click(screen.getByRole('button', { name: '반복 규칙: 내 입력으로 다시 저장' }));
  expect(save).toHaveBeenCalledTimes(2);
  expect(save.mock.calls[1]?.[2]).toEqual({ ...base, interval: 2 });
  expect(save.mock.calls[1]?.[4]).toEqual(latest);
});

it('restores a recurrence draft with its original base and requires an explicit conflict retry', async () => {
  await setLocale('ko', { reload: false });
  window.localStorage.clear();
  const base = { freq: 'daily' as const, interval: 1, weekdays: [], until: null, count: null };
  const latest = { ...base, interval: 3 };
  const initial = {
    ...v2,
    due_date: '2026-10-26',
    series: { id: 'series', root_task_id: v2.id, timezone: 'Asia/Seoul', rule: base },
  };
  const first = mount(initial);
  try {
    await screen.findByLabelText('간격');
    await type('task-recurrence-interval', '2');
    await waitFor(() => {
      expect(
        Object.values(window.localStorage).some((value: string) =>
          value.includes('"interval":"2"'),
        ),
      ).toBe(true);
    });
    first.unmount();
    const current = { ...initial, series: { ...initial.series, rule: latest } };
    mount(current);
    const save = vi
      .spyOn(work, 'setTaskRecurrence')
      .mockRejectedValueOnce(new TaskRuleConflict(current))
      .mockResolvedValueOnce({
        ...current,
        series: { ...current.series, rule: { ...base, interval: 2 } },
      });
    await fireEvent.click(await screen.findByRole('button', { name: m.common_draft_restore() }));
    expect(screen.getByLabelText('간격')).toHaveValue('2');
    expect(save).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByRole('button', { name: '반복 저장' }));
    await screen.findByText(/그사이 다른 사람이 반복 규칙/);
    expect(save).toHaveBeenCalledTimes(1);
    expect(save.mock.calls[0]?.[4]).toEqual(base);
    await fireEvent.click(screen.getByRole('button', { name: '반복 규칙: 내 입력으로 다시 저장' }));
    await waitFor(() => {
      expect(save).toHaveBeenCalledTimes(2);
    });
    expect(save.mock.calls[1]?.[4]).toEqual(latest);
  } finally {
    window.localStorage.clear();
  }
});

it.each(['success', 'conflict'] as const)(
  'ignores a late recurrence %s after switching accounts',
  async (result) => {
    await setLocale('ko', { reload: false });
    window.localStorage.clear();
    const rule = { freq: 'daily' as const, interval: 1, weekdays: [], until: null, count: null };
    const initial = {
      ...v2,
      due_date: '2026-10-26',
      series: { id: 'series', root_task_id: v2.id, timezone: 'Asia/Seoul', rule },
    };
    mount(initial);
    let resolve!: (value: TaskDetailView) => void;
    let reject!: (error: unknown) => void;
    const pending = new Promise<TaskDetailView>((yes, no) => {
      resolve = yes;
      reject = no;
    });
    vi.spyOn(work, 'setTaskRecurrence').mockReturnValue(pending);
    await screen.findByLabelText('간격');
    await type('task-recurrence-interval', '2');
    await fireEvent.click(screen.getByRole('button', { name: '반복 저장' }));
    me.userId = 'user-b';
    await waitFor(() => expect(field('task-title')).not.toBeVisible());
    const fresh = {
      ...initial,
      version: 3,
      title: 'late result',
      series: { ...initial.series, rule: { ...rule, interval: 7 } },
    };
    if (result === 'success') resolve(fresh);
    else reject(new TaskRuleConflict(fresh));
    await pending.catch(() => undefined);
    me.userId = 'user-a';
    await waitFor(() => expect(field('task-title')).toBeVisible());
    expect(field('task-title')).toHaveValue(v2.title);
    await fireEvent.click(await screen.findByRole('button', { name: m.common_draft_restore() }));
    expect(screen.getByLabelText('간격')).toHaveValue('2');
    expect(screen.queryByText(/그사이 다른 사람이 반복 규칙/)).toBeNull();
    expect(screen.queryByText(describeRule({ ...rule, interval: 7 }), { exact: false })).toBeNull();
    window.localStorage.clear();
  },
);

it.each(['success', 'conflict'] as const)(
  'ignores a late recurrence %s after opening another task',
  async (result) => {
    await setLocale('ko', { reload: false });
    const rule = { freq: 'daily' as const, interval: 1, weekdays: [], until: null, count: null };
    const initial = {
      ...v2,
      due_date: '2026-10-26',
      series: { id: 'series', root_task_id: v2.id, timezone: 'Asia/Seoul', rule },
    };
    const view = mount(initial);
    let resolve!: (value: TaskDetailView) => void;
    let reject!: (error: unknown) => void;
    const pending = new Promise<TaskDetailView>((yes, no) => {
      resolve = yes;
      reject = no;
    });
    vi.spyOn(work, 'setTaskRecurrence').mockReturnValue(pending);
    await screen.findByLabelText('간격');
    await type('task-recurrence-interval', '2');
    await fireEvent.click(screen.getByRole('button', { name: '반복 저장' }));
    const other = {
      ...initial,
      id: 't2',
      title: '다른 업무',
      series: {
        ...initial.series,
        id: 'other-series',
        root_task_id: 't2',
        rule: { ...rule, interval: 4 },
      },
    };
    vi.mocked(work.task).mockResolvedValue(other);
    await view.rerender({
      taskId: other.id,
      tasks: [initial, other],
      workspaces: [],
      onchanged: () => undefined,
      onopen: () => undefined,
    });
    await waitFor(() => expect(field('task-title')).toHaveValue(other.title));
    const fresh = {
      ...initial,
      version: 3,
      title: 'late result',
      series: { ...initial.series, rule: { ...rule, interval: 7 } },
    };
    if (result === 'success') resolve(fresh);
    else reject(new TaskRuleConflict(fresh));
    await pending.catch(() => undefined);
    await waitFor(() => expect(screen.getByLabelText('간격')).toHaveValue('4'));
    expect(field('task-title')).toHaveValue(other.title);
    expect(screen.queryByText(/그사이 다른 사람이 반복 규칙/)).toBeNull();
    expect(screen.queryByText(describeRule({ ...rule, interval: 7 }), { exact: false })).toBeNull();
  },
);
