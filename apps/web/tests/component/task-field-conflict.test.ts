import './fixtures/task-statuses';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import TaskDetail from '$lib/features/tasks/TaskDetail.svelte';
import { taskData as work } from '$lib/features/tasks/detail-data';
import { activityApi } from '$lib/features/activity/api';
import { api, ApiError, type TaskDetailView } from '$lib/api/client';
import { me } from '$lib/me.svelte';
import * as feedback from '$lib/features/feedback/feedback.svelte';

const base: TaskDetailView = {
  id: 't1',
  item_id: 'i1',
  module_id: 'm1',
  workspace_id: 'ws-1',
  parent_id: null,
  reference: 'KED-1',
  title: '처음 제목',
  description: '처음 설명',
  status: '할 일',
  status_category: 'todo',
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
  checklist_done: 0,
  checklist_total: 0,
  version: 2,
  move_version: 0,
  completed_at: null,
  access: 'write',
  updated_at: '2026-10-21T00:00:00Z',
  checklist: [],
};
const fresh = {
  ...base,
  assignee_ids: [],
  version: 3,
  title: '서버 제목',
  description: '서버 설명',
};

beforeEach(() => {
  me.userId = 'user-a';
  localStorage.clear();
  vi.spyOn(work, 'task').mockResolvedValue(base);
  vi.spyOn(work, 'assignable').mockResolvedValue([]);
  vi.spyOn(api, 'taskBacklinks').mockResolvedValue([]);
  vi.spyOn(activityApi, 'comments').mockResolvedValue([]);
});
afterEach(() => {
  vi.restoreAllMocks();
  me.userId = '';
});

async function mount(current = base) {
  vi.mocked(work.task).mockResolvedValue(current);
  const view = render(TaskDetail, {
    taskId: current.id,
    tasks: [current],
    workspaces: [],
    onchanged: vi.fn(),
    onopen: vi.fn(),
  });
  await screen.findByLabelText('설명');
  return view;
}
async function edit(id: string, value: string) {
  const input = document.getElementById(id);
  if (input === null) throw new Error(id);
  await fireEvent.input(input, { target: { value } });
}
async function conflict() {
  await edit('task-title', '내 제목');
  await edit('task-description', '내 설명');
  const save = document.getElementById('task-save');
  if (save === null) throw new Error('save');
  await fireEvent.click(save);
  return screen.findByRole('region', { name: '바뀐 내용 확인' });
}

it('opens on the first 409, requires each clash, and sends mixed choices with the latest version', async () => {
  const update = vi
    .spyOn(work, 'updateTask')
    .mockResolvedValueOnce({ kind: 'conflict', current: fresh })
    .mockResolvedValueOnce({
      kind: 'updated',
      task: { ...fresh, description: '내 설명', version: 4 },
    });
  await mount();
  const panel = await conflict();
  expect(update).toHaveBeenCalledTimes(1);
  await fireEvent.click(within(panel).getByRole('button', { name: '선택한 내용으로 저장' }));
  expect(update).toHaveBeenCalledTimes(1);
  expect(within(panel).getByRole('alert')).toHaveTextContent('2');
  await waitFor(() => {
    expect(within(panel).getByRole('radio', { name: '제목: 내 입력 사용' })).toHaveFocus();
  });
  await fireEvent.click(within(panel).getByRole('radio', { name: '제목: 서버 최신 사용' }));
  await fireEvent.click(within(panel).getByRole('radio', { name: '설명: 내 입력 사용' }));
  await fireEvent.click(within(panel).getByRole('button', { name: '선택한 내용으로 저장' }));
  await waitFor(() => {
    expect(update).toHaveBeenCalledTimes(2);
  });
  expect(update.mock.calls[1]?.slice(0, 3)).toEqual(['user-a', 't1', 3]);
  expect(update.mock.calls[1]?.[3]).toEqual({ description: '내 설명' });
});

it('keeps the full long descriptions and cancels with Escape without changing input or sending', async () => {
  const serverText = `첫 줄\n${'긴내용'.repeat(1200)}\n끝`;
  const update = vi
    .spyOn(work, 'updateTask')
    .mockResolvedValue({ kind: 'conflict', current: { ...fresh, description: serverText } });
  await mount();
  const panel = await conflict();
  expect(within(panel).getByRole('textbox', { name: '설명: 서버 최신' })).toHaveValue(serverText);
  expect(within(panel).getByRole('textbox', { name: '설명: 내 입력' })).toHaveValue('내 설명');
  await fireEvent.keyDown(within(panel).getByRole('radio', { name: '제목: 내 입력 사용' }), {
    key: 'Escape',
  });
  expect(screen.queryByRole('region', { name: '바뀐 내용 확인' })).toBeNull();
  expect(screen.getByLabelText('제목')).toHaveValue('내 제목');
  expect(update).toHaveBeenCalledTimes(1);
});

it('previews the selected dates against the latest version and keeps the approved period whole', async () => {
  const latest = { ...fresh, due_date: '2026-11-15' };
  const update = vi
    .spyOn(work, 'updateTask')
    .mockResolvedValueOnce({ kind: 'conflict', current: latest })
    .mockResolvedValueOnce({
      kind: 'updated',
      task: { ...latest, due_date: '2026-11-20', version: 4 },
    });
  const preview = vi
    .spyOn(work, 'previewSchedule')
    .mockResolvedValue({ shifts: [], undated: [], blocked: null });
  await mount();
  await edit('task-due', '2026-11-20');
  const panel = await conflict();
  for (const label of ['제목', '설명', '일정(시작일·마감일·마일스톤)']) {
    await fireEvent.click(within(panel).getByRole('radio', { name: `${label}: 내 입력 사용` }));
  }
  await fireEvent.click(within(panel).getByRole('button', { name: '선택한 내용으로 저장' }));
  await waitFor(() => {
    expect(update).toHaveBeenCalledTimes(2);
  });
  expect(preview).toHaveBeenLastCalledWith('t1', null, '2026-11-20');
  expect(update.mock.calls[1]?.[2]).toBe(3);
  expect(update.mock.calls[1]?.[3]).toMatchObject({
    start_date: null,
    due_date: '2026-11-20',
    milestone: false,
  });
});

it.each(['updated', 'conflict'] as const)(
  'ignores a late %s after changing tasks',
  async (kind) => {
    let resolve!: (value: Awaited<ReturnType<typeof work.updateTask>>) => void;
    const pending = new Promise<Awaited<ReturnType<typeof work.updateTask>>>((yes) => {
      resolve = yes;
    });
    vi.spyOn(work, 'updateTask').mockReturnValueOnce(pending);
    const view = await mount();
    await edit('task-title', '내 제목');
    document.getElementById('task-save')?.click();
    const other = { ...base, id: 't2', title: '다른 업무' };
    vi.mocked(work.task).mockResolvedValueOnce(other);
    await view.rerender({
      taskId: 't2',
      tasks: [other],
      workspaces: [],
      onchanged: vi.fn(),
      onopen: vi.fn(),
    });
    await waitFor(() => {
      expect(screen.getByLabelText('제목')).toHaveValue('다른 업무');
    });
    expect(screen.getByLabelText('제목')).toBeEnabled();
    resolve(kind === 'updated' ? { kind, task: fresh } : { kind, current: fresh });
    await pending;
    expect(screen.getByLabelText('제목')).toHaveValue('다른 업무');
    expect(screen.queryByRole('region', { name: '바뀐 내용 확인' })).toBeNull();
  },
);

it('collapses server-only changes without blocking an explicit save', async () => {
  const update = vi
    .spyOn(work, 'updateTask')
    .mockResolvedValueOnce({
      kind: 'conflict',
      current: { ...fresh, description: base.description },
    })
    .mockResolvedValueOnce({
      kind: 'updated',
      task: { ...fresh, description: '내 설명', version: 4 },
    });
  await mount();
  await edit('task-description', '내 설명');
  document.getElementById('task-save')?.click();
  const panel = await screen.findByRole('region', { name: '바뀐 내용 확인' });
  expect(
    within(panel).getByText('다른 사람이 바꾼 칸 1개(서버 값 유지)').closest('details'),
  ).not.toHaveAttribute('open');
  expect(within(panel).getByRole('radio', { name: '설명: 내 입력 사용' })).toBeChecked();
  expect(update).toHaveBeenCalledTimes(1);
  await fireEvent.click(within(panel).getByRole('button', { name: '선택한 내용으로 저장' }));
  await waitFor(() => {
    expect(update).toHaveBeenCalledTimes(2);
  });
  expect(update.mock.calls[1]?.[3]).toEqual({ description: '내 설명' });
});

it('uses the item-defined status name in the collapsed server changes', async () => {
  vi.spyOn(work, 'updateTask').mockResolvedValue({
    kind: 'conflict',
    current: { ...fresh, status: '검수 대기', description: base.description },
  });
  await mount();
  await edit('task-description', '내 설명');
  document.getElementById('task-save')?.click();
  const panel = await screen.findByRole('region', { name: '바뀐 내용 확인' });
  expect(within(panel).getByText('검수 대기')).toBeInTheDocument();
  expect(within(panel).queryByRole('radio', { name: '상태: 내 입력 사용' })).toBeNull();
});

it('locks all other editors and freezes choices during a pending save', async () => {
  let finish!: (value: Awaited<ReturnType<typeof work.updateTask>>) => void;
  const update = vi
    .spyOn(work, 'updateTask')
    .mockResolvedValueOnce({ kind: 'conflict', current: fresh })
    .mockReturnValueOnce(
      new Promise((resolve) => {
        finish = resolve;
      }),
    );
  await mount();
  const panel = await conflict();
  expect(document.getElementById('task-title')?.closest('form')?.parentElement).toHaveProperty(
    'inert',
    true,
  );
  for (const label of ['제목', '설명'])
    await fireEvent.click(within(panel).getByRole('radio', { name: `${label}: 내 입력 사용` }));
  await fireEvent.click(within(panel).getByRole('button', { name: '선택한 내용으로 저장' }));
  expect(within(panel).getByRole('radio', { name: '설명: 내 입력 사용' })).toBeDisabled();
  expect(within(panel).getByRole('button', { name: '비교 닫고 입력 계속 편집' })).toBeDisabled();
  await fireEvent.keyDown(panel, { key: 'Escape' });
  expect(panel).toBeInTheDocument();
  finish({ kind: 'updated', task: { ...fresh, version: 4 } });
  await waitFor(() => {
    expect(screen.queryByRole('region', { name: '바뀐 내용 확인' })).toBeNull();
  });
  expect(update).toHaveBeenCalledTimes(2);
});

it('requires the original stale version again when cancelling after a selected save fails', async () => {
  const update = vi
    .spyOn(work, 'updateTask')
    .mockResolvedValueOnce({ kind: 'conflict', current: fresh })
    .mockRejectedValueOnce(new ApiError(503, 'unavailable', 'failed'))
    .mockResolvedValueOnce({ kind: 'conflict', current: fresh });
  await mount();
  let panel = await conflict();
  await fireEvent.click(within(panel).getByRole('radio', { name: '제목: 서버 최신 사용' }));
  await fireEvent.click(within(panel).getByRole('radio', { name: '설명: 내 입력 사용' }));
  await fireEvent.click(within(panel).getByRole('button', { name: '선택한 내용으로 저장' }));
  await waitFor(() => {
    expect(within(panel).getByRole('button', { name: '비교 닫고 입력 계속 편집' })).toBeEnabled();
  });
  await fireEvent.click(within(panel).getByRole('button', { name: '비교 닫고 입력 계속 편집' }));
  expect(screen.getByLabelText('제목')).toHaveValue('내 제목');
  document.getElementById('task-save')?.click();
  panel = await screen.findByRole('region', { name: '바뀐 내용 확인' });
  expect(update.mock.calls[2]?.[2]).toBe(2);
  expect(within(panel).getByRole('radio', { name: '제목: 내 입력 사용' })).not.toBeChecked();
});

it('applies selected text for manual editing without a request and retains the original draft', async () => {
  const update = vi
    .spyOn(work, 'updateTask')
    .mockResolvedValueOnce({ kind: 'conflict', current: fresh });
  await mount();
  const panel = await conflict();
  for (const label of ['제목', '설명'])
    await fireEvent.click(within(panel).getByRole('radio', { name: `${label}: 서버 최신 사용` }));
  await fireEvent.click(within(panel).getByRole('button', { name: '선택 적용 후 계속 편집' }));
  expect(screen.getByLabelText('설명')).toHaveValue('서버 설명');
  await edit('task-description', '직접 합친 설명');
  expect(update).toHaveBeenCalledTimes(1);
  await waitFor(() => {
    expect(Object.values(localStorage).join()).toContain('내 제목');
  });
  expect(Object.values(localStorage).join()).toContain('직접 합친 설명');
});

it('finishes explicitly when all chosen values already match the server without another PATCH', async () => {
  const update = vi
    .spyOn(work, 'updateTask')
    .mockResolvedValueOnce({ kind: 'conflict', current: fresh });
  await mount();
  const panel = await conflict();
  for (const label of ['제목', '설명'])
    await fireEvent.click(within(panel).getByRole('radio', { name: `${label}: 서버 최신 사용` }));
  await fireEvent.click(within(panel).getByRole('button', { name: '선택한 내용으로 저장' }));
  expect(screen.queryByRole('region', { name: '바뀐 내용 확인' })).toBeNull();
  expect(screen.getByLabelText('제목')).toHaveValue('서버 제목');
  expect(update).toHaveBeenCalledTimes(1);
});

it('reopens immediately on another 409 and preserves original and selected input after failure', async () => {
  const update = vi
    .spyOn(work, 'updateTask')
    .mockResolvedValueOnce({ kind: 'conflict', current: fresh })
    .mockResolvedValueOnce({
      kind: 'conflict',
      current: { ...fresh, version: 4, description: '세 번째 설명' },
    })
    .mockRejectedValueOnce(new ApiError(503, 'unavailable', 'failed'));
  await mount();
  let panel = await conflict();
  await fireEvent.click(within(panel).getByRole('radio', { name: '제목: 서버 최신 사용' }));
  await fireEvent.click(within(panel).getByRole('radio', { name: '설명: 내 입력 사용' }));
  await fireEvent.click(within(panel).getByRole('button', { name: '선택한 내용으로 저장' }));
  await waitFor(() => {
    expect(screen.getByDisplayValue('세 번째 설명')).toBeInTheDocument();
  });
  panel = screen.getByRole('region', { name: '바뀐 내용 확인' });
  expect(within(panel).getByRole('radio', { name: '설명: 내 입력 사용' })).not.toBeChecked();
  expect(update).toHaveBeenCalledTimes(2);
  await fireEvent.click(within(panel).getByRole('radio', { name: '설명: 내 입력 사용' }));
  await fireEvent.click(within(panel).getByRole('button', { name: '선택한 내용으로 저장' }));
  await waitFor(() => {
    expect(update).toHaveBeenCalledTimes(3);
  });
  expect(Object.values(localStorage).join()).toContain('내 제목');
  expect(Object.values(localStorage).join()).toContain('내 설명');
  await fireEvent.click(within(panel).getByRole('button', { name: '처음 입력 복원' }));
  expect(screen.getByLabelText('제목')).toHaveValue('내 제목');
  expect(screen.getByLabelText('설명')).toHaveValue('내 설명');
  expect(update).toHaveBeenCalledTimes(3);
});

it('hides the comparison on account change and stops writes after permission denial', async () => {
  const update = vi
    .spyOn(work, 'updateTask')
    .mockResolvedValueOnce({ kind: 'conflict', current: fresh })
    .mockRejectedValueOnce(new ApiError(403, 'forbidden', 'revoked'));
  await mount();
  const panel = await conflict();
  await fireEvent.click(within(panel).getByRole('radio', { name: '제목: 내 입력 사용' }));
  await fireEvent.click(within(panel).getByRole('radio', { name: '설명: 내 입력 사용' }));
  await fireEvent.click(within(panel).getByRole('button', { name: '선택한 내용으로 저장' }));
  await waitFor(() => {
    expect(update).toHaveBeenCalledTimes(2);
  });
  expect(within(panel).getByRole('button', { name: '선택한 내용으로 저장' })).toBeDisabled();
  me.userId = 'user-b';
  await waitFor(() => {
    expect(screen.queryByRole('region', { name: '바뀐 내용 확인' })).toBeNull();
  });
  expect(update).toHaveBeenCalledTimes(2);
});

it.each(['older', 'newer', 'agreed'] as const)(
  'keeps a late child response %s than the compared snapshot safe',
  async (kind) => {
    const listed = {
      ...base,
      checklist: [{ id: 'c1', text: '검토', checked: false }],
      checklist_total: 1,
    };
    const server = { ...listed, assignee_ids: [], version: 4, title: '서버 제목' };
    const reloaded = {
      ...listed,
      version: kind === 'older' ? 3 : 5,
      title: kind === 'older' ? base.title : kind === 'newer' ? '더 새 제목' : '내 제목',
      checklist: [{ id: 'c1', text: '검토', checked: true }],
    };
    let finish!: () => void;
    vi.spyOn(work, 'updateChecklistItem').mockReturnValueOnce(
      new Promise((resolve) => {
        finish = () => {
          resolve(undefined);
        };
      }),
    );
    const update = vi
      .spyOn(work, 'updateTask')
      .mockResolvedValueOnce({ kind: 'conflict', current: server });
    if (kind === 'older')
      update.mockResolvedValueOnce({ kind: 'updated', task: { ...reloaded, version: 9 } });
    else
      update
        .mockResolvedValueOnce({ kind: 'conflict', current: { ...reloaded, assignee_ids: [] } })
        .mockResolvedValueOnce({ kind: 'updated', task: { ...reloaded, version: 9 } });
    await mount(listed);
    await fireEvent.click(screen.getByRole('checkbox', { name: '미완료 항목: 검토' }));
    vi.mocked(work.task).mockResolvedValueOnce(reloaded);
    await edit('task-title', '내 제목');
    document.getElementById('task-save')?.click();
    await screen.findByRole('region', { name: '바뀐 내용 확인' });
    finish();
    await screen.findByRole('checkbox', { name: '완료 항목: 검토' });
    let panel = screen.getByRole('region', { name: '바뀐 내용 확인' });
    expect(update).toHaveBeenCalledTimes(1);
    expect(within(panel).getByRole('textbox', { name: '제목: 서버 최신' })).toHaveValue(
      '서버 제목',
    );
    await fireEvent.click(within(panel).getByRole('radio', { name: '제목: 내 입력 사용' }));
    await fireEvent.click(within(panel).getByRole('button', { name: '선택한 내용으로 저장' }));
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(2);
    });
    expect(update.mock.calls[1]?.[2]).toBe(4);
    if (kind !== 'older') {
      panel = screen.getByRole('region', { name: '바뀐 내용 확인' });
      if (kind === 'agreed')
        expect(within(panel).getByText('입력과 서버 최신값이 같습니다.')).toBeInTheDocument();
      else {
        expect(within(panel).getByRole('textbox', { name: '제목: 서버 최신' })).toHaveValue(
          '더 새 제목',
        );
        await fireEvent.click(within(panel).getByRole('radio', { name: '제목: 내 입력 사용' }));
      }
      await fireEvent.click(within(panel).getByRole('button', { name: '선택한 내용으로 저장' }));
    }
    await waitFor(() => {
      expect(screen.queryByRole('region', { name: '바뀐 내용 확인' })).toBeNull();
    });
    if (kind === 'newer') expect(update.mock.calls[2]?.[2]).toBe(5);
    else expect(update).toHaveBeenCalledTimes(2);
  },
);

it('shows normalized tags once and saves the chosen whole tag set', async () => {
  const server = { ...fresh, title: base.title, description: base.description, tags: ['server'] };
  const update = vi
    .spyOn(work, 'updateTask')
    .mockResolvedValueOnce({ kind: 'conflict', current: server })
    .mockResolvedValueOnce({ kind: 'updated', task: { ...server, version: 4, tags: ['a', 'b'] } });
  await mount();
  await edit('task-tags', ' a, a, b ');
  document.getElementById('task-save')?.click();
  const panel = await screen.findByRole('region', { name: '바뀐 내용 확인' });
  expect(within(panel).getByRole('textbox', { name: '태그(쉼표로 구분): 내 입력' })).toHaveValue(
    '"a", "b"',
  );
  await fireEvent.click(
    within(panel).getByRole('radio', { name: '태그(쉼표로 구분): 내 입력 사용' }),
  );
  await fireEvent.click(within(panel).getByRole('button', { name: '선택한 내용으로 저장' }));
  await waitFor(() => {
    expect(update).toHaveBeenCalledTimes(2);
  });
  expect(update.mock.calls[1]?.[3].tags).toEqual(['a', 'b']);
});

it('leaves comparison for an accessible successor approval and keeps the selected period and version', async () => {
  const server = { ...base, assignee_ids: [], version: 3, due_date: '2026-11-15' };
  const shift = {
    task_id: 'follower',
    reference: 'KED-2',
    title: '후속',
    base_version: 4,
    from_start_date: '2026-11-16',
    from_due_date: '2026-11-17',
    start_date: '2026-11-21',
    due_date: '2026-11-22',
    causes: [],
  };
  vi.spyOn(work, 'previewSchedule')
    .mockResolvedValueOnce({ shifts: [], undated: [], blocked: null })
    .mockResolvedValueOnce({ shifts: [shift], undated: [], blocked: null });
  const update = vi
    .spyOn(work, 'updateTask')
    .mockResolvedValueOnce({ kind: 'conflict', current: server })
    .mockResolvedValueOnce({
      kind: 'updated',
      task: { ...server, version: 4, due_date: '2026-11-20' },
    });
  await mount();
  await edit('task-due', '2026-11-20');
  document.getElementById('task-save')?.click();
  const panel = await screen.findByRole('region', { name: '바뀐 내용 확인' });
  await fireEvent.click(
    within(panel).getByRole('radio', { name: '일정(시작일·마감일·마일스톤): 내 입력 사용' }),
  );
  await fireEvent.click(within(panel).getByRole('button', { name: '선택한 내용으로 저장' }));
  const approve = await screen.findByRole('button', { name: '후속 업무도 옮겨 저장' });
  expect(approve.closest('section')?.parentElement).toHaveProperty('inert', false);
  expect(update).toHaveBeenCalledTimes(1);
  await fireEvent.click(approve);
  await waitFor(() => {
    expect(update).toHaveBeenCalledTimes(2);
  });
  expect(update.mock.calls[1]?.[2]).toBe(3);
  expect(update.mock.calls[1]?.[3]).toMatchObject({
    due_date: '2026-11-20',
    start_date: null,
    milestone: false,
  });
  expect(update.mock.calls[1]?.[4]).toEqual([
    { task_id: 'follower', base_version: 4, start_date: '2026-11-21', due_date: '2026-11-22' },
  ]);
});

it('explains text differences that look identical without changing either full value', async () => {
  vi.spyOn(work, 'updateTask').mockResolvedValueOnce({
    kind: 'conflict',
    current: { ...fresh, description: '내 설명\uFEFF' },
  });
  await mount();
  const panel = await conflict();
  expect(
    within(panel).getByText(/두 값이 같아 보이지만 저장되는 값은 다릅니다/),
  ).toBeInTheDocument();
  expect(within(panel).getByRole('textbox', { name: '설명: 내 입력' })).toHaveValue('내 설명');
  expect(within(panel).getByRole('textbox', { name: '설명: 서버 최신' })).toHaveValue(
    '내 설명\uFEFF',
  );
});

it('distinguishes assigned people with the same name using the existing candidate map', async () => {
  const first = '0190a000-0000-7000-8000-00000000a1a1';
  const second = '0190a000-0000-7000-8000-00000000b2b2';
  vi.mocked(work.assignable).mockResolvedValue([
    { user_id: first, display_name: 'Alex' },
    { user_id: second, display_name: 'Alex' },
  ]);
  const update = vi.spyOn(work, 'updateTask').mockResolvedValueOnce({
    kind: 'conflict',
    current: {
      ...fresh,
      title: base.title,
      description: base.description,
      assignee_ids: [second],
    },
  });
  await mount();
  await fireEvent.click(await screen.findByRole('checkbox', { name: 'Alex #a1a1' }));
  document.getElementById('task-save')?.click();
  const panel = await screen.findByRole('region', { name: '바뀐 내용 확인' });
  expect(
    within(panel).getByRole('textbox', { name: '담당자 (같은 완료 상태를 공유): 내 입력' }),
  ).toHaveValue('Alex #a1a1');
  expect(
    within(panel).getByRole('textbox', { name: '담당자 (같은 완료 상태를 공유): 서버 최신' }),
  ).toHaveValue('Alex #b2b2');
  expect(update).toHaveBeenCalledTimes(1);
});

it('keeps the 409 snapshot authoritative when a late composed child read mixes another version', async () => {
  const listed = {
    ...base,
    checklist: [{ id: 'c1', text: '검토', checked: false }],
    checklist_total: 1,
  };
  const server = { ...listed, version: 4, title: '409 제목', assignee_ids: ['current-person'] };
  const later = {
    ...listed,
    version: 5,
    title: '다른 시점 제목',
    assignees: [{ user_id: 'old-person', display_name: '다른 시점 담당' }],
    checklist: [{ id: 'c1', text: '검토', checked: true }],
  };
  let finish!: () => void;
  vi.spyOn(work, 'updateChecklistItem').mockReturnValueOnce(
    new Promise((resolve) => {
      finish = () => {
        resolve(undefined);
      };
    }),
  );
  const update = vi
    .spyOn(work, 'updateTask')
    .mockResolvedValueOnce({ kind: 'conflict', current: server })
    .mockResolvedValueOnce({
      kind: 'conflict',
      current: { ...later, assignee_ids: ['real-person'] },
    });
  await mount(listed);
  await fireEvent.click(screen.getByRole('checkbox', { name: '미완료 항목: 검토' }));
  vi.mocked(work.task).mockResolvedValueOnce(later);
  await edit('task-title', '내 제목');
  document.getElementById('task-save')?.click();
  await screen.findByRole('region', { name: '바뀐 내용 확인' });
  finish();
  await screen.findByRole('checkbox', { name: '완료 항목: 검토' });
  let panel = screen.getByRole('region', { name: '바뀐 내용 확인' });
  expect(within(panel).getByRole('textbox', { name: '제목: 서버 최신' })).toHaveValue('409 제목');
  await fireEvent.click(within(panel).getByRole('radio', { name: '제목: 내 입력 사용' }));
  await fireEvent.click(within(panel).getByRole('button', { name: '선택한 내용으로 저장' }));
  await waitFor(() => {
    expect(update).toHaveBeenCalledTimes(2);
  });
  expect(update.mock.calls[1]?.[2]).toBe(4);
  expect(update.mock.calls[1]?.[3]).toEqual({ title: '내 제목' });
  panel = screen.getByRole('region', { name: '바뀐 내용 확인' });
  expect(within(panel).getByRole('textbox', { name: '제목: 서버 최신' })).toHaveValue(
    '다른 시점 제목',
  );
  expect(within(panel).getByRole('radio', { name: '제목: 내 입력 사용' })).not.toBeChecked();
});

it('does not refetch status choices when applying or cancelling a failed comparison', async () => {
  vi.spyOn(work, 'updateTask')
    .mockResolvedValueOnce({ kind: 'conflict', current: fresh })
    .mockRejectedValueOnce(new ApiError(503, 'unavailable', 'failed'));
  await mount();
  const panel = await conflict();
  const before = vi.mocked(work.taskStatuses).mock.calls.length;
  for (const label of ['제목', '설명'])
    await fireEvent.click(within(panel).getByRole('radio', { name: `${label}: 내 입력 사용` }));
  await fireEvent.click(within(panel).getByRole('button', { name: '선택한 내용으로 저장' }));
  await waitFor(() => {
    expect(within(panel).getByRole('button', { name: '비교 닫고 입력 계속 편집' })).toBeEnabled();
  });
  await fireEvent.click(within(panel).getByRole('button', { name: '비교 닫고 입력 계속 편집' }));
  expect(vi.mocked(work.taskStatuses).mock.calls).toHaveLength(before);
});

it.each(['updated', 'conflict'] as const)(
  'ignores a late %s after the same task is closed and reopened',
  async (kind) => {
    let resolve!: (value: Awaited<ReturnType<typeof work.updateTask>>) => void;
    const pending = new Promise<Awaited<ReturnType<typeof work.updateTask>>>((yes) => {
      resolve = yes;
    });
    vi.spyOn(work, 'updateTask').mockReturnValueOnce(pending);
    const done = vi.spyOn(feedback, 'notifyDone');
    const first = await mount();
    await edit('task-title', '앞 세션 입력');
    document.getElementById('task-save')?.click();
    first.unmount();
    await mount({ ...base, title: '다시 연 업무' });
    screen.getByLabelText('제목').focus();
    resolve(kind === 'updated' ? { kind, task: fresh } : { kind, current: fresh });
    await pending;
    await waitFor(() => {
      expect(screen.getByLabelText('제목')).toHaveValue('다시 연 업무');
    });
    expect(screen.getByLabelText('제목')).toHaveFocus();
    expect(screen.queryByRole('region', { name: '바뀐 내용 확인' })).toBeNull();
    expect(done).not.toHaveBeenCalled();
  },
);

it('restores the kept paired snapshot and candidate instead of mixing a fresh detail read into them', async () => {
  const update = vi
    .spyOn(work, 'updateTask')
    .mockResolvedValueOnce({
      kind: 'conflict',
      current: { ...fresh, assignee_ids: ['paired-person'] },
    })
    .mockRejectedValueOnce(new ApiError(503, 'unavailable', 'failed'))
    .mockResolvedValueOnce({
      kind: 'conflict',
      current: {
        ...fresh,
        version: 9,
        title: '9판 제목',
        description: '9판 설명',
        assignee_ids: ['real-person'],
      },
    });
  const first = await mount();
  let panel = await conflict();
  await fireEvent.click(within(panel).getByRole('radio', { name: '제목: 서버 최신 사용' }));
  await fireEvent.click(within(panel).getByRole('radio', { name: '설명: 내 입력 사용' }));
  await fireEvent.click(within(panel).getByRole('button', { name: '선택한 내용으로 저장' }));
  await waitFor(() => {
    expect(within(panel).getByRole('button', { name: '비교 닫고 입력 계속 편집' })).toBeEnabled();
  });
  me.userId = 'user-b';
  await waitFor(() => {
    expect(screen.queryByRole('region', { name: '바뀐 내용 확인' })).toBeNull();
  });
  first.unmount();
  me.userId = 'user-a';
  await mount({
    ...base,
    version: 9,
    title: '9판 제목',
    description: '9판 설명',
    assignees: [{ user_id: 'old-person', display_name: '별도 조회 담당' }],
  });
  await fireEvent.click(await screen.findByRole('button', { name: '입력 복원' }));
  panel = await screen.findByRole('region', { name: '바뀐 내용 확인' });
  expect(within(panel).getByRole('textbox', { name: '설명: 서버 최신' })).toHaveValue('서버 설명');
  expect(within(panel).getByRole('textbox', { name: '설명: 내 입력' })).toHaveValue('내 설명');
  expect(update).toHaveBeenCalledTimes(2);
  await fireEvent.click(within(panel).getByRole('button', { name: '선택한 내용으로 저장' }));
  await waitFor(() => {
    expect(update).toHaveBeenCalledTimes(3);
  });
  expect(update.mock.calls[2]?.[2]).toBe(3);
  expect(update.mock.calls[2]?.[3]).toEqual({ description: '내 설명' });
  panel = screen.getByRole('region', { name: '바뀐 내용 확인' });
  expect(within(panel).getByRole('textbox', { name: '설명: 서버 최신' })).toHaveValue('9판 설명');
});

it('restores choices made before saving the comparison without sending them automatically', async () => {
  const update = vi.spyOn(work, 'updateTask').mockResolvedValueOnce({
    kind: 'conflict',
    current: fresh,
  });
  const first = await mount();
  let panel = await conflict();
  await fireEvent.click(within(panel).getByRole('radio', { name: '제목: 서버 최신 사용' }));
  await fireEvent.click(within(panel).getByRole('radio', { name: '설명: 내 입력 사용' }));
  me.userId = 'user-b';
  await waitFor(() => {
    expect(screen.queryByRole('region', { name: '바뀐 내용 확인' })).toBeNull();
  });
  first.unmount();
  me.userId = 'user-a';
  await mount();
  await fireEvent.click(await screen.findByRole('button', { name: '입력 복원' }));
  panel = await screen.findByRole('region', { name: '바뀐 내용 확인' });
  expect(within(panel).getByRole('radio', { name: '제목: 서버 최신 사용' })).toBeChecked();
  expect(within(panel).getByRole('radio', { name: '설명: 내 입력 사용' })).toBeChecked();
  expect(update).toHaveBeenCalledTimes(1);
});
