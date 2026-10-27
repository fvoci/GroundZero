import * as feedback from '$lib/features/feedback/feedback.svelte';
import './fixtures/task-statuses';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { ItemView } from '$lib/api/client';
import { api } from '$lib/api/client';
import TaskBoard from '$lib/features/tasks/TaskBoard.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';
import { itemViewPath, router } from '$lib/router.svelte';
import { me } from '$lib/me.svelte';

// D-103(감사 P-26): 이미 있는 보기는 목록에서 읽은 판으로 고친다(`PUT /api/views/{id}`). 그사이 다른 탭이 저장했으면
// 서버는 409와 최신 보기를 돌려주고 아무것도 바꾸지 않는다. 화면은 지금 설정과 이름을 그대로 두고 목록을 다시 읽어,
// 다시 저장하면 최신 판을 기준으로 지금 설정이 저장된다. 새 이름은 만들기(`POST`)이고, 다른 탭이 같은 이름을 먼저
// 만들었으면 409로 알린다.
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
  updated_at: '2026-10-23T00:00:00Z',
  status: 'active',
  is_template: false,
  version: 1,
  starts_on: null,
  ends_on: null,
};

interface Stored {
  id: string;
  name: string;
  config: Record<string, unknown>;
  version: number;
}

function response(status: number, json: unknown): Response {
  return new Response(JSON.stringify(json), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });
}

/** 시험이 바꾸는 서버 상태: 목록 읽기 실패, 쓰기 응답 붙잡기. */
interface Control {
  listFails: boolean;
  held: (() => void)[] | undefined;
}

/** 저장한 보기만 다루는 작은 서버(D-103 계약). 보낸 요청을 모은다. */
function viewServer(
  stored: Stored[],
  control: Control = { listFails: false, held: undefined },
): { method: string; path: string; body: Record<string, unknown> }[] {
  const sent: { method: string; path: string; body: Record<string, unknown> }[] = [];
  vi.stubGlobal('fetch', (input: unknown, init?: RequestInit) => {
    const path = typeof input === 'string' ? input : '';
    const method = init?.method ?? 'GET';
    const raw: unknown = typeof init?.body === 'string' ? JSON.parse(init.body) : {};
    const body: Record<string, unknown> = typeof raw === 'object' && raw !== null ? { ...raw } : {};
    if (path === '/api/items/i1/views' && method === 'GET') {
      return Promise.resolve(
        control.listFails
          ? response(500, { error: 'internal', message: 'x', request_id: 'r1' })
          : response(200, stored),
      );
    }
    if (method !== 'GET') sent.push({ method, path, body });
    const held = control.held;
    if (held !== undefined && method !== 'GET') {
      return new Promise<Response>((resolve) => {
        held.push(() => {
          control.held = undefined;
          resolve(answer(method, path, body));
        });
      });
    }
    return Promise.resolve(answer(method, path, body));
  });
  function answer(method: string, path: string, body: Record<string, unknown>): Response {
    if (path === '/api/items/i1/views' && method === 'POST') {
      const taken = stored.find((v) => v.name === body.name);
      if (taken !== undefined) {
        return response(409, { error: 'exists', current: taken });
      }
      const created = {
        id: `v${String(stored.length + 1)}`,
        name: String(body.name),
        config: {},
        version: 1,
      };
      stored.push(created);
      return response(201, created);
    }
    const target = stored.find((v) => path === `/api/views/${v.id}`);
    if (target !== undefined && method === 'PUT') {
      if (body.base_version !== target.version) {
        return response(409, { error: 'conflict', current: target });
      }
      const config: unknown = body.config;
      if (typeof config === 'object' && config !== null) target.config = { ...config };
      target.version += 1;
      return response(200, target);
    }
    return response(404, { error: 'not_found', message: 'x' });
  }
  return sent;
}

describe('saved view save on a stale version', () => {
  it('reuses the saved view request id after a lost response and explicit draft restore', async () => {
    const saved: Stored[] = [];
    vi.spyOn(api, 'savedViews').mockImplementation(() => Promise.resolve(saved));
    const create = vi.spyOn(api, 'createView').mockImplementation(() => {
      saved.push({ id: 'created', name: '응답 유실', config: {}, version: 1 });
      return Promise.reject(new Error('response lost'));
    });
    const update = vi.spyOn(api, 'updateView');
    const props = { item, workspaces: [], openTaskId: undefined };
    const first = render(TaskBoard, props);
    await fireEvent.click(screen.getByRole('button', { name: /필터·정렬/ }));
    await fireEvent.input(screen.getByRole('textbox', { name: '보기 이름' }), {
      target: { value: '응답 유실' },
    });
    await fireEvent.click(screen.getByRole('button', { name: '지금 보기 저장' }));
    await waitFor(() => {
      expect(create).toHaveBeenCalledOnce();
    });
    const id = create.mock.calls[0]?.[1];
    first.unmount();
    render(TaskBoard, props);
    await fireEvent.click(screen.getByRole('button', { name: /필터·정렬/ }));
    await fireEvent.click(await screen.findByRole('button', { name: '입력 되살리기' }));
    expect(create).toHaveBeenCalledOnce();
    await fireEvent.click(screen.getByRole('button', { name: '지금 보기 저장' }));
    await waitFor(() => {
      expect(create).toHaveBeenCalledTimes(2);
    });
    expect(create.mock.calls[1]?.[1]).toBe(id);
    expect(update).not.toHaveBeenCalled();
    await fireEvent.input(screen.getByRole('textbox', { name: '보기 이름' }), {
      target: { value: '다른 보기' },
    });
    await fireEvent.click(screen.getByRole('button', { name: '지금 보기 저장' }));
    await waitFor(() => {
      expect(create).toHaveBeenCalledTimes(3);
    });
    expect(create.mock.calls[2]?.[1]).not.toBe(id);
  });
  // R5 F-3(⑱·D-97): 응답을 잃은 저장 뒤 이탈 확인에서 버리면 요청까지 버린다. 남으면 버린 보기 초안을 다시 제안했다.
  it('drops the pending view request when leaving with discard after a lost response', async () => {
    const saved: Stored[] = [];
    vi.spyOn(api, 'savedViews').mockImplementation(() => Promise.resolve(saved));
    const create = vi.spyOn(api, 'createView').mockImplementation(() => {
      saved.push({ id: 'created', name: '버릴 보기', config: {}, version: 1 });
      return Promise.reject(new Error('response lost'));
    });
    const props = { item, workspaces: [], openTaskId: undefined };
    const first = render(TaskBoard, props);
    await fireEvent.click(screen.getByRole('button', { name: /필터·정렬/ }));
    await fireEvent.input(screen.getByRole('textbox', { name: '보기 이름' }), {
      target: { value: '버릴 보기' },
    });
    await fireEvent.click(screen.getByRole('button', { name: '지금 보기 저장' }));
    await waitFor(() => {
      expect(create).toHaveBeenCalledOnce();
    });
    router.navigate('/view-discarded');
    await fireEvent.click(await screen.findByRole('button', { name: m.common_discard_confirm() }));
    expect(router.path).toBe('/view-discarded');
    first.unmount();
    render(TaskBoard, props);
    await fireEvent.click(screen.getByRole('button', { name: /필터·정렬/ }));
    expect(screen.queryByTestId('form-draft-offer')).toBeNull();
  });
  // R5 F-4(D-121): 보기 초안을 되살리면 보기 전환처럼 주소도 바꾼다. 주소는 칸반인데 화면만 목록이 되던 것을 막는다.
  it('moves the address to the restored view of a view draft', async () => {
    vi.spyOn(api, 'savedViews').mockResolvedValue([]);
    // 주소의 보기가 저장 문맥보다 앞서도록 경로로 읽히는 id를 쓴다.
    const ws = '00000000-0000-4000-8000-0000000000a1';
    const routed = { ...item, id: '00000000-0000-4000-8000-0000000000b1', workspace_id: ws };
    const props = { item: routed, workspaces: [], openTaskId: undefined };
    router.navigate(itemViewPath(ws, routed.id, 'board'));
    const first = render(TaskBoard, props);
    await fireEvent.click(screen.getByRole('button', { name: '목록' }));
    expect(router.path).toBe(itemViewPath(ws, routed.id, 'list'));
    await fireEvent.click(screen.getByRole('button', { name: /필터·정렬/ }));
    await fireEvent.input(screen.getByRole('textbox', { name: '보기 이름' }), {
      target: { value: '목록 보기' },
    });
    first.unmount();
    router.navigate(itemViewPath(ws, routed.id, 'board'));
    render(TaskBoard, props);
    expect(screen.getByRole('button', { name: '칸반' })).toHaveAttribute('aria-pressed', 'true');
    await fireEvent.click(screen.getByRole('button', { name: /필터·정렬/ }));
    await fireEvent.click(await screen.findByRole('button', { name: '입력 되살리기' }));
    expect(screen.getByRole('textbox', { name: '보기 이름' })).toHaveValue('목록 보기');
    expect(router.path).toBe(itemViewPath(ws, routed.id, 'list'));
    expect(screen.getByRole('button', { name: '목록' })).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByRole('button', { name: '칸반' })).toHaveAttribute('aria-pressed', 'false');
  });
  it('offers an unsaved view name after remount and restores it without sending a save', async () => {
    const sent = viewServer([]);
    const props = { item, workspaces: [], openTaskId: undefined };
    const first = render(TaskBoard, props);
    await fireEvent.click(screen.getByRole('button', { name: /필터·정렬/ }));
    await fireEvent.input(screen.getByRole('textbox', { name: '보기 이름' }), {
      target: { value: '집중할 업무' },
    });
    const unload = new Event('beforeunload', { cancelable: true });
    window.dispatchEvent(unload);
    expect(unload.defaultPrevented).toBe(true);
    feedback.accountSafety.switching = true;
    const switchingUnload = new Event('beforeunload', { cancelable: true });
    window.dispatchEvent(switchingUnload);
    expect(switchingUnload.defaultPrevented).toBe(false);
    feedback.accountSafety.switching = false;
    first.unmount();
    render(TaskBoard, props);
    await fireEvent.click(screen.getByRole('button', { name: /필터·정렬/ }));
    await screen.findByTestId('form-draft-offer');
    expect(screen.getByRole('textbox', { name: '보기 이름' })).toHaveValue('');
    await fireEvent.click(screen.getByRole('button', { name: '입력 되살리기' }));
    expect(screen.getByRole('textbox', { name: '보기 이름' })).toHaveValue('집중할 업무');
    expect(sent).toEqual([]);
  });
  beforeEach(async () => {
    me.userId = 'user-a';
    localStorage.clear();
    await setLocale('ko', { reload: false });
    window.sessionStorage.clear();
    vi.spyOn(api, 'tasks').mockResolvedValue([]);
    vi.spyOn(api, 'syncRecurrences').mockResolvedValue(0);
  });
  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
    window.sessionStorage.clear();
  });

  it('keeps the current settings on 409 and saves them on the latest version next time', async () => {
    const stored: Stored[] = [
      {
        id: 'v1',
        name: '내 보기',
        config: { view: 'list', sort: 'position', filter: {} },
        version: 1,
      },
    ];
    const sent = viewServer(stored);
    render(TaskBoard, { props: { item, workspaces: [], openTaskId: undefined } });
    // 저장한 보기는 접힌 "필터·정렬" 영역 안에 있다.
    await fireEvent.click(screen.getByRole('button', { name: /필터·정렬/ }));
    const select = await screen.findByRole('combobox', { name: '저장한 보기' });
    await screen.findByRole('option', { name: '내 보기' });
    await fireEvent.change(select, { target: { value: 'v1' } });
    // 이 탭이 판 1을 읽은 뒤 다른 탭이 같은 보기를 저장했다.
    const first = stored[0];
    if (first === undefined) throw new Error('stored');
    first.config = { view: 'calendar', sort: 'due', filter: {} };
    first.version = 2;

    await fireEvent.click(screen.getByRole('button', { name: '칸반' }));
    await fireEvent.click(screen.getByRole('button', { name: '지금 보기 저장' }));

    expect(
      await screen.findByText(/다른 탭에서 보기 "내 보기"을\(를\) 먼저 저장해/),
    ).toBeInTheDocument();
    expect(sent[0]).toMatchObject({ method: 'PUT', path: '/api/views/v1' });
    expect(sent[0]?.body).toMatchObject({ base_version: 1, config: { view: 'board' } });
    expect(first.config).toEqual({ view: 'calendar', sort: 'due', filter: {} });
    // 지금 설정과 이름은 그대로다.
    expect(screen.getByRole('button', { name: '칸반' })).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByRole('textbox', { name: '보기 이름' })).toHaveValue('내 보기');

    // 다시 저장하면 다시 읽은 판(2)을 기준으로 지금 설정을 저장한다.
    await fireEvent.click(screen.getByRole('button', { name: '지금 보기 저장' }));
    await waitFor(() => {
      expect(sent).toHaveLength(2);
    });
    expect(sent[1]).toMatchObject({ method: 'PUT', path: '/api/views/v1' });
    expect(sent[1]?.body).toMatchObject({ base_version: 2, config: { view: 'board' } });
    expect(sent[1]?.body.request_id).not.toBe(sent[0]?.body.request_id);
    expect(first.config).toMatchObject({ view: 'board' });
  });

  it('reports a name another tab created first and keeps the settings', async () => {
    const stored: Stored[] = [];
    const sent = viewServer(stored);
    render(TaskBoard, { props: { item, workspaces: [], openTaskId: undefined } });
    // 저장한 보기는 접힌 "필터·정렬" 영역 안에 있다.
    await fireEvent.click(screen.getByRole('button', { name: /필터·정렬/ }));
    await screen.findByRole('combobox', { name: '저장한 보기' });
    // 이 탭이 목록을 읽은 뒤 다른 탭이 같은 이름을 만들었다.
    stored.push({ id: 'v9', name: '새 보기', config: { view: 'list' }, version: 1 });

    await fireEvent.input(screen.getByRole('textbox', { name: '보기 이름' }), {
      target: { value: '새 보기' },
    });
    await fireEvent.click(screen.getByRole('button', { name: '지금 보기 저장' }));

    expect(
      await screen.findByText(/다른 탭에서 보기 "새 보기"을\(를\) 먼저 저장해/),
    ).toBeInTheDocument();
    expect(sent[0]).toMatchObject({ method: 'POST', path: '/api/items/i1/views' });
    expect(stored).toHaveLength(1);
    expect(screen.getByRole('textbox', { name: '보기 이름' })).toHaveValue('새 보기');
  });

  // CodeRabbit: 409 뒤 목록 다시 읽기가 실패하면 목록이 비어 다시 저장이 PUT 대신 POST(또 409)가 됐다. 충돌 응답의
  // 최신 보기를 목록에 남겨 다시 저장은 그 판으로 고친다.
  it('keeps the conflicting view from the 409 when reading the list again fails', async () => {
    const stored: Stored[] = [{ id: 'v1', name: '내 보기', config: { view: 'list' }, version: 1 }];
    const control: Control = { listFails: false, held: undefined };
    const sent = viewServer(stored, control);
    render(TaskBoard, { props: { item, workspaces: [], openTaskId: undefined } });
    // 저장한 보기는 접힌 "필터·정렬" 영역 안에 있다.
    await fireEvent.click(screen.getByRole('button', { name: /필터·정렬/ }));
    const select = await screen.findByRole('combobox', { name: '저장한 보기' });
    await screen.findByRole('option', { name: '내 보기' });
    await fireEvent.change(select, { target: { value: 'v1' } });
    const first = stored[0];
    if (first === undefined) throw new Error('stored');
    first.version = 2;
    control.listFails = true;

    await fireEvent.click(screen.getByRole('button', { name: '칸반' }));
    await fireEvent.click(screen.getByRole('button', { name: '지금 보기 저장' }));
    await screen.findByText(/다른 탭에서 보기 "내 보기"을\(를\) 먼저 저장해/);
    expect(screen.getByRole('option', { name: '내 보기' })).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: '지금 보기 저장' }));
    await waitFor(() => {
      expect(sent).toHaveLength(2);
    });

    expect(sent[1]).toMatchObject({ method: 'PUT', path: '/api/views/v1' });
    expect(sent[1]?.body).toMatchObject({ base_version: 2, config: { view: 'board' } });
  });

  // CodeRabbit: 저장을 기다리는 사이 목록에서 다른 보기를 고르면 늦은 응답이 선택을 저장한 보기로 되돌렸다(지우기
  // 단추가 화면과 다른 보기를 가리킴). 시작할 때의 선택이 그대로일 때만 선택을 바꾼다.
  it('does not move the selection the user changed while the save was in flight', async () => {
    const done = vi.spyOn(feedback, 'notifyDone');
    const stored: Stored[] = [
      { id: 'v1', name: '가 보기', config: { view: 'list' }, version: 1 },
      { id: 'v2', name: '나 보기', config: { view: 'calendar' }, version: 1 },
    ];
    const control: Control = { listFails: false, held: [] };
    const sent = viewServer(stored, control);
    render(TaskBoard, { props: { item, workspaces: [], openTaskId: undefined } });
    // 저장한 보기는 접힌 "필터·정렬" 영역 안에 있다.
    await fireEvent.click(screen.getByRole('button', { name: /필터·정렬/ }));
    const select = await screen.findByRole('combobox', { name: '저장한 보기' });
    await screen.findByRole('option', { name: '나 보기' });
    await fireEvent.change(select, { target: { value: 'v1' } });

    await fireEvent.click(screen.getByRole('button', { name: '지금 보기 저장' }));
    await waitFor(() => {
      expect(sent).toHaveLength(1);
    });
    await fireEvent.change(select, { target: { value: 'v2' } });
    for (const release of control.held ?? []) release();
    await waitFor(() => {
      expect(done).toHaveBeenCalledWith('보기 "가 보기"을(를) 저장했습니다.');
    });

    expect(select).toHaveValue('v2');
  });
});
