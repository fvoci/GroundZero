import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { parseRoute, router } from '$lib/router.svelte';

// 뒤로·앞으로 가기도 미저장 편집 보호를 거친다(D-121). 브라우저는 주소를 먼저 바꾸고 popstate를 알리므로, 가드가 막으면
// 같은 기록을 다시 쌓아 원래 주소로 되돌리고 화면은 그대로 둔다. 확인 대화상자에서 떠나기로 하면 proceed가 다시 뒤로 간다.
// jsdom의 기록 이동 대신 주소를 바꾼 뒤 popstate를 직접 보낸다.

function popTo(path: string): void {
  window.history.replaceState(null, '', path);
  window.dispatchEvent(new PopStateEvent('popstate'));
}

describe('router popstate guard', () => {
  it('does not let a pending back token skip an unrelated navigation (#159)', () => {
    const back = vi.spyOn(window.history, 'back').mockImplementation(() => undefined);
    const guard = vi.fn(() => true);
    router.guard = guard;
    router.back('/shared');
    router.navigate('/time');
    guard.mockReturnValue(false);
    popTo('/audit');
    expect(router.path).toBe('/time');
    expect(guard).toHaveBeenCalledTimes(3);
    expect(back).toHaveBeenCalledOnce();
  });

  it('consumes a permitted back only once when close is pressed twice (#159)', () => {
    const back = vi.spyOn(window.history, 'back').mockImplementation(() => undefined);
    router.guard = () => true;
    router.back('/shared');
    router.back('/shared');
    expect(back).toHaveBeenCalledOnce();
    popTo('/shared');
  });
  beforeEach(() => {
    router.guard = undefined;
    window.history.replaceState(null, '', '/notes');
    router.path = '/notes';
    router.hash = '';
  });
  afterEach(() => {
    vi.restoreAllMocks();
    router.guard = undefined;
    window.history.replaceState(null, '', '/');
    router.path = '/';
  });

  it('parses the task views and the task detail under the board view', () => {
    const ws = '11111111-1111-1111-1111-111111111111';
    const item = '22222222-2222-2222-2222-222222222222';
    const task = '33333333-3333-3333-3333-333333333333';
    expect(parseRoute(`/w/${ws}/i/${item}/tasks/gantt`)).toEqual({
      name: 'item',
      workspaceId: ws,
      itemId: item,
      group: 'tasks',
      view: 'gantt',
    });
    expect(parseRoute(`/w/${ws}/i/${item}/tasks/board/t/${task}`)).toEqual({
      name: 'item',
      workspaceId: ws,
      itemId: item,
      group: 'tasks',
      view: 'board',
      taskId: task,
    });
    expect(parseRoute('/tasks')).toEqual({ name: 'my-tasks' });
    expect(parseRoute(`/notifications/${task}`)).toEqual({
      name: 'notifications',
      notificationId: task,
    });
  });

  it('follows back navigation when no guard is set', () => {
    popTo('/shared');
    expect(router.path).toBe('/shared');
    expect(router.route).toEqual({ name: 'shared' });
  });

  it('keeps the screen and restores the address when the guard refuses', () => {
    const guard = vi.fn(() => false);
    router.guard = guard;
    popTo('/shared');
    expect(guard).toHaveBeenCalledTimes(1);
    expect(router.path).toBe('/notes');
    // 막은 뒤에는 주소도 원래 화면으로 되돌린다(기록을 하나 더 쌓는다).
    expect(window.location.pathname).toBe('/notes');
  });

  it('leaves once the user confirms through proceed and does not ask again for that move', () => {
    const back = vi.spyOn(window.history, 'back').mockImplementation(() => undefined);
    let proceed: (() => void) | undefined;
    const guard = vi.fn((go: () => void) => {
      proceed = go;
      return false;
    });
    router.guard = guard;
    popTo('/shared');
    expect(router.path).toBe('/notes');
    // 확인 대화상자에서 "떠나기": 같은 뒤로 가기를 다시 한다. 그때는 가드를 다시 묻지 않는다.
    proceed?.();
    expect(back).toHaveBeenCalledTimes(1);
    popTo('/shared');
    expect(guard).toHaveBeenCalledTimes(1);
    expect(router.path).toBe('/shared');
  });

  // RA-1: 가드가 허락한 back()의 popstate는 다시 묻지 않는다. 그다음 뒤로 가기는 다시 묻는다.
  it('does not ask again for the popstate of an allowed back, but asks for the next one', () => {
    const back = vi.spyOn(window.history, 'back').mockImplementation(() => undefined);
    const guard = vi.fn(() => true);
    router.guard = guard;
    expect(router.back()).toBe(true);
    expect(back).toHaveBeenCalledTimes(1);
    expect(guard).toHaveBeenCalledTimes(1);
    guard.mockReturnValue(false);
    popTo('/shared');
    expect(guard).toHaveBeenCalledTimes(1);
    expect(router.path).toBe('/shared');
    popTo('/time');
    expect(guard).toHaveBeenCalledTimes(2);
    expect(router.path).toBe('/shared');
  });

  it('does not skip the next popstate when the guard refuses back', () => {
    const back = vi.spyOn(window.history, 'back').mockImplementation(() => undefined);
    const guard = vi.fn(() => false);
    router.guard = guard;
    expect(router.back()).toBe(false);
    expect(back).not.toHaveBeenCalled();
    popTo('/shared');
    expect(guard).toHaveBeenCalledTimes(2);
    expect(router.path).toBe('/notes');
  });

  // RF-1: back()도 갈 경로를 가드에 넘긴다(확인 뒤 다시 하는 back()도 같다). 그래야 다음 경로에서도 마운트된 채인 폼이
  // 묻지 않고 통과한다.
  it('passes the next path to the guard on back and on its retry after refusal', () => {
    const back = vi.spyOn(window.history, 'back').mockImplementation(() => undefined);
    let proceed: (() => void) | undefined;
    const guard = vi.fn<(go: () => void, next?: string) => boolean>((go) => {
      if (proceed !== undefined) return true;
      proceed = go;
      return false;
    });
    router.guard = guard;
    expect(router.back('/shared')).toBe(false);
    expect(guard).toHaveBeenLastCalledWith(expect.any(Function), '/shared');
    expect(back).not.toHaveBeenCalled();
    proceed?.();
    expect(guard).toHaveBeenCalledTimes(2);
    expect(guard).toHaveBeenLastCalledWith(expect.any(Function), '/shared');
    expect(back).toHaveBeenCalledTimes(1);
    // 막은 history.back 대신 그 popstate를 보낸다(다음 시험에 건너뛰기를 남기지 않게).
    popTo('/shared');
    expect(guard).toHaveBeenCalledTimes(2);
    expect(router.path).toBe('/shared');
  });

  // 가드는 갈 경로를 받는다(다음 경로에서도 마운트된 채인 폼은 묻지 않고 통과한다, D-121).
  it('passes the next path to the guard on popstate and navigate, and none on a context change', () => {
    const guard = vi.fn<(go: () => void, next?: string) => boolean>(() => true);
    router.guard = guard;
    popTo('/shared');
    expect(guard).toHaveBeenLastCalledWith(expect.any(Function), '/shared');
    router.navigate('/time#top');
    expect(guard).toHaveBeenLastCalledWith(expect.any(Function), '/time');
    router.confirmLeave(() => undefined);
    expect(guard).toHaveBeenLastCalledWith(expect.any(Function));
  });

  it('replaces the address without asking the guard or adding history', () => {
    const guard = vi.fn(() => false);
    router.guard = guard;
    const before = window.history.length;
    router.replace('/shared');
    expect(guard).not.toHaveBeenCalled();
    expect(router.path).toBe('/shared');
    expect(window.location.pathname).toBe('/shared');
    expect(window.history.length).toBe(before);
  });

  it('ignores hash-only changes for the guard', () => {
    const guard = vi.fn(() => false);
    router.guard = guard;
    window.history.replaceState(null, '', '/notes#ref-1');
    window.dispatchEvent(new PopStateEvent('popstate'));
    expect(guard).not.toHaveBeenCalled();
    expect(router.hash).toBe('#ref-1');
  });
});

// 이동이 아닌 문맥 전환(조직 바꾸기)도 같은 가드를 거친다: 막히면 실행하지 않고, 떠나기로 하면 가드가 그 동작을 실행한다.
describe('router.confirmLeave', () => {
  afterEach(() => {
    router.guard = undefined;
  });

  it('runs a context change only when the guard allows it, or when the user proceeds', () => {
    const action = vi.fn();
    let proceed: (() => void) | undefined;
    router.guard = (next) => {
      proceed = next;
      return false;
    };
    router.confirmLeave(action);
    expect(action).not.toHaveBeenCalled();
    proceed?.();
    expect(action).toHaveBeenCalledTimes(1);

    router.guard = () => true;
    router.confirmLeave(action);
    expect(action).toHaveBeenCalledTimes(2);

    router.guard = undefined;
    router.confirmLeave(action);
    expect(action).toHaveBeenCalledTimes(3);
  });
});
