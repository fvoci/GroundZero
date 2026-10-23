import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { ItemProfileView, ItemView } from '$lib/api/client';
import { api } from '$lib/api/client';
import ItemProfilePanel from '$lib/features/modules/ItemProfilePanel.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

// 항목 정보 패널의 리더 칸은 입력(`form.leader`)만 보인다. 입력이 "없음"인데 서버의 리더 이름을 보이면, 409 뒤 "내 입력으로
// 다시 저장"이 화면에 보이지 않게 다른 사람이 정한 리더를 지운다(D-104 흐름에서 드러남).

const item: ItemView = {
  id: 'i1',
  module_id: 'm1',
  workspace_id: 'ws-1',
  reference: 'DEV-1',
  title: '항목',
  description: '',
  visibility: 'workspace',
  access: 'owner',
  archived: false,
  updated_at: '2026-10-23T00:00:00Z',
  status: 'active',
  is_template: false,
  version: 1,
  starts_on: null,
  ends_on: null,
};

const leader = { user_id: 'u1', display_name: 'Test User' };

const profile: ItemProfileView = {
  leader: null,
  goal: '',
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

/** `PUT /api/items/i1/profile`에 차례로 `replies`를 돌려주고 보낸 본문을 모은다. */
function serve(replies: [number, unknown][]): Record<string, unknown>[] {
  const sent: Record<string, unknown>[] = [];
  vi.stubGlobal('fetch', (input: unknown, init?: RequestInit) => {
    if (input === '/api/items/i1/profile' && init?.method === 'PUT') {
      const body: unknown = JSON.parse(typeof init.body === 'string' ? init.body : '{}');
      if (typeof body === 'object' && body !== null) sent.push({ ...body });
      const [status, json] = replies.shift() ?? [500, { error: 'internal', message: 'x' }];
      return Promise.resolve(response(status, json));
    }
    return new Promise<Response>(() => undefined);
  });
  return sent;
}

function leaderTrigger(): HTMLElement {
  const el = document.querySelector<HTMLElement>('[aria-labelledby="item-leader-label"]');
  if (el === null) throw new Error('leader trigger');
  return el;
}

async function pickLeader(name: string): Promise<void> {
  const trigger = leaderTrigger();
  await fireEvent.pointerDown(trigger, { button: 0, pointerType: 'mouse' });
  await fireEvent.click(trigger);
  const option = await screen.findByRole('option', { name });
  await fireEvent.pointerUp(option, { button: 0, pointerType: 'mouse' });
  await fireEvent.click(option);
}

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  Element.prototype.hasPointerCapture = (): boolean => false;
  Element.prototype.setPointerCapture = (): void => undefined;
  Element.prototype.releasePointerCapture = (): void => undefined;
  Element.prototype.scrollIntoView = (): void => undefined;
});

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe('item profile leader label', () => {
  it('keeps showing "none" after a conflict whose latest has a leader', async () => {
    vi.spyOn(api, 'itemProfile').mockResolvedValue(profile);
    vi.spyOn(api, 'assignable').mockResolvedValue([leader]);
    const latest: ItemProfileView = { ...profile, leader, version: 2 };
    const sent = serve([
      [409, { error: 'conflict', current: latest }],
      [204, undefined],
    ]);
    render(ItemProfilePanel, { props: { item, kind: 'project', onchanged: () => undefined } });

    await fireEvent.click(await screen.findByRole('button', { name: '고치기' }));
    await fireEvent.click(screen.getByRole('button', { name: '저장' }));
    const notice = await screen.findByText(/그사이 다른 사람이 항목 정보를 바꿔/);
    const box = notice.closest('[data-profile-conflict]');
    if (!(box instanceof HTMLElement)) throw new Error('conflict box');
    // 최신본 요약에는 다른 사람이 정한 리더가 보이고, 내 입력 칸은 그대로 "없음"이다.
    expect(within(box).getByText('Test User')).toBeInTheDocument();
    expect(leaderTrigger().textContent.trim()).toBe('없음');

    await fireEvent.click(within(box).getByRole('button', { name: '내 입력으로 다시 저장' }));
    await waitFor(() => {
      expect(sent).toHaveLength(2);
    });
    expect(sent[1]).toMatchObject({ base_version: 2, leader_id: null });
  });

  it('shows "none" once the leader is cleared in the form', async () => {
    vi.spyOn(api, 'itemProfile').mockResolvedValue({ ...profile, leader });
    vi.spyOn(api, 'assignable').mockResolvedValue([leader]);
    const sent = serve([[204, undefined]]);
    render(ItemProfilePanel, { props: { item, kind: 'project', onchanged: () => undefined } });

    await fireEvent.click(await screen.findByRole('button', { name: '고치기' }));
    await waitFor(() => {
      expect(leaderTrigger().textContent.trim()).toBe('Test User');
    });
    await pickLeader('없음');
    expect(leaderTrigger().textContent.trim()).toBe('없음');
    await fireEvent.click(screen.getByRole('button', { name: '저장' }));
    await waitFor(() => {
      expect(sent).toHaveLength(1);
    });
    expect(sent[0]).toMatchObject({ leader_id: null });
  });

  it('names the leader of the input, not the latest, when they differ', async () => {
    const other = { user_id: 'u2', display_name: 'Second User' };
    vi.spyOn(api, 'itemProfile').mockResolvedValue({ ...profile, leader });
    // 고를 수 있는 사람 목록에 입력의 리더가 없어도(목록을 못 읽음) 그 이름을 보인다.
    vi.spyOn(api, 'assignable').mockResolvedValue([]);
    serve([[409, { error: 'conflict', current: { ...profile, leader: other, version: 2 } }]]);
    render(ItemProfilePanel, { props: { item, kind: 'project', onchanged: () => undefined } });

    await fireEvent.click(await screen.findByRole('button', { name: '고치기' }));
    await fireEvent.click(screen.getByRole('button', { name: '저장' }));
    await screen.findByText(/그사이 다른 사람이 항목 정보를 바꿔/);
    expect(leaderTrigger().textContent.trim()).toBe('Test User');
  });
});
