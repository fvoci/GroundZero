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
  beforeEach(() => {
    router.guard = undefined;
    window.history.replaceState(null, '', '/notes');
    router.path = '/notes';
    router.hash = '';
  });
  afterEach(() => {
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
      view: 'gantt',
    });
    expect(parseRoute(`/w/${ws}/i/${item}/tasks/board/t/${task}`)).toEqual({
      name: 'item',
      workspaceId: ws,
      itemId: item,
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

  it('ignores hash-only changes for the guard', () => {
    const guard = vi.fn(() => false);
    router.guard = guard;
    window.history.replaceState(null, '', '/notes#ref-1');
    window.dispatchEvent(new PopStateEvent('popstate'));
    expect(guard).not.toHaveBeenCalled();
    expect(router.hash).toBe('#ref-1');
  });
});
