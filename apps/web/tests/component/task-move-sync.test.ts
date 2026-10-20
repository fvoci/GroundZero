import { fireEvent, render, screen, within } from '@testing-library/svelte';
import { tick } from 'svelte';
import type { MockInstance } from 'vitest';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { ItemView, TaskStatus, TaskView } from '$lib/api/client';
import { api, ApiError, NetworkError } from '$lib/api/client';
import {
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

  it('retry does not overwrite a change made elsewhere in the meantime', async () => {
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
  });
});
