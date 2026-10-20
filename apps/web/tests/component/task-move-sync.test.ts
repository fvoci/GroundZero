import { fireEvent, render, screen, within } from '@testing-library/svelte';
import { tick } from 'svelte';
import type { MockInstance } from 'vitest';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { ItemView, TaskStatus, TaskView } from '$lib/api/client';
import { api, ApiError, NetworkError } from '$lib/api/client';
import {
  classifyMoveError,
  originOf,
  overlapped,
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
  starts_on: null,
  ends_on: null,
};

function task(id: string, title: string, status: TaskStatus = 'todo', version = 1): TaskView {
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
    predecessor_ids: [],
    series: null,
    occurrence_date: null,
    assignees: [],
    checklist_done: 0,
    checklist_total: 0,
    version,
    completed_at: null,
    access: 'write',
    updated_at: '2026-10-21T00:00:00Z',
  };
}

describe('move result helpers', () => {
  const before = [task('a', 'A'), task('b', 'B'), task('c', 'C', 'in_progress')];
  const origin = { status: 'todo' as const, nextId: 'b', version: 1 };
  const plan = { status: 'in_progress' as const, beforeId: null };

  it('tells applied, not applied, changed and gone apart from a fresh list', () => {
    expect(originOf(before, 'a')).toEqual(origin);
    const applied = [
      task('b', 'B'),
      task('c', 'C', 'in_progress'),
      task('a', 'A', 'in_progress', 2),
    ];
    expect(reconcileMove(applied, 'a', origin, plan)).toBe('applied');
    expect(reconcileMove(before, 'a', origin, plan)).toBe('not_applied');
    // 다른 사람이 완료로 옮겼다(판이 오름): 덮지 않고 서버를 따른다.
    const elsewhere = [task('b', 'B'), task('c', 'C', 'in_progress'), task('a', 'A', 'done', 2)];
    expect(reconcileMove(elsewhere, 'a', origin, plan)).toBe('changed');
    // 같은 자리여도 판이 다르면(그사이 고쳐짐) 반영 안 됨으로 단정하지 않는다.
    const edited = [task('a', 'A', 'todo', 2), task('b', 'B'), task('c', 'C', 'in_progress')];
    expect(reconcileMove(edited, 'a', origin, plan)).toBe('changed');
    expect(reconcileMove([task('b', 'B')], 'a', origin, plan)).toBe('gone');
  });

  it('counts a move whose target is the starting place as applied (the state already matches)', () => {
    const atEnd = { status: 'todo' as const, nextId: null, version: 1 };
    expect(reconcileMove([task('a', 'A')], 'a', atEnd, { status: 'todo', beforeId: null })).toBe(
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

  it('notices a change that slipped in before a resend only when it raised the version', () => {
    // 이 이동만이면 열이 바뀌어 판 1 → 2.
    expect(overlapped(origin, plan, 2)).toBe(false);
    // 그사이 다른 사람의 열 이동이나 편집이 판을 올렸다.
    expect(overlapped(origin, plan, 3)).toBe(true);
    // 같은 열 안 순서 바꾸기는 판이 오르지 않는다: 이 이동만이면 그대로, 편집이 끼면 오른다.
    const sameLane = { status: 'todo' as const, beforeId: null };
    expect(overlapped(origin, sameLane, 1)).toBe(false);
    expect(overlapped(origin, sameLane, 2)).toBe(true);
    // 한계: 다른 사람이 같은 열 안에서만 순서를 바꿨다면 판이 같아 알 수 없다(위 첫 경우와 구별되지 않음).
  });

  it('keeps other pending moves on top of a fresh list and replans a vanished target', () => {
    const out = overlayPending(before, [
      { id: 'b', plan: { status: 'in_progress', beforeId: 'c' } },
    ]);
    expect(out.map((t) => `${t.id}:${t.status}`)).toEqual([
      'a:todo',
      'b:in_progress',
      'c:in_progress',
    ]);
    expect(replan(before, { status: 'in_progress', beforeId: 'c' })).toEqual({
      status: 'in_progress',
      beforeId: 'c',
    });
    expect(replan(before, { status: 'done', beforeId: 'c' })).toEqual({
      status: 'done',
      beforeId: null,
    });
  });
});

// 칸반 저장 상태: 화면은 먼저 옮기고, 응답이 늦으면 지연을, 결과를 모르면 서버 목록을 대조해 확정한다.
describe('kanban save states', () => {
  beforeEach(async () => {
    vi.useFakeTimers();
    await setLocale('ko', { reload: false });
    window.sessionStorage.clear();
    vi.spyOn(api, 'savedViews').mockResolvedValue([]);
    vi.spyOn(api, 'syncRecurrences').mockResolvedValue(0);
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
  const movedA = task('a', 'API 연결', 'in_progress', 2);
  const moved = [task('b', '문서 검토'), movedA];

  /** 목록 읽기를 차례로 돌려준다(마지막 것을 계속). */
  function lists(...answers: (TaskView[] | Promise<TaskView[]>)[]): MockInstance<typeof api.tasks> {
    let i = 0;
    return vi.spyOn(api, 'tasks').mockImplementation(() => {
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
    const save = deferred<Awaited<ReturnType<typeof api.moveTask>>>();
    vi.spyOn(api, 'moveTask').mockReturnValue(save.promise);
    await mount();
    await moveRight('a');
    expect(laneOf('a')).toBe('in_progress');
    expect(saveState('a')).toBe('saving');
    expect(liveText()).toContain('API 연결: 저장 중입니다.');
    save.resolve({ ...movedA, checklist: [] });
    await flush();
    expect(saveState('a')).toBeNull();
    expect(liveText()).toContain('API 연결: 오른쪽 열로 옮겼습니다(진행 중).');
  });

  it('slow success: says the response is delayed, then confirms when it arrives', async () => {
    lists(start, moved);
    const save = deferred<Awaited<ReturnType<typeof api.moveTask>>>();
    vi.spyOn(api, 'moveTask').mockReturnValue(save.promise);
    await mount();
    await moveRight('a');
    await flush(3000);
    expect(saveState('a')).toBe('slow');
    expect(liveText()).toContain('응답이 지연되고 있습니다');
    save.resolve({ ...movedA, checklist: [] });
    await flush();
    expect(saveState('a')).toBeNull();
    expect(laneOf('a')).toBe('in_progress');
  });

  it('explicit failure: puts the card back and explains, with nothing left pending', async () => {
    lists(start);
    vi.spyOn(api, 'moveTask').mockRejectedValue(new ApiError(400, 'stale', 'x'));
    await mount();
    await moveRight('a');
    await flush();
    expect(laneOf('a')).toBe('todo');
    expect(saveState('a')).toBeNull();
    expect(screen.getByRole('alert').textContent).toContain('그사이 다른 곳에서 업무가 바뀌어');
  });

  it('lost response but saved on the server: confirms after reading the list again', async () => {
    const read = lists(start, moved);
    const move = vi.spyOn(api, 'moveTask').mockRejectedValue(new NetworkError('reset'));
    await mount();
    await moveRight('a');
    await flush();
    expect(read).toHaveBeenCalledTimes(2);
    expect(move).toHaveBeenCalledTimes(1);
    expect(saveState('a')).toBeNull();
    expect(laneOf('a')).toBe('in_progress');
    expect(liveText()).toContain('API 연결: 저장을 확인했습니다(진행 중).');
  });

  it('lost response and not saved: shows the server place, retries once only on request', async () => {
    lists(start, start, start, moved);
    const move = vi
      .spyOn(api, 'moveTask')
      .mockRejectedValueOnce(new NetworkError('reset'))
      .mockResolvedValueOnce({ ...movedA, checklist: [] });
    await mount();
    await moveRight('a');
    await flush();
    expect(laneOf('a')).toBe('todo');
    expect(saveState('a')).toBe('not_saved');
    // 자동으로 다시 보내지 않는다.
    await flush(20000);
    expect(move).toHaveBeenCalledTimes(1);
    const box = card('a').closest<HTMLElement>('[data-board-card]');
    if (box === null) throw new Error('box');
    await fireEvent.click(within(box).getByRole('button', { name: '다시 시도' }));
    await flush();
    expect(move).toHaveBeenCalledTimes(2);
    expect(move).toHaveBeenLastCalledWith('a', 'in_progress', null);
    expect(saveState('a')).toBeNull();
    expect(laneOf('a')).toBe('in_progress');
    // 응답의 판이 이 이동만으로 될 판(1 → 2)이므로 겹친 변경으로 알리지 않는다.
    expect(liveText()).toContain('API 연결: 오른쪽 열로 옮겼습니다(진행 중).');
    expect(screen.queryByRole('alert')?.textContent ?? '').not.toContain('덮었을 수 있습니다');
  });

  it('lost response and the list cannot be read: stays "not confirmed" until checked again', async () => {
    let call = 0;
    vi.spyOn(api, 'tasks').mockImplementation(() => {
      call += 1;
      if (call === 1) return Promise.resolve(start);
      if (call === 2) return Promise.reject(new NetworkError('offline'));
      return Promise.resolve(moved);
    });
    vi.spyOn(api, 'moveTask').mockRejectedValue(new NetworkError('reset'));
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
    expect(laneOf('a')).toBe('in_progress');
    expect(liveText()).toContain('저장을 확인했습니다');
  });

  it('retry is not sent when the list read just before it shows a change made elsewhere', async () => {
    const elsewhere = [task('b', '문서 검토'), task('a', 'API 연결', 'done', 2)];
    lists(start, start, elsewhere);
    const move = vi.spyOn(api, 'moveTask').mockRejectedValue(new NetworkError('reset'));
    await mount();
    await moveRight('a');
    await flush();
    const box = card('a').closest<HTMLElement>('[data-board-card]');
    if (box === null) throw new Error('box');
    await fireEvent.click(within(box).getByRole('button', { name: '다시 시도' }));
    await flush();
    expect(move).toHaveBeenCalledTimes(1);
    expect(laneOf('a')).toBe('done');
    expect(screen.getByRole('alert').textContent).toContain('그사이 다른 곳에서 바뀌어');
  });

  it('a change made elsewhere right after the reread is reported, not confirmed', async () => {
    // 다시 시도 직전 목록은 이동 전 그대로(판 1)였는데, 보내기 전에 다른 사람이 완료로 옮겼다(판 2). 이동 API에는 판
    // 조건이 없어 이번 이동이 그 위에 반영되고(판 3) 그 변경을 덮는다. 막을 수는 없으므로 확정이라고 하지 않고 알린다.
    const overlappedA = task('a', 'API 연결', 'in_progress', 3);
    const read = lists(start, start, start, [task('b', '문서 검토'), overlappedA]);
    const move = vi
      .spyOn(api, 'moveTask')
      .mockRejectedValueOnce(new NetworkError('reset'))
      .mockResolvedValueOnce({ ...overlappedA, checklist: [] });
    await mount();
    await moveRight('a');
    await flush();
    expect(saveState('a')).toBe('not_saved');
    const box = card('a').closest<HTMLElement>('[data-board-card]');
    if (box === null) throw new Error('box');
    await fireEvent.click(within(box).getByRole('button', { name: '다시 시도' }));
    await flush();
    expect(move).toHaveBeenCalledTimes(2);
    // 보낸 뒤 서버 목록을 다시 읽어 최신 상태를 보인다.
    expect(read).toHaveBeenCalledTimes(4);
    expect(saveState('a')).toBeNull();
    expect(laneOf('a')).toBe('in_progress');
    expect(screen.getByRole('alert').textContent).toContain(
      'API 연결: 다시 보내는 사이 다른 곳에서 이 업무가 바뀌어, 이번 이동이 그 변경을 덮었을 수 있습니다.',
    );
    expect(liveText()).not.toContain('옮겼습니다');
    expect(liveText()).not.toContain('저장을 확인했습니다');
    // 안내는 다음 동작 전까지 남는다(자동으로 지우지 않는다).
    await flush(20000);
    expect(screen.getByRole('alert').textContent).toContain('덮었을 수 있습니다');
    expect(move).toHaveBeenCalledTimes(2);
  });

  it('a request still running is confirmed early when the list already shows it saved', async () => {
    lists(start, moved);
    const save = deferred<Awaited<ReturnType<typeof api.moveTask>>>();
    vi.spyOn(api, 'moveTask').mockReturnValue(save.promise);
    await mount();
    await moveRight('a');
    await flush(10000);
    expect(saveState('a')).toBeNull();
    expect(liveText()).toContain('저장을 확인했습니다');
    // 늦게 온 성공 응답은 아무것도 되돌리지 않는다.
    save.resolve({ ...movedA, checklist: [] });
    await flush();
    expect(laneOf('a')).toBe('in_progress');
  });

  it('a running request is not declared unsaved even if the list still shows the old place', async () => {
    lists(start);
    const save = deferred<Awaited<ReturnType<typeof api.moveTask>>>();
    vi.spyOn(api, 'moveTask').mockReturnValue(save.promise);
    await mount();
    await moveRight('a');
    await flush(10000);
    expect(saveState('a')).toBe('slow');
    expect(laneOf('a')).toBe('in_progress');
  });

  it('a list read started while the request was running cannot declare it unsaved after the request ends', async () => {
    // 10초 대조가 목록을 읽는 동안 요청이 커밋된 뒤 응답 없이 끝나고, 다시 대조가 시작된다. 먼저 시작한 읽기는 커밋 전
    // 목록을 늦게 돌려준다. 그 목록으로 "저장되지 않음"을 정하면 안 된다(나중 대조만 따른다).
    const early = deferred<TaskView[]>();
    const later = deferred<TaskView[]>();
    let call = 0;
    vi.spyOn(api, 'tasks').mockImplementation(() => {
      call += 1;
      if (call === 1) return Promise.resolve(start);
      if (call === 2) return early.promise;
      if (call === 3) return later.promise;
      return Promise.resolve(moved);
    });
    const save = deferred<Awaited<ReturnType<typeof api.moveTask>>>();
    vi.spyOn(api, 'moveTask').mockReturnValue(save.promise);
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
    expect(laneOf('a')).toBe('in_progress');
    expect(liveText()).toContain('API 연결: 저장을 확인했습니다(진행 중).');
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('blocks a second move of the same card but not of another card', async () => {
    lists(start);
    const save = deferred<Awaited<ReturnType<typeof api.moveTask>>>();
    const move = vi.spyOn(api, 'moveTask').mockReturnValue(save.promise);
    await mount();
    await moveRight('a');
    await moveRight('a');
    expect(move).toHaveBeenCalledTimes(1);
    expect(liveText()).toContain('앞선 이동을 저장하는 중입니다');
    await moveRight('b');
    expect(move).toHaveBeenCalledTimes(2);
    expect(move).toHaveBeenLastCalledWith('b', 'in_progress', null);
  });

  it('leaving the screen stops the timers and ignores late answers', async () => {
    lists(start);
    const save = deferred<Awaited<ReturnType<typeof api.moveTask>>>();
    vi.spyOn(api, 'moveTask').mockReturnValue(save.promise);
    const errors = vi.spyOn(console, 'error');
    const view = await mount();
    await moveRight('a');
    expect(vi.getTimerCount()).toBeGreaterThan(0);
    view.unmount();
    expect(vi.getTimerCount()).toBe(0);
    save.reject(new NetworkError('reset'));
    await flush(20000);
    expect(errors).not.toHaveBeenCalled();
  });

  it('an older list read that arrives late does not overwrite a newer one', async () => {
    const first = deferred<TaskView[]>();
    const answers: Promise<TaskView[]>[] = [
      Promise.resolve(start),
      first.promise,
      Promise.resolve(moved),
    ];
    let i = 0;
    vi.spyOn(api, 'tasks').mockImplementation(
      () => answers[Math.min(i++, 2)] ?? Promise.resolve([]),
    );
    vi.spyOn(api, 'moveTask')
      .mockRejectedValueOnce(new ApiError(400, 'stale', 'x'))
      .mockResolvedValueOnce({ ...movedA, checklist: [] });
    await mount();
    await moveRight('a'); // 실패 → 다시 읽기(first, 멈춤)
    await flush();
    await moveRight('a'); // 성공 → 다시 읽기(moved)
    await flush();
    expect(laneOf('a')).toBe('in_progress');
    first.resolve(start);
    await flush();
    expect(laneOf('a')).toBe('in_progress');
    // 늦게 끝난 실패 쪽 흐름이 방금 성공한 이동의 안내를 실패 경고로 덮지 않는다.
    expect(screen.queryByRole('alert')).toBeNull();
    expect(liveText()).toContain('API 연결: 오른쪽 열로 옮겼습니다(진행 중).');
  });
});
