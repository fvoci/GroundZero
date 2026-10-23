import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { ItemView } from '$lib/api/client';
import { api } from '$lib/api/client';
import TaskBoard from '$lib/features/tasks/TaskBoard.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

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

/** 저장한 보기만 다루는 작은 서버(D-103 계약). 보낸 요청을 모은다. */
function viewServer(
  stored: Stored[],
): { method: string; path: string; body: Record<string, unknown> }[] {
  const sent: { method: string; path: string; body: Record<string, unknown> }[] = [];
  vi.stubGlobal('fetch', (input: unknown, init?: RequestInit) => {
    const path = typeof input === 'string' ? input : '';
    const method = init?.method ?? 'GET';
    const raw: unknown = typeof init?.body === 'string' ? JSON.parse(init.body) : {};
    const body = typeof raw === 'object' && raw !== null ? { ...raw } : {};
    if (path === '/api/items/i1/views' && method === 'GET') {
      return Promise.resolve(response(200, stored));
    }
    if (method !== 'GET') sent.push({ method, path, body });
    if (path === '/api/items/i1/views' && method === 'POST') {
      const taken = stored.find((v) => v.name === body.name);
      if (taken !== undefined) {
        return Promise.resolve(response(409, { error: 'exists', current: taken }));
      }
      const created = {
        id: `v${stored.length + 1}`,
        name: String(body.name),
        config: {},
        version: 1,
      };
      stored.push(created);
      return Promise.resolve(response(201, created));
    }
    const target = stored.find((v) => path === `/api/views/${v.id}`);
    if (target !== undefined && method === 'PUT') {
      if (body.base_version !== target.version) {
        return Promise.resolve(response(409, { error: 'conflict', current: target }));
      }
      const config: unknown = body.config;
      if (typeof config === 'object' && config !== null) target.config = { ...config };
      target.version += 1;
      return Promise.resolve(response(200, target));
    }
    return new Promise<Response>(() => undefined);
  });
  return sent;
}

describe('saved view save on a stale version', () => {
  beforeEach(async () => {
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
});
