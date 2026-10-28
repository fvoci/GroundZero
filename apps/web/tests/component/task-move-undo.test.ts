import { taskData as work } from '$lib/features/tasks/detail-data';
import './fixtures/task-statuses';
import { fireEvent, render, screen } from '@testing-library/svelte';
import { tick } from 'svelte';
import type { MockInstance } from 'vitest';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { ItemView, TaskStatus, TaskView } from '$lib/api/client';
import { api, ApiError, NetworkError } from '$lib/api/client';
import type { DoneAction } from '$lib/features/feedback/feedback.svelte';
import * as feedback from '$lib/features/feedback/feedback.svelte';
import TaskBoard from '$lib/features/tasks/TaskBoard.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

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

function task(id: string, title: string, status: TaskStatus = '할 일', version = 1): TaskView {
  return {
    id,
    item_id: 'i1',
    module_id: 'm1',
    workspace_id: 'ws-1',
    parent_id: null,
    reference: `KED-${id}`,
    title,
    description: '',
    status,
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
    version,
    move_version: version,
    completed_at: null,
    access: 'write',
    updated_at: '2026-10-21T00:00:00Z',
  };
}

// 이동 알림의 되돌리기: 확정된 이동에만 내놓고, 보내기 전 그 카드가 이 이동의 결과 그대로일 때만 보내며, 끼어든 변경은 알린다.
// 이동 완료와 되돌리기는 완료 토스트(key `board-move`)와 그 동작 단추다(D-82). 토스트는 FeedbackHost가 그리므로 여기서는
// notifyDone·dismissDone이 남긴 토스트 하나를 흉내 낸다.
describe('kanban undo', () => {
  it('prevents the toast default while a save is still pending (#163)', async () => {
    let resolve: (value: Awaited<ReturnType<typeof work.moveTask>>) => void = () => undefined;
    lists(start, moved);
    vi.spyOn(work, 'moveTask')
      .mockResolvedValueOnce({ ...movedA, checklist: [] })
      .mockImplementationOnce(
        () =>
          new Promise((r) => {
            resolve = r;
          }),
      );
    await mount();
    await moveRight('a');
    await flush();
    const action = toast?.action;
    const pending = moveRight('a');
    await flush();
    const event = new MouseEvent('click', { cancelable: true });
    action?.onClick(event);
    expect(event.defaultPrevented).toBe(true);
    resolve({ ...movedA, checklist: [] });
    await pending;
  });
  let toast: { message: string; action: DoneAction | undefined } | undefined;
  let done: MockInstance<typeof feedback.notifyDone>;
  beforeEach(async () => {
    vi.useFakeTimers();
    await setLocale('ko', { reload: false });
    window.sessionStorage.clear();
    vi.spyOn(api, 'savedViews').mockResolvedValue([]);
    vi.spyOn(work, 'syncRecurrences').mockResolvedValue(0);
    toast = undefined;
    done = vi.spyOn(feedback, 'notifyDone').mockImplementation((message, key, options) => {
      if (key === 'board-move') toast = { message, action: options?.action };
    });
    vi.spyOn(feedback, 'dismissDone').mockImplementation((key) => {
      if (key === 'board-move') toast = undefined;
    });
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  const flush = async (ms = 0): Promise<void> => {
    await vi.advanceTimersByTimeAsync(ms);
    await tick();
  };
  const start = [task('a', 'API 연결'), task('b', '문서 검토')];
  const movedA = task('a', 'API 연결', '진행 중', 2);
  const moved = [task('b', '문서 검토'), movedA];

  function lists(
    ...answers: (TaskView[] | Promise<TaskView[]>)[]
  ): MockInstance<typeof work.tasks> {
    let i = 0;
    return vi.spyOn(work, 'tasks').mockImplementation(() => {
      const answer = answers[Math.min(i, answers.length - 1)] ?? [];
      i += 1;
      return answer instanceof Promise ? answer : Promise.resolve(answer);
    });
  }
  const card = (id: string): HTMLElement => {
    const el = document.querySelector<HTMLElement>(`[data-task-card="${id}"]`);
    if (el === null) throw new Error(`card ${id}`);
    return el;
  };
  const laneOf = (id: string): string | undefined =>
    card(id).closest<HTMLElement>('[data-lane]')?.dataset.lane;
  const moveRight = async (id: string): Promise<void> => {
    await fireEvent.keyDown(card(id), { key: 'ArrowRight', altKey: true });
    await tick();
  };
  const liveText = (): string =>
    [...document.querySelectorAll('[aria-live="polite"]')].map((e) => e.textContent).join(' ');
  const undoOffered = (): boolean => toast?.action !== undefined;
  /** 토스트의 되돌리기 단추를 누른다. 토스트 단추는 누르면 토스트가 닫힌다. */
  async function clickUndo(): Promise<void> {
    const action = toast?.action;
    if (action === undefined) throw new Error('undo');
    toast = undefined;
    const event = new MouseEvent('click', { cancelable: true });
    action.onClick(event);
    if (event.defaultPrevented) toast = { message: '', action };
    await tick();
  }
  const toasted = (text: string): boolean => done.mock.calls.some(([m]) => m.includes(text));

  async function mount(): Promise<void> {
    render(TaskBoard, { props: { item, workspaces: [], openTaskId: undefined } });
    await flush();
  }

  it('offers undo after a confirmed move and moves the card back to where it was', async () => {
    const read = lists(start, moved, moved, start);
    const move = vi
      .spyOn(work, 'moveTask')
      .mockResolvedValueOnce({ ...movedA, checklist: [] })
      .mockResolvedValueOnce({ ...task('a', 'API 연결', '할 일', 3), checklist: [] });
    await mount();
    await moveRight('a');
    await flush();
    // 이동 완료는 되돌리기 단추가 달린 토스트 하나다. 화면 안(알림 영역 포함) 완료 문구는 없다.
    expect(toast?.message).toBe('API 연결: 오른쪽 열로 옮겼습니다(진행 중).');
    expect(undoOffered()).toBe(true);
    expect(toast?.action?.label).toBe('되돌리기');
    expect(liveText()).not.toContain('옮겼습니다');
    expect(document.querySelector('[data-move-undo]')).toBeNull();
    await clickUndo();
    await flush();
    // 보내기 전에 서버를 다시 읽고, 원래 자리(할 일, 문서 검토 앞)로 보낸다.
    expect(read).toHaveBeenCalledTimes(4);
    expect(move).toHaveBeenLastCalledWith(expect.any(String), 'a', '할 일', 'b', 2);
    expect(laneOf('a')).toBe('할 일');
    // 되돌린 완료도 같은 key의 토스트이고, 다시 되돌리기는 내놓지 않는다.
    expect(toast?.message).toBe('API 연결: 되돌렸습니다(할 일).');
    expect(undoOffered()).toBe(false);
    expect(liveText()).not.toContain('되돌렸습니다');
    expect(screen.queryByRole('alert')).toBeNull();
    expect(document.activeElement).toBe(card('a'));
  });

  it('#50 uses the successful neighbor move version before its reload finishes', async () => {
    const a = task('a', 'API 연결');
    const b = task('b', '문서 검토');
    const c = task('c', '회의 준비');
    const savedB = { ...b, move_version: 2 };
    const savedC = { ...c, status: '진행 중', move_version: 2 };
    let finishOld: (tasks: TaskView[]) => void = () => undefined;
    const oldRead = new Promise<TaskView[]>((resolve) => {
      finishOld = resolve;
    });
    lists([a, b, c], oldRead, [savedB, savedC], [savedB, savedC], [c, savedB]);
    const move = vi
      .spyOn(work, 'moveTask')
      .mockImplementation((_expectedUser, id, status) =>
        Promise.resolve({ ...(id === 'b' ? savedB : savedC), status, checklist: [] }),
      );
    await mount();
    await fireEvent.keyDown(card('b'), { key: 'ArrowDown', altKey: true });
    await flush();
    await moveRight('c');
    await flush();
    finishOld([a, b, c]);
    await flush();
    expect(toast?.message).toContain('회의 준비');
    await clickUndo();
    await flush();
    expect(move).toHaveBeenLastCalledWith(expect.any(String), 'c', '할 일', 'b', 2);
  });

  it('undo goes back before the card that was next on the server, not on the optimistic screen', async () => {
    // 서버 [A,B,C]. B를 열 끝으로 옮기고 응답을 기다리는 동안(화면 [A,C,B]) A를 오른쪽으로 옮겨 성공, 그 뒤 B의 이동이
    // 실패한다. A의 원점은 서버 기준 "B 앞"이어야 한다. 화면 배열에서 원점을 잡으면 "C 앞"으로 되돌려 [B,A,C]가 된다.
    const three = [task('a', 'API 연결'), task('b', '문서 검토'), task('c', '회의 준비')];
    const afterA = [task('b', '문서 검토'), task('c', '회의 준비'), movedA];
    lists(three, afterA, afterA, afterA, afterA);
    let failB: (e: Error) => void = () => undefined;
    const bMove = new Promise<Awaited<ReturnType<typeof work.moveTask>>>((_, reject) => {
      failB = reject;
    });
    const move = vi
      .spyOn(work, 'moveTask')
      .mockImplementation((_expectedUser, id) =>
        id === 'b' ? bMove : Promise.resolve({ ...movedA, checklist: [] }),
      );
    await mount();
    await fireEvent.keyDown(card('b'), { key: 'ArrowDown', altKey: true });
    await tick();
    expect(move).toHaveBeenLastCalledWith(expect.any(String), 'b', '할 일', null, undefined);
    await moveRight('a');
    await flush();
    expect(undoOffered()).toBe(true);
    failB(new ApiError(400, 'stale', 'moved meanwhile'));
    await flush();
    await clickUndo();
    await flush();
    expect(move).toHaveBeenLastCalledWith(expect.any(String), 'a', '할 일', 'b', 2);
  });

  it('does not offer undo while the result is unknown', async () => {
    let call = 0;
    vi.spyOn(work, 'tasks').mockImplementation(() => {
      call += 1;
      return call === 1 ? Promise.resolve(start) : Promise.reject(new NetworkError('offline'));
    });
    vi.spyOn(work, 'moveTask').mockRejectedValue(new NetworkError('reset'));
    await mount();
    await moveRight('a');
    await flush();
    expect(undoOffered()).toBe(false);
    await flush(20000);
    expect(undoOffered()).toBe(false);
  });

  it('offers undo once a lost response is confirmed from the list', async () => {
    lists(start, moved);
    vi.spyOn(work, 'moveTask').mockRejectedValue(new NetworkError('reset'));
    await mount();
    await moveRight('a');
    await flush();
    expect(toast?.message).toContain('저장을 확인했습니다');
    expect(liveText()).not.toContain('저장을 확인했습니다');
    expect(undoOffered()).toBe(true);
  });

  it('does not move back when the card changed elsewhere after the move', async () => {
    const elsewhere = [task('b', '문서 검토'), task('a', 'API 연결', '완료', 3)];
    lists(start, moved, elsewhere);
    const move = vi.spyOn(work, 'moveTask').mockResolvedValue({ ...movedA, checklist: [] });
    await mount();
    await moveRight('a');
    await flush();
    await clickUndo();
    await flush();
    expect(move).toHaveBeenCalledTimes(1);
    expect(laneOf('a')).toBe('완료');
    expect(screen.getByRole('alert').textContent).toContain('되돌리지 않았습니다');
  });

  it('sends the move version it read, and a refusal changes nothing and is not sent again', async () => {
    // 되돌리기 직전 목록은 이 이동의 결과(이동 판 2)였다. 보내기 전에 다른 사람이 같은 열 안에서 순서를 바꿨으면 서버가
    // 조건을 보고 409 move_conflict로 거절한다(D-89). 아무것도 바뀌지 않았으니 최신 상태를 보이고 다시 보내지 않는다.
    const reordered = [movedA, task('b', '문서 검토')];
    lists(start, moved, moved, reordered);
    const move = vi
      .spyOn(work, 'moveTask')
      .mockResolvedValueOnce({ ...movedA, checklist: [] })
      .mockRejectedValueOnce(
        new ApiError(409, 'move_conflict', 'the task moved since it was read'),
      );
    await mount();
    await moveRight('a');
    await flush();
    await clickUndo();
    await flush();
    expect(move.mock.calls[1]).toEqual([expect.any(String), 'a', '할 일', 'b', 2]);
    expect(screen.getByRole('alert').textContent).toContain('되돌리지 않았습니다');
    expect(toasted('되돌렸습니다')).toBe(false);
    await flush(20000);
    expect(move).toHaveBeenCalledTimes(2);
  });

  it('withdraws the offer when the same card is moved again', async () => {
    lists(start, moved);
    const later = new Promise<Awaited<ReturnType<typeof work.moveTask>>>(() => undefined);
    vi.spyOn(work, 'moveTask')
      .mockResolvedValueOnce({ ...movedA, checklist: [] })
      .mockReturnValueOnce(later);
    await mount();
    await moveRight('a');
    await flush();
    expect(undoOffered()).toBe(true);
    await moveRight('a');
    await flush();
    expect(undoOffered()).toBe(false);
  });

  it('keeps the offer and sends nothing when the current state cannot be read', async () => {
    let call = 0;
    vi.spyOn(work, 'tasks').mockImplementation(() => {
      call += 1;
      if (call === 1) return Promise.resolve(start);
      if (call === 2) return Promise.resolve(moved);
      return Promise.reject(new NetworkError('offline'));
    });
    const move = vi.spyOn(work, 'moveTask').mockResolvedValue({ ...movedA, checklist: [] });
    await mount();
    await moveRight('a');
    await flush();
    await clickUndo();
    await flush();
    expect(move).toHaveBeenCalledTimes(1);
    expect(liveText()).toContain('지금 상태를 확인하지 못해 되돌리지 않았습니다');
    // 토스트 단추는 누르면 닫히므로 되돌리기를 다시 내놓는다.
    expect(undoOffered()).toBe(true);
    expect(toast?.message).toBe('API 연결: 방금 옮긴 것을 되돌릴 수 있습니다.');
  });

  it('takes the undo toast down when the board is left', async () => {
    lists(start, moved);
    vi.spyOn(work, 'moveTask').mockResolvedValue({ ...movedA, checklist: [] });
    const view = render(TaskBoard, { props: { item, workspaces: [], openTaskId: undefined } });
    await flush();
    await moveRight('a');
    await flush();
    expect(undoOffered()).toBe(true);
    // 떠난 보드의 되돌리기 단추를 토스트에 남기지 않는다.
    view.unmount();
    expect(undoOffered()).toBe(false);
  });
});
