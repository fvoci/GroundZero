import './fixtures/task-statuses';
import type { Module, Workspace } from '@fvoci/api-client';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { ItemProfileView, ItemView } from '$lib/api/client';
import { api } from '$lib/api/client';
import * as feedback from '$lib/features/feedback/feedback.svelte';
import ItemProfilePanel from '$lib/features/modules/ItemProfilePanel.svelte';
import ItemScreen from '$lib/features/modules/ItemScreen.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';
import './fixtures/authenticated';

// D-104(감사 P-30): 항목 정보(제목·설명·공개 범위)와 항목 정보 패널의 저장은 읽은 판을 보낸다. 그사이 다른 사람이
// 고쳤으면 서버는 409와 최신본을 돌려주고 아무것도 바꾸지 않는다. 화면은 입력을 그대로 두고 최신본을 보이며, 최신본을
// 불러오거나 내 입력을 최신 판 기준으로 다시 저장하게 한다. 입력 중에 항목을 다시 읽어도 기준 판을 옮기지 않는다(옮기면
// 그사이의 변경을 말없이 덮는다).

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

const workspace: Workspace = {
  access: 'owner',
  id: 'ws-1',
  organization_id: 'org-1',
  name: '공간',
  role: 'admin',
  is_owner: true,
  realtime_enabled: false,
};

const mod: Module = {
  access: 'owner',
  id: 'm1',
  workspace_id: 'ws-1',
  kind: 'project',
  name: '개발',
  key: 'DEV',
  previous_keys: [],
  archived: false,
  icon: '',
  description: '',
};

function item(over: Partial<ItemView> = {}): ItemView {
  return {
    id: 'i1',
    module_id: 'm1',
    workspace_id: 'ws-1',
    reference: 'DEV-1',
    title: '항목',
    description: '원래 설명',
    visibility: 'workspace',
    access: 'owner',
    archived: false,
    updated_at: '2026-10-23T00:00:00Z',
    status: 'active',
    is_template: false,
    version: 1,
    starts_on: null,
    ends_on: null,
    ...over,
  };
}

const profile: ItemProfileView = {
  leader: null,
  goal: '원래 목표',
  scope: '',
  starts_on: null,
  ends_on: null,
  status: 'active',
  details: {},
  members: [],
  groups: [],
  can_edit: true,
  can_manage_members: false,
  version: 1,
};

function response(status: number, json: unknown): Response {
  return new Response(json === undefined ? null : JSON.stringify(json), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });
}

/**
 * `PUT {path}` 요청에 차례로 `replies`를 돌려주고 보낸 본문을 모은다. 나머지 요청(하위 칸)은 끝나지 않게 둔다. 서버 응답
 * 수준에서 흉내 내므로 API 클라이언트의 409 해석까지 함께 본다.
 */
function serve(path: string, replies: [number, unknown][]): Record<string, unknown>[] {
  const sent: Record<string, unknown>[] = [];
  vi.stubGlobal('fetch', (input: unknown, init?: RequestInit) => {
    if (input === path && init?.method === 'PUT') {
      const body: unknown = JSON.parse(typeof init.body === 'string' ? init.body : '{}');
      if (typeof body === 'object' && body !== null) sent.push({ ...body });
      const [status, json] = replies.shift() ?? [500, { error: 'internal', message: 'x' }];
      return Promise.resolve(response(status, json));
    }
    return new Promise<Response>(() => undefined);
  });
  return sent;
}

function mockItem(...views: ItemView[]): void {
  const spy = vi.spyOn(api, 'item');
  for (const v of views) spy.mockResolvedValueOnce(v);
  spy.mockResolvedValue(views.at(-1) ?? item());
  vi.spyOn(api, 'module').mockResolvedValue(mod);
  vi.spyOn(api, 'itemDocuments').mockResolvedValue([]);
}

function field<T extends HTMLElement>(id: string, kind: new () => T): T {
  const el = document.getElementById(id);
  if (el instanceof kind) return el;
  throw new Error(id);
}

describe('item info save on a stale version', () => {
  it('keeps the input, shows the latest and saves mine again on the latest version', async () => {
    await setLocale('ko', { reload: false });
    mockItem(item());
    const done = vi.spyOn(feedback, 'notifyDone');
    const latest = item({ title: '다른 사람 제목', description: '다른 사람 설명', version: 2 });
    const sent = serve('/api/items/i1', [
      [409, { error: 'conflict', current: latest }],
      [200, item({ title: '내 제목', description: '내 설명', version: 3 })],
    ]);
    render(ItemScreen, { props: { workspace, itemId: 'i1', workspaces: [workspace] } });

    await fireEvent.input(await waitFor(() => field('item-title', HTMLInputElement)), {
      target: { value: '내 제목' },
    });
    await fireEvent.input(field('item-description', HTMLTextAreaElement), {
      target: { value: '내 설명' },
    });
    const form = screen.getByRole('form', { name: '항목 정보' });
    await fireEvent.click(within(form).getByRole('button', { name: '저장' }));

    const notice = await screen.findByText(
      /그사이 다른 사람이 이 항목의 제목·설명·공개 범위를 바꿔/,
    );
    const box = notice.closest('[data-item-conflict]');
    if (!(box instanceof HTMLElement)) throw new Error('conflict box');
    expect(within(box).getByText('다른 사람 제목')).toBeInTheDocument();
    expect(within(box).getByText('다른 사람 설명')).toBeInTheDocument();
    expect(sent[0]).toMatchObject({ base_version: 1, title: '내 제목', description: '내 설명' });
    // 입력은 그대로다.
    expect(field('item-title', HTMLInputElement).value).toBe('내 제목');
    expect(field('item-description', HTMLTextAreaElement).value).toBe('내 설명');

    await fireEvent.click(within(box).getByRole('button', { name: '내 입력으로 다시 저장' }));
    await waitFor(() => {
      expect(sent).toHaveLength(2);
    });
    expect(sent[1]).toMatchObject({ base_version: 2, title: '내 제목', description: '내 설명' });
    // 일반 완료는 토스트다(D-82).
    await waitFor(() => {
      expect(done).toHaveBeenCalledWith('저장했습니다.');
    });
    expect(document.querySelector('[data-item-conflict]')).toBeNull();
  });

  it('loads the latest into the form when asked', async () => {
    await setLocale('ko', { reload: false });
    mockItem(item());
    const latest = item({ title: '다른 사람 제목', description: '다른 사람 설명', version: 2 });
    serve('/api/items/i1', [[409, { error: 'conflict', current: latest }]]);
    render(ItemScreen, { props: { workspace, itemId: 'i1', workspaces: [workspace] } });

    await fireEvent.input(await waitFor(() => field('item-title', HTMLInputElement)), {
      target: { value: '내 제목' },
    });
    await fireEvent.click(
      within(screen.getByRole('form', { name: '항목 정보' })).getByRole('button', {
        name: '저장',
      }),
    );
    await fireEvent.click(await screen.findByRole('button', { name: '서버 내용 불러오기' }));

    expect(field('item-title', HTMLInputElement).value).toBe('다른 사람 제목');
    expect(field('item-description', HTMLTextAreaElement).value).toBe('다른 사람 설명');
    expect(document.querySelector('[data-item-conflict]')).toBeNull();
  });

  it('keeps the base version it was filled with when the item is read again while typing', async () => {
    await setLocale('ko', { reload: false });
    // 템플릿 표시 뒤 다시 읽은 항목은 그사이 다른 사람이 제목을 고쳐 판 2다.
    mockItem(item(), item({ is_template: true, title: '다른 사람이 고친 제목', version: 2 }));
    vi.spyOn(api, 'setItemTemplate').mockResolvedValue(undefined);
    const sent = serve('/api/items/i1', [[200, item({ description: '쓰는 중인 설명' })]]);
    render(ItemScreen, { props: { workspace, itemId: 'i1', workspaces: [workspace] } });

    await fireEvent.input(await waitFor(() => field('item-description', HTMLTextAreaElement)), {
      target: { value: '쓰는 중인 설명' },
    });
    await fireEvent.click(await screen.findByRole('button', { name: '템플릿으로 표시' }));
    await screen.findByRole('button', { name: '템플릿 해제' });
    await fireEvent.click(
      within(screen.getByRole('form', { name: '항목 정보' })).getByRole('button', {
        name: '저장',
      }),
    );
    await waitFor(() => {
      expect(sent).toHaveLength(1);
    });

    // 입력 중이던 화면은 판 1에서 채운 것이다. 판 2로 보내면 다른 사람이 고친 제목을 말없이 덮는다.
    expect(sent[0]).toMatchObject({ base_version: 1, description: '쓰는 중인 설명' });
  });
});

describe('item profile save on a stale version', () => {
  it('keeps the input, shows the latest and saves mine again on the latest version', async () => {
    await setLocale('ko', { reload: false });
    vi.spyOn(api, 'itemProfile').mockResolvedValue(profile);
    vi.spyOn(api, 'assignable').mockResolvedValue([]);
    const latest: ItemProfileView = { ...profile, goal: '다른 사람 목표', version: 2 };
    const sent = serve('/api/items/i1/profile', [
      [409, { error: 'conflict', current: latest }],
      [204, undefined],
    ]);
    const onchanged = vi.fn();
    render(ItemProfilePanel, { props: { item: item(), kind: 'project', onchanged } });

    await fireEvent.click(await screen.findByRole('button', { name: '고치기' }));
    await fireEvent.input(field('item-goal', HTMLTextAreaElement), {
      target: { value: '내 목표' },
    });
    await fireEvent.click(screen.getByRole('button', { name: '저장' }));

    const notice = await screen.findByText(/그사이 다른 사람이 항목 정보를 바꿔/);
    const box = notice.closest('[data-profile-conflict]');
    if (!(box instanceof HTMLElement)) throw new Error('conflict box');
    expect(within(box).getByText('다른 사람 목표')).toBeInTheDocument();
    expect(sent[0]).toMatchObject({ base_version: 1, goal: '내 목표' });
    expect(field('item-goal', HTMLTextAreaElement).value).toBe('내 목표');
    expect(onchanged).not.toHaveBeenCalled();

    await fireEvent.click(within(box).getByRole('button', { name: '내 입력으로 다시 저장' }));
    await waitFor(() => {
      expect(sent).toHaveLength(2);
    });
    expect(sent[1]).toMatchObject({ base_version: 2, goal: '내 목표' });
    await waitFor(() => {
      expect(onchanged).toHaveBeenCalledTimes(1);
    });
  });
});
