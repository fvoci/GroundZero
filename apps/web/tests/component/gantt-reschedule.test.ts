import * as feedback from '$lib/features/feedback/feedback.svelte';
import { org } from '$lib/org.svelte';
import './fixtures/task-statuses';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { ItemView, ShiftProposalView, TaskDetailView, TaskView } from '$lib/api/client';
import { api } from '$lib/api/client';
import { editDates } from '$lib/features/tasks/gantt';
import TaskBoard from '$lib/features/tasks/TaskBoard.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

// 간트 끌기로 일정 바꾸기: 최신 업무로 D-31 미리보기·승인을 거쳐 저장하고, 취소·충돌·그사이 바뀜이면 저장하지 않는다.
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

function task(
  id: string,
  title: string,
  start: string | null,
  due: string | null,
  version = 1,
): TaskView {
  return {
    id,
    item_id: 'i1',
    module_id: 'm1',
    workspace_id: 'ws-1',
    parent_id: null,
    reference: `KED-${id}`,
    title,
    description: '',
    status: 'todo',
    priority: 'none',
    start_date: start,
    due_date: due,
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
    version,
    move_version: version,
    completed_at: null,
    access: 'write',
    updated_at: '2026-10-21T00:00:00Z',
  };
}
const detail = (t: TaskView): TaskDetailView => ({ ...t, checklist: [] });
const design = task('a', '설계', '2026-10-20', '2026-10-22');
const build = task('b', '구현', '2026-10-23', '2026-10-25');
const shift: ShiftProposalView = {
  causes: [],
  task_id: 'b',
  reference: 'KED-b',
  title: '구현',
  base_version: 1,
  from_start_date: '2026-10-23',
  from_due_date: '2026-10-25',
  start_date: '2026-10-24',
  due_date: '2026-10-26',
};

describe('gantt date edits', () => {
  it('moves both dates, resizes either end, and refuses impossible ranges', () => {
    expect(editDates(design, 'move', 1)).toEqual({
      start_date: '2026-10-21',
      due_date: '2026-10-23',
    });
    expect(editDates(design, 'start', -2)).toEqual({
      start_date: '2026-10-18',
      due_date: '2026-10-22',
    });
    expect(editDates(design, 'end', 3)).toEqual({
      start_date: '2026-10-20',
      due_date: '2026-10-25',
    });
    expect(editDates(design, 'end', -3)).toBeUndefined();
    expect(editDates(design, 'move', 0)).toBeUndefined();
    // 마감만 있는 업무를 옮기면 마감만, 끝을 끌면 기간이 된다.
    const dueOnly = task('c', '점검', null, '2026-10-22');
    expect(editDates(dueOnly, 'move', -1)).toEqual({ start_date: null, due_date: '2026-10-21' });
    expect(editDates(dueOnly, 'start', -2)).toEqual({
      start_date: '2026-10-20',
      due_date: '2026-10-22',
    });
    expect(editDates({ ...dueOnly, milestone: true }, 'end', 1)).toBeUndefined();
  });
});

describe('gantt reschedule', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
    window.sessionStorage.setItem(org.key('board:i1'), JSON.stringify({ view: 'gantt' }));
    vi.spyOn(api, 'savedViews').mockResolvedValue([]);
    vi.spyOn(api, 'syncRecurrences').mockResolvedValue(0);
    Element.prototype.setPointerCapture = (): void => undefined;
  });
  afterEach(() => {
    vi.restoreAllMocks();
    window.sessionStorage.clear();
  });

  async function mount(): Promise<HTMLElement> {
    render(TaskBoard, { props: { item, workspaces: [], openTaskId: undefined } });
    const gantt = await screen.findByTestId('task-gantt');
    return gantt;
  }
  const rowButton = (gantt: HTMLElement, id: string): HTMLElement => {
    const el = gantt.querySelector<HTMLElement>(`[data-task-card="${id}"]`);
    if (el === null) throw new Error(id);
    return el;
  };

  it('Alt+Right moves the task a day and saves when no later task is affected', async () => {
    const done = vi.spyOn(feedback, 'notifyDone');
    vi.spyOn(api, 'tasks').mockResolvedValue([design, build]);
    vi.spyOn(api, 'task').mockResolvedValue(detail(design));
    const preview = vi
      .spyOn(api, 'previewSchedule')
      .mockResolvedValue({ shifts: [], undated: [], blocked: null });
    const update = vi.spyOn(api, 'updateTask').mockResolvedValue({
      kind: 'updated',
      task: detail(task('a', '설계', '2026-10-21', '2026-10-23', 2)),
    });
    const gantt = await mount();
    await fireEvent.keyDown(rowButton(gantt, 'a'), { key: 'ArrowRight', altKey: true });
    await waitFor(() => {
      expect(update).toHaveBeenCalled();
    });
    await waitFor(() => {
      expect(done).toHaveBeenCalledWith(
        expect.stringContaining('밀릴 업무가 없어 그대로 저장했습니다.'),
      );
    });
    expect(preview).toHaveBeenCalledWith('a', '2026-10-21', '2026-10-23');
    expect(update.mock.calls[0]?.[2]).toBe(1);
    expect(update.mock.calls[0]?.[3]).toMatchObject({
      start_date: '2026-10-21',
      due_date: '2026-10-23',
    });
    expect(update.mock.calls[0]?.[4]).toEqual([]);
  });

  it('asks before moving later tasks and saves them together when approved', async () => {
    vi.spyOn(api, 'tasks').mockResolvedValue([design, build]);
    vi.spyOn(api, 'task').mockResolvedValue(detail(design));
    vi.spyOn(api, 'previewSchedule').mockResolvedValue({
      shifts: [shift],
      undated: [],
      blocked: null,
    });
    const update = vi.spyOn(api, 'updateTask').mockResolvedValue({
      kind: 'updated',
      task: detail(task('a', '설계', '2026-10-20', '2026-10-23', 2)),
    });
    const gantt = await mount();
    await fireEvent.keyDown(rowButton(gantt, 'a'), {
      key: 'ArrowRight',
      altKey: true,
      shiftKey: true,
    });
    const dialog = await screen.findByRole('dialog');
    expect(within(dialog).getByText(/2026-10-20~2026-10-22 → 2026-10-20~2026-10-23/)).toBeTruthy();
    expect(
      within(dialog).getByText(/구현: 2026-10-23~2026-10-25 → 2026-10-24~2026-10-26/),
    ).toBeTruthy();
    expect(update).not.toHaveBeenCalled();
    await fireEvent.click(within(dialog).getByRole('button', { name: '후속 업무도 옮겨 저장' }));
    await waitFor(() => {
      expect(update).toHaveBeenCalled();
    });
    expect(update.mock.calls[0]?.[4]).toEqual([
      { task_id: 'b', base_version: 1, start_date: '2026-10-24', due_date: '2026-10-26' },
    ]);
    await waitFor(() => {
      expect(screen.queryByRole('dialog')).toBeNull();
    });
    // 창이 닫히면 그 업무의 간트 줄로 초점이 돌아온다.
    await waitFor(() => {
      expect(document.activeElement).toBe(rowButton(gantt, 'a'));
    });
  });

  it('says before approval when more followers would move than one save allows', async () => {
    vi.spyOn(api, 'tasks').mockResolvedValue([design, build]);
    vi.spyOn(api, 'task').mockResolvedValue(detail(design));
    vi.spyOn(api, 'previewSchedule').mockResolvedValue({
      shifts: [shift],
      undated: [],
      blocked: {
        reason: 'too_many_shifts',
        shift_count: 501,
        max_shifts: 500,
        task_id: null,
        reference: null,
        date: null,
      },
    });
    const update = vi.spyOn(api, 'updateTask').mockResolvedValue({
      kind: 'updated',
      task: detail(task('a', '설계', '2026-10-20', '2026-10-23', 2)),
    });
    const gantt = await mount();
    await fireEvent.keyDown(rowButton(gantt, 'a'), {
      key: 'ArrowRight',
      altKey: true,
      shiftKey: true,
    });
    const dialog = await screen.findByRole('dialog');
    expect(within(dialog).getByTestId('shift-blocked').textContent).toBe(
      '옮길 후속 업무가 501개로 한 번에 옮길 수 있는 500개를 넘어 함께 저장할 수 없습니다. 후속 업무를 옮기지 않고 저장하거나 취소하세요.',
    );
    const all = within(dialog).getByRole('button', { name: '후속 업무도 옮겨 저장' });
    expect(all).toHaveProperty('disabled', true);
    await fireEvent.click(all);
    expect(update).not.toHaveBeenCalled();
    // 일부만 옮기지 않는다: 이 업무만 저장하면 후속 목록은 비어 있다.
    await fireEvent.click(within(dialog).getByRole('button', { name: '이 업무만 저장' }));
    await waitFor(() => {
      expect(update).toHaveBeenCalled();
    });
    expect(update.mock.calls[0]?.[4]).toEqual([]);
  });

  it('cannot be cancelled once the change is being saved', async () => {
    vi.spyOn(api, 'tasks').mockResolvedValue([design, build]);
    vi.spyOn(api, 'task').mockResolvedValue(detail(design));
    vi.spyOn(api, 'previewSchedule').mockResolvedValue({
      shifts: [shift],
      undated: [],
      blocked: null,
    });
    let finish: (v: Awaited<ReturnType<typeof api.updateTask>>) => void = () => undefined;
    const update = vi.spyOn(api, 'updateTask').mockReturnValue(
      new Promise((resolve) => {
        finish = resolve;
      }),
    );
    const gantt = await mount();
    await fireEvent.keyDown(rowButton(gantt, 'a'), { key: 'ArrowRight', altKey: true });
    const dialog = await screen.findByRole('dialog');
    await fireEvent.click(within(dialog).getByRole('button', { name: '후속 업무도 옮겨 저장' }));
    expect(update).toHaveBeenCalled();
    // 저장을 보낸 뒤에는 취소 단추가 꺼지고 Esc로도 닫히지 않는다(닫으면 "바꾸지 않았다"는 안내가 사실과 달라진다).
    expect(within(dialog).getByRole('button', { name: '취소' })).toHaveProperty('disabled', true);
    await fireEvent.keyDown(document.activeElement ?? dialog, { key: 'Escape' });
    expect(screen.getByRole('dialog')).toBeTruthy();
    expect(screen.queryByText(/일정을 바꾸지 않았습니다/)).toBeNull();
    finish({ kind: 'updated', task: detail(task('a', '설계', '2026-10-21', '2026-10-23', 2)) });
    await waitFor(() => {
      expect(screen.queryByRole('dialog')).toBeNull();
    });
  });

  it('saves nothing when cancelled and reads the list again', async () => {
    const read = vi.spyOn(api, 'tasks').mockResolvedValue([design, build]);
    vi.spyOn(api, 'task').mockResolvedValue(detail(design));
    vi.spyOn(api, 'previewSchedule').mockResolvedValue({
      shifts: [shift],
      undated: [],
      blocked: null,
    });
    const update = vi.spyOn(api, 'updateTask');
    const gantt = await mount();
    await fireEvent.keyDown(rowButton(gantt, 'a'), { key: 'ArrowRight', altKey: true });
    const dialog = await screen.findByRole('dialog');
    await fireEvent.click(within(dialog).getByRole('button', { name: '취소' }));
    await waitFor(() => {
      expect(read).toHaveBeenCalledTimes(2);
    });
    expect(update).not.toHaveBeenCalled();
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('does not save when the dates changed elsewhere, or when the task changed while asking', async () => {
    vi.spyOn(api, 'tasks').mockResolvedValue([design, build]);
    const read = vi
      .spyOn(api, 'task')
      .mockResolvedValueOnce(detail(task('a', '설계', '2026-10-21', '2026-10-24', 2)))
      .mockResolvedValue(detail(design));
    vi.spyOn(api, 'previewSchedule').mockResolvedValue({ shifts: [], undated: [], blocked: null });
    const update = vi.spyOn(api, 'updateTask').mockResolvedValue({
      kind: 'conflict',
      current: detail(task('a', '설계', '2026-10-20', '2026-10-22', 3)),
    });
    const gantt = await mount();
    await fireEvent.keyDown(rowButton(gantt, 'a'), { key: 'ArrowRight', altKey: true });
    expect((await screen.findByRole('alert')).textContent).toContain(
      '그사이 다른 곳에서 일정이 바뀌어',
    );
    expect(update).not.toHaveBeenCalled();
    await fireEvent.keyDown(rowButton(gantt, 'a'), { key: 'ArrowRight', altKey: true });
    await waitFor(() => {
      expect(screen.getByRole('alert').textContent).toContain(
        '그사이 다른 곳에서 업무가 바뀌어 저장하지 않았습니다',
      );
    });
    expect(read).toHaveBeenCalledTimes(2);
  });

  it('dragging the bar by two columns moves it two days; Escape cancels a drag', async () => {
    vi.spyOn(api, 'tasks').mockResolvedValue([design, build]);
    vi.spyOn(api, 'task').mockResolvedValue(detail(design));
    vi.spyOn(api, 'previewSchedule').mockResolvedValue({ shifts: [], undated: [], blocked: null });
    const update = vi.spyOn(api, 'updateTask').mockResolvedValue({
      kind: 'updated',
      task: detail(task('a', '설계', '2026-10-22', '2026-10-24', 2)),
    });
    const gantt = await mount();
    const bar = gantt.querySelector<HTMLElement>('[data-gantt-bar="a"]');
    if (bar === null) throw new Error('bar');
    const px =
      1.75 * (Number.parseFloat(getComputedStyle(document.documentElement).fontSize) || 16);
    await fireEvent.pointerDown(bar, { pointerId: 1, button: 0, buttons: 1, clientX: 100 });
    await fireEvent.pointerMove(bar, { pointerId: 1, buttons: 1, clientX: 100 + px });
    await fireEvent.keyDown(window, { key: 'Escape' });
    await fireEvent.pointerUp(bar, { pointerId: 1, clientX: 100 + px });
    expect(update).not.toHaveBeenCalled();
    await fireEvent.pointerDown(bar, { pointerId: 2, button: 0, buttons: 1, clientX: 100 });
    await fireEvent.pointerMove(bar, { pointerId: 2, buttons: 1, clientX: 100 + 2 * px });
    await fireEvent.pointerUp(bar, { pointerId: 2, clientX: 100 + 2 * px });
    await waitFor(() => {
      expect(update).toHaveBeenCalled();
    });
    expect(update.mock.calls[0]?.[3]).toMatchObject({
      start_date: '2026-10-22',
      due_date: '2026-10-24',
    });
  });

  it('offers nothing to drag when the board is read-only', async () => {
    vi.spyOn(api, 'tasks').mockResolvedValue([{ ...design, access: 'read' }]);
    render(TaskBoard, {
      props: { item: { ...item, access: 'read' }, workspaces: [], openTaskId: undefined },
    });
    const gantt = await screen.findByTestId('task-gantt');
    expect(gantt.querySelector('[data-gantt-edge]')).toBeNull();
    expect(screen.queryByText(/막대를 끌어 일정을 옮기고/)).toBeNull();
  });
});
