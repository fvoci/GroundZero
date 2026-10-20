import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import type { MockInstance } from 'vitest';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { ItemView, TaskView } from '$lib/api/client';
import { api } from '$lib/api/client';
import TaskBoard from '$lib/features/tasks/TaskBoard.svelte';
import TaskDetail from '$lib/features/tasks/TaskDetail.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

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
  access: 'write',
  archived: false,
  updated_at: '2026-10-19T00:00:00Z',
  status: 'active',
  is_template: false,
  starts_on: null,
  ends_on: null,
};

function task(id: string, title: string): TaskView {
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
    start_date: null,
    due_date: null,
    estimate_minutes: null,
    tags: [],
    milestone: false,
    predecessor_ids: [],
    series: null,
    occurrence_date: null,
    assignees: [],
    checklist_done: 0,
    checklist_total: 0,
    version: 1,
    completed_at: null,
    access: 'write',
    updated_at: '2026-10-19T00:00:00Z',
  };
}

// 칸반 맥락(목표 3): 카드를 열었다 닫은 뒤 브라우저 뒤로 가기는 보드를 떠나야 한다. 닫기가 새 기록을 쌓으면 뒤로 가기가
// 방금 닫은 카드를 다시 연다.
describe('task dialog and browser history', () => {
  it('closing a card opened from the board goes back instead of adding an entry', async () => {
    await setLocale('ko', { reload: false });
    window.history.replaceState(null, '', '/w/ws-1/i/i1');
    vi.spyOn(api, 'tasks').mockResolvedValue([task('2', 'API 연결')]);
    vi.spyOn(api, 'savedViews').mockResolvedValue([]);
    vi.spyOn(api, 'syncRecurrences').mockResolvedValue(0);
    vi.spyOn(api, 'task').mockReturnValue(new Promise(() => undefined));
    vi.spyOn(api, 'assignable').mockResolvedValue([]);
    const push = vi.spyOn(window.history, 'pushState');
    const back = vi.spyOn(window.history, 'back').mockImplementation(() => undefined);

    const view = render(TaskBoard, {
      props: { item, workspaces: [], openTaskId: undefined },
    });
    await fireEvent.click(await screen.findByRole('button', { name: /^KED-2\s*API 연결$/ }));
    expect(push).toHaveBeenCalledTimes(1);
    // 앱에서는 주소가 바뀌면 부모가 연 업무를 넘긴다.
    await view.rerender({ item, workspaces: [], openTaskId: '2' });
    await waitFor(() => {
      expect(screen.getByRole('dialog')).toBeTruthy();
    });
    await fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
    await waitFor(() => {
      expect(back).toHaveBeenCalledTimes(1);
    });
    expect(push).toHaveBeenCalledTimes(1);
  });
});

// 칸반 맥락(목표 3): 다른 화면에 갔다가 돌아와도(보드가 새로 그려짐) 보기·필터·정렬이 그대로다.
describe('board context across navigation', () => {
  it('restores the view and filter when the board is shown again', async () => {
    await setLocale('ko', { reload: false });
    window.sessionStorage.clear();
    vi.spyOn(api, 'tasks').mockResolvedValue([task('2', 'API 연결')]);
    vi.spyOn(api, 'savedViews').mockResolvedValue([]);
    vi.spyOn(api, 'syncRecurrences').mockResolvedValue(0);
    const first = render(TaskBoard, { props: { item, workspaces: [], openTaskId: undefined } });
    const status = await screen.findByLabelText('상태');
    await fireEvent.change(status, { target: { value: 'in_progress' } });
    await fireEvent.click(screen.getByRole('button', { name: '목록' }));
    first.unmount();

    render(TaskBoard, { props: { item, workspaces: [], openTaskId: undefined } });
    const again = await screen.findByLabelText('상태');
    expect(again instanceof HTMLSelectElement ? again.value : '').toBe('in_progress');
    expect(screen.getByRole('button', { name: '목록' }).getAttribute('aria-pressed')).toBe('true');
  });

  // 같은 보드가 다른 항목으로 바뀌면 그 항목의 맥락을 쓴다. 기억한 것이 없으면 기본값이고, 앞 항목의 필터가 남아
  // 새 항목 이름으로 저장되지 않는다.
  it('opens another item without its own context at the defaults', async () => {
    await setLocale('ko', { reload: false });
    window.sessionStorage.clear();
    vi.spyOn(api, 'tasks').mockResolvedValue([task('2', 'API 연결')]);
    vi.spyOn(api, 'savedViews').mockResolvedValue([]);
    vi.spyOn(api, 'syncRecurrences').mockResolvedValue(0);
    const view = render(TaskBoard, { props: { item, workspaces: [], openTaskId: undefined } });
    await fireEvent.change(await screen.findByLabelText('상태'), {
      target: { value: 'in_progress' },
    });
    await fireEvent.click(screen.getByRole('button', { name: '목록' }));

    const other = { ...item, id: 'i2', reference: 'KED-9' };
    await view.rerender({ item: other, workspaces: [], openTaskId: undefined });
    await waitFor(() => {
      const status = screen.getByLabelText('상태');
      expect(status instanceof HTMLSelectElement ? status.value : 'x').toBe('any');
    });
    expect(screen.getByRole('button', { name: '칸반' }).getAttribute('aria-pressed')).toBe('true');
    const saved: unknown = JSON.parse(window.sessionStorage.getItem('fvoci:board:i2') ?? 'null');
    expect(JSON.stringify(saved)).not.toContain('in_progress');
  });
});

// 칸반(목표 3): 끌기·키보드 말고도 카드 메뉴로 다른 열로 옮길 수 있다(같은 이동 저장 경로).
describe('move menu', () => {
  it('moves a card to the chosen column through the same move call', async () => {
    await setLocale('ko', { reload: false });
    window.sessionStorage.clear();
    vi.spyOn(api, 'tasks').mockResolvedValue([task('2', 'API 연결')]);
    vi.spyOn(api, 'savedViews').mockResolvedValue([]);
    vi.spyOn(api, 'syncRecurrences').mockResolvedValue(0);
    const move = vi
      .spyOn(api, 'moveTask')
      .mockResolvedValue({ ...task('2', 'API 연결'), checklist: [] });
    render(TaskBoard, { props: { item, workspaces: [], openTaskId: undefined } });
    await fireEvent.click(await screen.findByRole('button', { name: 'API 연결 옮기기' }));
    await fireEvent.click(await screen.findByRole('menuitem', { name: '완료로' }));
    await waitFor(() => {
      expect(move).toHaveBeenCalledWith('2', 'done', null);
    });
  });
});

// 칸반(목표 3): 열마다 빠르게 더하면 그 열의 상태로 만든다(만든 뒤 같은 이동 저장 경로로 옮김).
describe('quick add in a column', () => {
  it('creates the task and puts it in that column', async () => {
    await setLocale('ko', { reload: false });
    window.sessionStorage.clear();
    vi.spyOn(api, 'tasks').mockResolvedValue([]);
    vi.spyOn(api, 'savedViews').mockResolvedValue([]);
    vi.spyOn(api, 'syncRecurrences').mockResolvedValue(0);
    const create = vi
      .spyOn(api, 'createTask')
      .mockResolvedValue({ ...task('9', '새 업무'), checklist: [] });
    const move = vi
      .spyOn(api, 'moveTask')
      .mockResolvedValue({ ...task('2', 'API 연결'), checklist: [] });
    render(TaskBoard, { props: { item, workspaces: [], openTaskId: undefined } });
    const lane = await screen.findByRole('region', { name: /진행 중/ });
    await fireEvent.click(within(lane).getByRole('button', { name: '업무 추가' }));
    const input = within(lane).getByRole('textbox');
    await fireEvent.input(input, { target: { value: '새 업무' } });
    await fireEvent.submit(input.closest('form') ?? input);
    await waitFor(() => {
      expect(move).toHaveBeenCalledWith('9', 'in_progress', null);
    });
    expect(create.mock.calls[0]?.[2]).toBe('새 업무');
  });
});

function boardMocks(list: TaskView[] | Promise<TaskView[]>): void {
  window.sessionStorage.clear();
  vi.spyOn(api, 'tasks').mockReturnValue(Promise.resolve(list));
  vi.spyOn(api, 'savedViews').mockResolvedValue([]);
  vi.spyOn(api, 'syncRecurrences').mockResolvedValue(0);
}

// 빠른 추가(Kaneo·Focalboard 비교): 누른 단추가 입력란으로 바뀌므로 초점이 그 입력란으로 가야 키보드로 이어 쓸 수 있고,
// Esc로 닫으면 같은 열의 "업무 추가" 단추로 돌아와야 한다. 다른 열에 만든 업무의 알림에는 그 제목이 들어간다.
describe('quick add focus, cancel and notice', () => {
  it('focuses the input, closes on Escape back to the button, and names the task it moved', async () => {
    await setLocale('ko', { reload: false });
    boardMocks([]);
    vi.spyOn(api, 'createTask').mockResolvedValue({ ...task('9', '새 업무'), checklist: [] });
    vi.spyOn(api, 'moveTask').mockResolvedValue({ ...task('9', '새 업무'), checklist: [] });
    render(TaskBoard, { props: { item, workspaces: [], openTaskId: undefined } });
    const lane = await screen.findByRole('region', { name: /진행 중/ });
    await fireEvent.click(within(lane).getByRole('button', { name: '업무 추가' }));
    const input = within(lane).getByRole('textbox');
    await waitFor(() => {
      expect(document.activeElement).toBe(input);
    });
    await fireEvent.keyDown(input, { key: 'Escape' });
    await waitFor(() => {
      expect(within(lane).queryByRole('textbox')).toBeNull();
      expect(document.activeElement).toBe(within(lane).getByRole('button', { name: '업무 추가' }));
    });

    await fireEvent.click(within(lane).getByRole('button', { name: '업무 추가' }));
    const again = within(lane).getByRole('textbox');
    await fireEvent.input(again, { target: { value: '새 업무' } });
    await fireEvent.submit(again.closest('form') ?? again);
    // 전에는 만든 업무가 아직 목록에 없어 ": 옮겼습니다(진행 중)."처럼 제목이 비었다.
    expect(await screen.findByText('새 업무: 옮겼습니다(진행 중).')).toBeTruthy();
  });
});

// 첫 목록을 받기 전에는 빈 열·"업무가 없습니다"가 아니라 불러오는 중이라고 보인다.
describe('board loading state', () => {
  it('says it is loading until the first list arrives', async () => {
    await setLocale('ko', { reload: false });
    let resolve: (list: TaskView[]) => void = () => undefined;
    boardMocks(new Promise<TaskView[]>((r) => (resolve = r)));
    render(TaskBoard, { props: { item, workspaces: [], openTaskId: undefined } });
    expect(await screen.findByText('불러오는 중…')).toBeTruthy();
    expect(screen.queryByTestId('task-board')).toBeNull();
    resolve([task('2', 'API 연결')]);
    expect(await screen.findByTestId('task-board')).toBeTruthy();
    expect(screen.queryByText('불러오는 중…')).toBeNull();
  });
});

// 끌기 중 창을 떠나거나(blur) 다른 곳에서 단추를 놓아 pointerup을 받지 못하면 끌기를 취소한다. 그러지 않으면 남은
// 끌기가 다음 클릭에서 카드를 놓아 버린다.
describe('drag cancel when the release is not seen', () => {
  async function startDragOver(
    laneName: RegExp,
  ): Promise<{ move: MockInstance<typeof api.moveTask> }> {
    await setLocale('ko', { reload: false });
    boardMocks([task('2', 'API 연결')]);
    const move = vi
      .spyOn(api, 'moveTask')
      .mockResolvedValue({ ...task('2', 'API 연결'), checklist: [] });
    render(TaskBoard, { props: { item, workspaces: [], openTaskId: undefined } });
    await screen.findByText('API 연결');
    const target = await screen.findByRole('region', { name: laneName });
    // jsdom에는 배치가 없으므로 포인터 아래 요소를 그 열로 정한다.
    Object.defineProperty(document, 'elementFromPoint', {
      configurable: true,
      value: () => target,
    });
    const card = document.querySelector('[data-board-card="2"]');
    if (card === null) throw new Error('card');
    await fireEvent.pointerDown(card, {
      pointerId: 1,
      isPrimary: true,
      button: 0,
      pointerType: 'mouse',
      clientX: 10,
      clientY: 10,
    });
    await fireEvent.pointerMove(window, {
      pointerId: 1,
      buttons: 1,
      pointerType: 'mouse',
      clientX: 60,
      clientY: 10,
    });
    return { move };
  }

  it('drops normally while the button is held (contrast)', async () => {
    const { move } = await startDragOver(/완료/);
    await fireEvent.pointerUp(window, {
      pointerId: 1,
      pointerType: 'mouse',
      clientX: 60,
      clientY: 10,
    });
    await waitFor(() => {
      expect(move).toHaveBeenCalledWith('2', 'done', null);
    });
  });

  it('cancels when the window loses focus mid-drag', async () => {
    const { move } = await startDragOver(/완료/);
    await fireEvent.blur(window);
    await fireEvent.pointerUp(window, {
      pointerId: 1,
      pointerType: 'mouse',
      clientX: 60,
      clientY: 10,
    });
    expect(await screen.findByText('API 연결: 끌기를 취소했습니다.')).toBeTruthy();
    expect(move).not.toHaveBeenCalled();
  });

  it('cancels when a move arrives with no button pressed', async () => {
    const { move } = await startDragOver(/완료/);
    await fireEvent.pointerMove(window, {
      pointerId: 1,
      buttons: 0,
      pointerType: 'mouse',
      clientX: 70,
      clientY: 10,
    });
    await fireEvent.pointerUp(window, {
      pointerId: 1,
      pointerType: 'mouse',
      clientX: 70,
      clientY: 10,
    });
    expect(move).not.toHaveBeenCalled();
  });
});

// 업무 상세: 지정할 수 있는 사람을 받기 전에는 "없습니다"를 보이지 않는다.
describe('task detail assignees while loading', () => {
  it('shows loading instead of the empty message until the people arrive', async () => {
    await setLocale('ko', { reload: false });
    vi.spyOn(api, 'task').mockResolvedValue({ ...task('2', 'API 연결'), checklist: [] });
    let resolve: (p: { user_id: string; display_name: string }[]) => void = () => undefined;
    vi.spyOn(api, 'assignable').mockReturnValue(new Promise((r) => (resolve = r)));
    vi.spyOn(api, 'taskBacklinks').mockResolvedValue([]);
    vi.spyOn(api, 'comments').mockResolvedValue([]);
    render(TaskDetail, {
      props: {
        taskId: '2',
        tasks: [task('2', 'API 연결')],
        workspaces: [],
        onchanged: () => undefined,
        onopen: () => undefined,
      },
    });
    const group = await screen.findByRole('group', { name: /담당자/ });
    expect(within(group).getByText('불러오는 중…')).toBeTruthy();
    expect(within(group).queryByText('지정할 수 있는 사람이 없습니다.')).toBeNull();
    resolve([{ user_id: 'u1', display_name: '가상 사용자' }]);
    expect(await within(group).findByText('가상 사용자')).toBeTruthy();
  });
});
