import { taskData as work } from '$lib/features/tasks/detail-data';
import './fixtures/task-statuses';
import { fireEvent, render, screen, within } from '@testing-library/svelte';
import { tick } from 'svelte';
import type { MockInstance } from 'vitest';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { ItemView, TaskStatus, TaskView } from '$lib/api/client';
import { api, ApiError, NetworkError } from '$lib/api/client';
import * as feedback from '$lib/features/feedback/feedback.svelte';
import {
  backTo,
  classifyMoveError,
  originOf,
  overlayPending,
  reconcileMove,
  replan,
} from '$lib/features/tasks/move-sync';
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

describe('move result helpers', () => {
  it('returns exactly between adjacent original neighbours after lane renumbering (#50)', () => {
    const original = [task('a', 'A'), task('b', 'B'), task('c', 'C')];
    const from = originOf(original, 'b');
    if (from === undefined) throw new Error('origin');
    const renumbered = [
      task('a', 'A', '할 일', 2),
      task('c', 'C', '할 일', 2),
      task('b', 'B', '진행 중', 2),
    ];
    expect(backTo(renumbered, from)).toEqual({
      plan: { status: '할 일', beforeId: 'c' },
      exact: true,
    });
  });
  const before = [task('a', 'A'), task('b', 'B'), task('c', 'C', '진행 중')];
  const origin = {
    status: '할 일' as const,
    nextId: 'b',
    nextVersion: 1,
    prevId: null,
    prevVersion: null,
    moveVersion: 1,
  };
  const plan = { status: '진행 중' as const, beforeId: null };

  it('tells applied, not applied, changed and gone apart from a fresh list', () => {
    expect(originOf(before, 'a')).toEqual(origin);
    const applied = [task('b', 'B'), task('c', 'C', '진행 중'), task('a', 'A', '진행 중', 2)];
    expect(reconcileMove(applied, 'a', origin, plan)).toBe('applied');
    expect(reconcileMove(before, 'a', origin, plan)).toBe('not_applied');
    // 다른 사람이 완료로 옮겼다(판이 오름): 덮지 않고 서버를 따른다.
    const elsewhere = [task('b', 'B'), task('c', 'C', '진행 중'), task('a', 'A', '완료', 2)];
    expect(reconcileMove(elsewhere, 'a', origin, plan)).toBe('changed');
    // 같은 자리여도 판이 다르면(그사이 고쳐짐) 반영 안 됨으로 단정하지 않는다.
    const edited = [task('a', 'A', '할 일', 2), task('b', 'B'), task('c', 'C', '진행 중')];
    expect(reconcileMove(edited, 'a', origin, plan)).toBe('changed');
    expect(reconcileMove([task('b', 'B')], 'a', origin, plan)).toBe('gone');
  });

  it('a neighbour moved elsewhere does not make a lost move look changed', () => {
    // A의 요청은 반영되지 않았고(열·이동 판 그대로) 그사이 B가 열 끝으로 옮겨졌다: 여전히 "반영 안 됨"이다.
    const neighbour = [task('a', 'A'), task('c', 'C'), task('b', 'B', '할 일', 2)];
    expect(reconcileMove(neighbour, 'a', origin, plan)).toBe('not_applied');
  });

  it('goes back before the next card, else after the previous one, else to the end', () => {
    const three = [task('a', 'A'), task('b', 'B'), task('c', 'C')];
    const from = originOf(three, 'b');
    if (from === undefined) throw new Error('origin');
    expect(from).toEqual({
      status: '할 일',
      nextId: 'c',
      nextVersion: 1,
      prevId: 'a',
      prevVersion: 1,
      moveVersion: 1,
    });
    // 뒤 업무가 그대로면 그 앞.
    expect(backTo([task('a', 'A'), task('c', 'C')], from)).toEqual({
      plan: { status: '할 일', beforeId: 'c' },
      exact: true,
    });
    // 뒤 업무가 옮겨졌으면(판이 오름) 앞 업무 뒤, 곧 그다음 업무 앞.
    const cMoved = [task('a', 'A'), task('d', 'D'), task('c', 'C', '할 일', 2)];
    expect(backTo(cMoved, from)).toEqual({ plan: { status: '할 일', beforeId: 'd' }, exact: true });
    // 앞 업무가 열 끝이면 열 끝(정확한 자리).
    expect(backTo([task('a', 'A')], from)).toEqual({
      plan: { status: '할 일', beforeId: null },
      exact: true,
    });
    // 둘 다 없으면 열 끝이되 정확하지 않다고 알린다.
    expect(backTo([task('d', 'D')], from)).toEqual({
      plan: { status: '할 일', beforeId: null },
      exact: false,
    });
  });

  it('counts a move whose target is the starting place as applied (the state already matches)', () => {
    const atEnd = {
      status: '할 일' as const,
      nextId: null,
      nextVersion: null,
      prevId: null,
      prevVersion: null,
      moveVersion: 1,
    };
    expect(reconcileMove([task('a', 'A')], 'a', atEnd, { status: '할 일', beforeId: null })).toBe(
      'applied',
    );
  });

  it('treats only answers from the app as failures; everything else is uncertain', () => {
    expect(classifyMoveError(new ApiError(400, 'stale', 'x'))).toBe('failed');
    expect(classifyMoveError(new ApiError(500, 'internal', 'x'))).toBe('failed');
    expect(classifyMoveError(new ApiError(504, 'unreachable', 'x'))).toBe('uncertain');
    expect(classifyMoveError(new ApiError(418, 'unexpected', 'x'))).toBe('uncertain');
    expect(classifyMoveError(new NetworkError('offline'))).toBe('uncertain');
    expect(classifyMoveError(new TypeError('bad body'))).toBe('uncertain');
  });

  it('tells a same-column reorder elsewhere apart from not applied, by the move version', () => {
    // 같은 자리·같은 열이어도 그사이 이 업무가 열 안에서 옮겨졌다 돌아왔으면 이동 판이 올라 '반영 안 됨'으로 보지 않는다.
    const reordered = [task('a', 'A', '할 일', 2), task('b', 'B'), task('c', 'C', '진행 중')];
    expect(reconcileMove(reordered, 'a', origin, plan)).toBe('changed');
  });

  it('keeps other pending moves on top of a fresh list and replans a vanished target', () => {
    const out = overlayPending(before, [{ id: 'b', plan: { status: '진행 중', beforeId: 'c' } }]);
    expect(out.map((t) => `${t.id}:${t.status}`)).toEqual(['a:할 일', 'b:진행 중', 'c:진행 중']);
    expect(replan(before, { status: '진행 중', beforeId: 'c' })).toEqual({
      status: '진행 중',
      beforeId: 'c',
    });
    expect(replan(before, { status: '완료', beforeId: 'c' })).toEqual({
      status: '완료',
      beforeId: null,
    });
  });
});

// 칸반 저장 상태: 화면은 먼저 옮기고, 응답이 늦으면 지연을, 결과를 모르면 서버 목록을 대조해 확정한다.
describe('kanban save states', () => {
  let done: MockInstance<typeof feedback.notifyDone>;
  beforeEach(async () => {
    vi.useFakeTimers();
    await setLocale('ko', { reload: false });
    window.sessionStorage.clear();
    vi.spyOn(api, 'savedViews').mockResolvedValue([]);
    vi.spyOn(work, 'syncRecurrences').mockResolvedValue(0);
    done = vi.spyOn(feedback, 'notifyDone');
  });
  /** 이동 완료는 토스트(key `board-move`)로 알린다(D-82). 지금까지 띄운 이동 완료 글. */
  const moveToast = (): string =>
    done.mock.calls
      .filter(([, key]) => key === 'board-move')
      .map(([text]) => text)
      .join(' ');
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

  /** 목록 읽기를 차례로 돌려준다(마지막 것을 계속). */
  function lists(
    ...answers: (TaskView[] | Promise<TaskView[]>)[]
  ): MockInstance<typeof work.tasks> {
    let i = 0;
    return vi.spyOn(work, 'tasks').mockImplementation(() => {
      const answer = answers[Math.min(i, answers.length - 1)] ?? [];
      i += 1;
      return Promise.resolve(answer);
    });
  }
  function deferred<T>(): {
    promise: Promise<T>;
    resolve: (v: T) => void;
    reject: (e: unknown) => void;
  } {
    let resolve: (v: T) => void = () => undefined;
    let reject: (e: unknown) => void = () => undefined;
    const promise = new Promise<T>((res, rej) => {
      resolve = res;
      reject = rej;
    });
    return { promise, resolve, reject };
  }
  const card = (id: string): HTMLElement => {
    const el = document.querySelector<HTMLElement>(`[data-task-card="${id}"]`);
    if (el === null) throw new Error(`card ${id}`);
    return el;
  };
  const laneOf = (id: string): string | undefined =>
    card(id).closest<HTMLElement>('[data-lane]')?.dataset.lane;
  const saveState = (id: string): string | null =>
    card(id)
      .closest('[data-board-card]')
      ?.querySelector('[data-save-state]')
      ?.getAttribute('data-save-state') ?? null;
  const moveRight = async (id: string): Promise<void> => {
    await fireEvent.keyDown(card(id), { key: 'ArrowRight', altKey: true });
    await tick();
  };
  const liveText = (): string =>
    [...document.querySelectorAll('[aria-live="polite"]')].map((e) => e.textContent).join(' ');

  async function mount(): Promise<{ unmount: () => void }> {
    const view = render(TaskBoard, { props: { item, workspaces: [], openTaskId: undefined } });
    await flush();
    return view;
  }

  it('normal save: saving right away, then confirmed', async () => {
    lists(start, moved);
    const save = deferred<Awaited<ReturnType<typeof work.moveTask>>>();
    vi.spyOn(work, 'moveTask').mockReturnValue(save.promise);
    await mount();
    await moveRight('a');
    expect(laneOf('a')).toBe('진행 중');
    expect(saveState('a')).toBe('saving');
    expect(liveText()).toContain('API 연결: 저장 중입니다.');
    save.resolve({ ...movedA, checklist: [] });
    await flush();
    expect(saveState('a')).toBeNull();
    expect(moveToast()).toContain('API 연결: 오른쪽 열로 옮겼습니다(진행 중).');
  });

  it('slow success: says the response is delayed, then confirms when it arrives', async () => {
    lists(start, moved);
    const save = deferred<Awaited<ReturnType<typeof work.moveTask>>>();
    vi.spyOn(work, 'moveTask').mockReturnValue(save.promise);
    await mount();
    await moveRight('a');
    await flush(3000);
    expect(saveState('a')).toBe('slow');
    expect(liveText()).toContain('응답이 지연되고 있습니다');
    save.resolve({ ...movedA, checklist: [] });
    await flush();
    expect(saveState('a')).toBeNull();
    expect(laneOf('a')).toBe('진행 중');
  });

  it('explicit failure: puts the card back and explains, with nothing left pending', async () => {
    lists(start);
    vi.spyOn(work, 'moveTask').mockRejectedValue(new ApiError(400, 'stale', 'x'));
    await mount();
    await moveRight('a');
    await flush();
    expect(laneOf('a')).toBe('할 일');
    expect(saveState('a')).toBeNull();
    expect(screen.getByRole('alert').textContent).toContain('그사이 다른 곳에서 업무가 바뀌어');
  });

  // 카드는 저장 전에 새 열에 다시 그려진다(앞 요소가 사라짐). 저장을 기다리는 동안에도 초점이 문서 처음으로 빠지지 않는다.
  it('keeps focus on the moved card while saving and after it is put back', async () => {
    lists(start);
    const save = deferred<Awaited<ReturnType<typeof work.moveTask>>>();
    vi.spyOn(work, 'moveTask').mockReturnValue(save.promise);
    await mount();
    card('a').focus();
    await moveRight('a');
    await flush();
    expect(laneOf('a')).toBe('진행 중');
    expect(card('a')).toHaveFocus();
    save.reject(new ApiError(400, 'stale', 'x'));
    await flush();
    expect(laneOf('a')).toBe('할 일');
    expect(card('a')).toHaveFocus();
  });

  it('a save that ends late does not take back focus the user moved elsewhere', async () => {
    lists(start, moved);
    const save = deferred<Awaited<ReturnType<typeof work.moveTask>>>();
    vi.spyOn(work, 'moveTask').mockReturnValue(save.promise);
    await mount();
    card('a').focus();
    await moveRight('a');
    await flush();
    card('b').focus();
    save.resolve({ ...movedA, checklist: [] });
    await flush();
    expect(card('b')).toHaveFocus();
  });

  it('lost response but saved on the server: confirms after reading the list again', async () => {
    const read = lists(start, moved);
    const move = vi.spyOn(work, 'moveTask').mockRejectedValue(new NetworkError('reset'));
    await mount();
    await moveRight('a');
    await flush();
    expect(read).toHaveBeenCalledTimes(2);
    expect(move).toHaveBeenCalledTimes(1);
    expect(saveState('a')).toBeNull();
    expect(laneOf('a')).toBe('진행 중');
    expect(moveToast()).toContain('API 연결: 저장을 확인했습니다(진행 중).');
  });

  it('lost response and not saved: shows the server place, retries once only on request', async () => {
    lists(start, start, start, moved);
    const move = vi
      .spyOn(work, 'moveTask')
      .mockRejectedValueOnce(new NetworkError('reset'))
      .mockResolvedValueOnce({ ...movedA, checklist: [] });
    await mount();
    await moveRight('a');
    await flush();
    expect(laneOf('a')).toBe('할 일');
    expect(saveState('a')).toBe('not_saved');
    // 자동으로 다시 보내지 않는다.
    await flush(20000);
    expect(move).toHaveBeenCalledTimes(1);
    const box = card('a').closest<HTMLElement>('[data-board-card]');
    if (box === null) throw new Error('box');
    await fireEvent.click(within(box).getByRole('button', { name: '다시 시도' }));
    await flush();
    expect(move).toHaveBeenCalledTimes(2);
    expect(move).toHaveBeenLastCalledWith(expect.any(String), 'a', '진행 중', null, 1);
    expect(saveState('a')).toBeNull();
    expect(laneOf('a')).toBe('진행 중');
    // 응답의 판이 이 이동만으로 될 판(1 → 2)이므로 겹친 변경으로 알리지 않는다.
    expect(moveToast()).toContain('API 연결: 오른쪽 열로 옮겼습니다(진행 중).');
    expect(screen.queryByRole('alert')?.textContent ?? '').not.toContain('덮었을 수 있습니다');
  });

  it('lost response and the list cannot be read: stays "not confirmed" until checked again', async () => {
    let call = 0;
    vi.spyOn(work, 'tasks').mockImplementation(() => {
      call += 1;
      if (call === 1) return Promise.resolve(start);
      if (call === 2) return Promise.reject(new NetworkError('offline'));
      return Promise.resolve(moved);
    });
    vi.spyOn(work, 'moveTask').mockRejectedValue(new NetworkError('reset'));
    await mount();
    await moveRight('a');
    await flush();
    expect(saveState('a')).toBe('unknown');
    // 요청은 이미 끝났으므로 3초 뒤에도 "지연"으로 바뀌지 않는다.
    await flush(3000);
    expect(saveState('a')).toBe('unknown');
    const box = card('a').closest<HTMLElement>('[data-board-card]');
    if (box === null) throw new Error('box');
    await fireEvent.click(within(box).getByRole('button', { name: '다시 확인' }));
    await flush();
    expect(saveState('a')).toBeNull();
    expect(laneOf('a')).toBe('진행 중');
    expect(moveToast()).toContain('저장을 확인했습니다');
  });

  it('retry is not sent when the list read just before it shows a change made elsewhere', async () => {
    const elsewhere = [task('b', '문서 검토'), task('a', 'API 연결', '완료', 2)];
    lists(start, start, elsewhere);
    const move = vi.spyOn(work, 'moveTask').mockRejectedValue(new NetworkError('reset'));
    await mount();
    await moveRight('a');
    await flush();
    const box = card('a').closest<HTMLElement>('[data-board-card]');
    if (box === null) throw new Error('box');
    await fireEvent.click(within(box).getByRole('button', { name: '다시 시도' }));
    await flush();
    expect(move).toHaveBeenCalledTimes(1);
    expect(laneOf('a')).toBe('완료');
    expect(screen.getByRole('alert').textContent).toContain('그사이 다른 곳에서 바뀌어');
  });

  it('a retry carries the move version it read, and a refusal shows the server state without sending again', async () => {
    // 다시 시도 직전 목록은 이동 전 그대로(이동 판 1)였다. 보내기 전에 다른 사람이 완료로 옮겼으면 서버가 조건을 보고
    // 아무것도 바꾸지 않고 409 move_conflict를 돌려준다(D-89). 화면은 최신 상태를 다시 읽어 보이고 다시 보내지 않는다.
    const elsewhere = [task('b', '문서 검토'), task('a', 'API 연결', '완료', 2)];
    const read = lists(start, start, start, elsewhere);
    const move = vi
      .spyOn(work, 'moveTask')
      .mockRejectedValueOnce(new NetworkError('reset'))
      .mockRejectedValueOnce(
        new ApiError(409, 'move_conflict', 'the task moved since it was read'),
      );
    await mount();
    await moveRight('a');
    await flush();
    expect(saveState('a')).toBe('not_saved');
    // 처음 옮기기는 조건 없이 보낸다(D-79).
    expect(move.mock.calls[0]?.[4]).toBeUndefined();
    const box = card('a').closest<HTMLElement>('[data-board-card]');
    if (box === null) throw new Error('box');
    await fireEvent.click(within(box).getByRole('button', { name: '다시 시도' }));
    await flush();
    expect(move).toHaveBeenCalledTimes(2);
    expect(move.mock.calls[1]).toEqual([expect.any(String), 'a', '진행 중', null, 1]);
    expect(read).toHaveBeenCalledTimes(4);
    expect(saveState('a')).toBeNull();
    expect(laneOf('a')).toBe('완료');
    expect(screen.getByRole('alert').textContent).toContain(
      'API 연결: 그사이 다른 곳에서 이 업무가 옮겨져, 덮어쓰지 않도록 다시 시도를 저장하지 않았습니다.',
    );
    expect(moveToast()).not.toContain('저장을 확인했습니다');
    await flush(20000);
    expect(move).toHaveBeenCalledTimes(2);
  });

  it('a request still running is confirmed early when the list already shows it saved', async () => {
    lists(start, moved);
    const save = deferred<Awaited<ReturnType<typeof work.moveTask>>>();
    vi.spyOn(work, 'moveTask').mockReturnValue(save.promise);
    await mount();
    await moveRight('a');
    await flush(10000);
    expect(saveState('a')).toBeNull();
    expect(moveToast()).toContain('저장을 확인했습니다');
    // 늦게 온 성공 응답은 아무것도 되돌리지 않는다.
    save.resolve({ ...movedA, checklist: [] });
    await flush();
    expect(laneOf('a')).toBe('진행 중');
  });

  it('a running request is not declared unsaved even if the list still shows the old place', async () => {
    lists(start);
    const save = deferred<Awaited<ReturnType<typeof work.moveTask>>>();
    vi.spyOn(work, 'moveTask').mockReturnValue(save.promise);
    await mount();
    await moveRight('a');
    await flush(10000);
    expect(saveState('a')).toBe('slow');
    expect(laneOf('a')).toBe('진행 중');
  });

  it('a list read started while the request was running cannot declare it unsaved after the request ends', async () => {
    // 10초 대조가 목록을 읽는 동안 요청이 커밋된 뒤 응답 없이 끝나고, 다시 대조가 시작된다. 먼저 시작한 읽기는 커밋 전
    // 목록을 늦게 돌려준다. 그 목록으로 "저장되지 않음"을 정하면 안 된다(나중 대조만 따른다).
    const early = deferred<TaskView[]>();
    const later = deferred<TaskView[]>();
    let call = 0;
    vi.spyOn(work, 'tasks').mockImplementation(() => {
      call += 1;
      if (call === 1) return Promise.resolve(start);
      if (call === 2) return early.promise;
      if (call === 3) return later.promise;
      return Promise.resolve(moved);
    });
    const save = deferred<Awaited<ReturnType<typeof work.moveTask>>>();
    vi.spyOn(work, 'moveTask').mockReturnValue(save.promise);
    await mount();
    await moveRight('a');
    await flush(10000);
    expect(saveState('a')).toBe('checking');
    save.reject(new NetworkError('reset'));
    await flush();
    early.resolve(start);
    await flush();
    expect(saveState('a')).not.toBe('not_saved');
    expect(screen.queryByRole('alert')).toBeNull();
    later.resolve(moved);
    await flush();
    expect(saveState('a')).toBeNull();
    expect(laneOf('a')).toBe('진행 중');
    expect(moveToast()).toContain('API 연결: 저장을 확인했습니다(진행 중).');
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('quick add includes the status in one request and keeps the response when reloading fails (#104)', async () => {
    vi.spyOn(work, 'tasks')
      .mockResolvedValueOnce(start)
      .mockRejectedValue(new NetworkError('offline'));
    const create = vi
      .spyOn(work, 'createTask')
      .mockResolvedValue({ ...task('n', '새 업무', '진행 중'), checklist: [] });
    const move = vi.spyOn(work, 'moveTask');
    await mount();
    const lane = screen.getByRole('region', { name: /진행 중/ });
    await fireEvent.click(within(lane).getByRole('button', { name: '업무 추가' }));
    const input = within(lane).getByRole('textbox');
    await fireEvent.input(input, { target: { value: '새 업무' } });
    await fireEvent.submit(input.closest('form') ?? input);
    await flush();
    expect(create).toHaveBeenCalledWith(
      expect.any(String),
      'i1',
      expect.any(String),
      '새 업무',
      null,
      {
        status: '진행 중',
      },
    );
    expect(move).not.toHaveBeenCalled();
    expect(laneOf('n')).toBe('진행 중');
    expect(saveState('n')).toBeNull();
  });

  it('a quick add that finishes after leaving the board does not go on to move it', async () => {
    lists(start);
    const made = deferred<Awaited<ReturnType<typeof work.createTask>>>();
    vi.spyOn(work, 'createTask').mockReturnValue(made.promise);
    const move = vi.spyOn(work, 'moveTask');
    const view = await mount();
    const lane = screen.getByRole('region', { name: /진행 중/ });
    await fireEvent.click(within(lane).getByRole('button', { name: '업무 추가' }));
    const input = within(lane).getByRole('textbox');
    await fireEvent.input(input, { target: { value: '새 업무' } });
    await fireEvent.submit(input.closest('form') ?? input);
    view.unmount();
    made.resolve({ ...task('n', '새 업무'), checklist: [] });
    await flush();
    expect(move).not.toHaveBeenCalled();
    expect(vi.getTimerCount()).toBe(0);
  });

  it('a list read started before a quick add cannot remove the new card while it is saving', async () => {
    // 다른 카드를 옮긴 뒤의 목록 읽기가 늦어지는 사이 새 카드를 다른 열에 만든다. 그 늦은 목록에는 새 카드가 없다.
    const slow = deferred<TaskView[]>();
    let call = 0;
    vi.spyOn(work, 'tasks').mockImplementation(() => {
      call += 1;
      if (call === 1) return Promise.resolve(start);
      if (call === 2) return slow.promise;
      if (call === 3) return Promise.reject(new NetworkError('offline'));
      return Promise.resolve([...start, task('n', '새 업무', '진행 중', 2)]);
    });
    const save = deferred<Awaited<ReturnType<typeof work.moveTask>>>();
    vi.spyOn(work, 'moveTask')
      .mockResolvedValueOnce({ ...task('b', '문서 검토', '진행 중', 2), checklist: [] })
      .mockReturnValueOnce(save.promise);
    vi.spyOn(work, 'createTask').mockResolvedValue({
      ...task('n', '새 업무', '진행 중'),
      checklist: [],
    });
    await mount();
    await moveRight('b');
    await flush();
    const lane = screen.getByRole('region', { name: /진행 중/ });
    await fireEvent.click(within(lane).getByRole('button', { name: '업무 추가' }));
    const input = within(lane).getByRole('textbox');
    await fireEvent.input(input, { target: { value: '새 업무' } });
    await fireEvent.submit(input.closest('form') ?? input);
    await flush();
    expect(saveState('n')).toBeNull();
    slow.resolve([task('a', 'API 연결'), task('b', '문서 검토', '진행 중', 2)]);
    await flush();
    expect(laneOf('n')).toBe('진행 중');
    expect(saveState('n')).toBeNull();
    expect(work.moveTask).toHaveBeenCalledTimes(1);
  });

  it('keeps a 409 move plan and only resaves it explicitly against the latest move version', async () => {
    const newest = task('a', 'API 연결', '완료', 5);
    lists(start, [newest], [newest]);
    const save = deferred<Awaited<ReturnType<typeof work.moveTask>>>();
    const move = vi
      .spyOn(work, 'moveTask')
      .mockRejectedValueOnce(new ApiError(409, 'move_conflict', 'moved elsewhere'))
      .mockReturnValueOnce(save.promise);
    await mount();
    await moveRight('a');
    await flush(20000);
    expect(move).toHaveBeenCalledTimes(1);
    expect(laneOf('a')).toBe('완료');
    expect(screen.getByText('API 연결: 요청한 상태 진행 중')).toBeVisible();
    await fireEvent.click(screen.getByRole('button', { name: '최신본 확인' }));
    await flush();
    await fireEvent.click(screen.getByRole('button', { name: '내 입력으로 다시 저장' }));
    await fireEvent.click(screen.getByRole('button', { name: '내 입력으로 다시 저장' }));
    expect(move).toHaveBeenCalledTimes(2);
    expect(move).toHaveBeenLastCalledWith(expect.any(String), 'a', '진행 중', null, 5);
    save.resolve({ ...newest, status: '진행 중', move_version: 6, checklist: [] });
    await flush();
  });

  it('blocks a second move of the same card but not of another card', async () => {
    lists(start);
    const save = deferred<Awaited<ReturnType<typeof work.moveTask>>>();
    const move = vi.spyOn(work, 'moveTask').mockReturnValue(save.promise);
    await mount();
    await moveRight('a');
    await moveRight('a');
    expect(move).toHaveBeenCalledTimes(1);
    expect(liveText()).toContain('앞선 이동을 저장하는 중입니다');
    await moveRight('b');
    expect(move).toHaveBeenCalledTimes(2);
    expect(move).toHaveBeenLastCalledWith(expect.any(String), 'b', '진행 중', null, undefined);
  });

  // 떠난 뒤 남는 가짜 타이머는 이 보드 것이 아니다: Svelte 이벤트 위임이 거는 1회성 0ms 타이머와 연결 확인 주기(전역,
  // feedback.svelte.ts). 그래서 전체 타이머 수 0을 요구하지 않고 보드가 건 타이머(3초 지연 안내·10초 목록 대조)가 떠날 때
  // 모두 지워지는지, 떠난 뒤 새 재조회·재전송이 없는지, 늦게 온 응답이 초점을 바꾸지 않는지 본다. 이미 보낸 저장 요청은
  // 끝나도록 둔다(화면 이탈은 서버 저장 취소가 아니다).
  it('leaving the screen clears the board timers and starts nothing afterwards', async () => {
    const reads = lists(start);
    const save = deferred<Awaited<ReturnType<typeof work.moveTask>>>();
    const move = vi.spyOn(work, 'moveTask').mockReturnValue(save.promise);
    const errors = vi.spyOn(console, 'error');
    const setTimer = vi.spyOn(globalThis, 'setTimeout');
    const clearTimer = vi.spyOn(globalThis, 'clearTimeout');
    const outside = document.createElement('button');
    document.body.append(outside);
    try {
      const view = await mount();
      card('a').focus();
      await moveRight('a');
      const boardTimers = setTimer.mock.results
        .filter((_, i) => {
          const ms = setTimer.mock.calls[i]?.[1];
          return ms === 3000 || ms === 10000;
        })
        .map((r): unknown => r.value);
      expect(boardTimers).toHaveLength(2);
      const readsBefore = reads.mock.calls.length;
      view.unmount();
      for (const t of boardTimers) expect(clearTimer).toHaveBeenCalledWith(t);
      outside.focus();
      await flush(20000);
      expect(reads.mock.calls.length).toBe(readsBefore);
      expect(move).toHaveBeenCalledTimes(1);
      save.reject(new NetworkError('reset'));
      await flush(20000);
      expect(reads.mock.calls.length).toBe(readsBefore);
      expect(move).toHaveBeenCalledTimes(1);
      expect(outside).toHaveFocus();
      expect(errors).not.toHaveBeenCalled();
    } finally {
      // 가짜 타이머를 되돌리기 전에 spy를 푼다(spy가 가짜 setTimeout을 "원본"으로 기억하고 있다).
      setTimer.mockRestore();
      clearTimer.mockRestore();
      outside.remove();
    }
  });

  // 같은 항목을 다시 열면 새 보드가 같은 id의 카드를 그린다. 떠난 보드의 늦은 성공 응답은 새 보드의 카드로 초점을
  // 가져가거나 목록을 다시 읽지 않는다(새 보드는 이미 서버 상태를 읽어 보이고 있다).
  it('a late answer for a board that was left does not touch a board opened afterwards', async () => {
    const reads = lists(start);
    const save = deferred<Awaited<ReturnType<typeof work.moveTask>>>();
    vi.spyOn(work, 'moveTask').mockReturnValue(save.promise);
    const first = await mount();
    card('a').focus();
    await moveRight('a');
    first.unmount();
    reads.mockResolvedValue(moved);
    await mount();
    expect(laneOf('a')).toBe('진행 중');
    expect(document.activeElement).toBe(document.body);
    const readsAfterMount = reads.mock.calls.length;
    save.resolve({ ...movedA, checklist: [] });
    await flush();
    expect(document.activeElement).toBe(document.body);
    expect(reads.mock.calls.length).toBe(readsAfterMount);
    expect(saveState('a')).toBeNull();
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('an older list read that arrives late does not overwrite a newer one', async () => {
    const first = deferred<TaskView[]>();
    const answers: Promise<TaskView[]>[] = [
      Promise.resolve(start),
      first.promise,
      Promise.resolve(moved),
    ];
    let i = 0;
    vi.spyOn(work, 'tasks').mockImplementation(
      () => answers[Math.min(i++, 2)] ?? Promise.resolve([]),
    );
    vi.spyOn(work, 'moveTask')
      .mockRejectedValueOnce(new ApiError(400, 'stale', 'x'))
      .mockResolvedValueOnce({ ...movedA, checklist: [] });
    await mount();
    await moveRight('a'); // 실패 → 다시 읽기(first, 멈춤)
    await flush();
    await moveRight('a'); // 성공 → 다시 읽기(moved)
    await flush();
    expect(laneOf('a')).toBe('진행 중');
    first.resolve(start);
    await flush();
    expect(laneOf('a')).toBe('진행 중');
    // 늦게 끝난 실패 쪽 흐름이 방금 성공한 이동의 안내를 실패 경고로 덮지 않는다.
    expect(screen.queryByRole('alert')).toBeNull();
    expect(moveToast()).toContain('API 연결: 오른쪽 열로 옮겼습니다(진행 중).');
  });
});
