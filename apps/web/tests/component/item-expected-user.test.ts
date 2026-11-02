import { taskData as work } from '$lib/features/tasks/detail-data';
import './fixtures/task-statuses';
import type { Module, Workspace } from '@fvoci/api-client';
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { ItemProfileView, ItemView } from '$lib/api/client';
import { api } from '$lib/api/client';
import { accountSafety, onAccountSwitch } from '$lib/features/feedback/feedback.svelte';
import ItemProfilePanel from '$lib/features/modules/ItemProfilePanel.svelte';
import ItemScreen from '$lib/features/modules/ItemScreen.svelte';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';
import { openItemInfo } from './fixtures/item-groups';

// D-96(#307): 항목 정보·프로필 저장은 편집을 시작한 화면의 사용자를 기대 사용자 머리로 보낸다. 그사이 이 창의 사용자가
// 바뀌어도 앞 사용자를 보내 서버가 409 account_changed로 막고, 화면은 비교 창 대신 계정 전환 흐름으로 넘긴다.

const HEADER = 'x-fvoci-expected-user';

const workspace: Workspace = {
  access: 'owner',
  id: 'ws-1',
  organization_id: 'org-1',
  name: '공간',
  role: 'admin',
  is_owner: true,
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
  updated_at: '2026-11-02T00:00:00Z',
  status: 'active',
  is_template: false,
  version: 1,
  starts_on: null,
  ends_on: null,
};

const profile: ItemProfileView = {
  leader: null,
  goal: '처음 목표',
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

/** `PUT {path}`에 409 account_changed로 답하고 보낸 기대 사용자 머리를 모은다. 나머지 요청은 끝나지 않게 둔다. */
function refuse(path: string): (string | null)[] {
  const sent: (string | null)[] = [];
  vi.stubGlobal('fetch', (input: unknown, init?: RequestInit) => {
    if (input === path && init?.method === 'PUT') {
      sent.push(new Headers(init.headers).get(HEADER));
      return Promise.resolve(
        new Response(JSON.stringify({ error: 'account_changed', message: 'account changed' }), {
          status: 409,
          headers: { 'Content-Type': 'application/json' },
        }),
      );
    }
    return new Promise<Response>(() => undefined);
  });
  return sent;
}

let reload = vi.fn<() => void>();
let stop: () => void = () => undefined;

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  localStorage.clear();
  me.userId = 'user-a';
  accountSafety.switching = false;
  reload = vi.fn<() => void>();
  stop = onAccountSwitch(reload);
  vi.spyOn(api, 'item').mockResolvedValue(item);
  vi.spyOn(api, 'module').mockResolvedValue(mod);
  vi.spyOn(api, 'itemDocuments').mockResolvedValue([]);
  vi.spyOn(api, 'itemProfile').mockResolvedValue(profile);
  vi.spyOn(work, 'assignable').mockResolvedValue([]);
});
afterEach(() => {
  cleanup();
  stop();
  accountSafety.switching = false;
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  localStorage.clear();
  sessionStorage.clear();
  me.userId = '';
});

describe('item writes carry the user who started them', () => {
  it('sends the item reader on an item info save and hands account_changed to the account switch', async () => {
    const sent = refuse('/api/items/i1');
    render(ItemScreen, { props: { workspace, itemId: 'i1', workspaces: [workspace] } });
    await openItemInfo();
    const title = await waitFor(() => {
      const el = document.getElementById('item-title');
      if (!(el instanceof HTMLInputElement)) throw new Error('item-title');
      return el;
    });
    await fireEvent.input(title, { target: { value: '고친 제목' } });
    // 다른 탭에서 계정을 바꿔 이 창의 사용자 값이 먼저 바뀌었다. 저장은 항목을 읽은 사용자로 나가야 한다.
    me.userId = 'user-b';
    await fireEvent.click(
      within(screen.getByRole('form', { name: '항목 정보' })).getByRole('button', {
        name: '저장',
      }),
    );
    await waitFor(() => {
      expect(sent).toEqual(['user-a']);
    });
    await waitFor(() => {
      expect(reload).toHaveBeenCalledOnce();
    });
    expect(accountSafety.switching).toBe(true);
    expect(screen.queryByRole('region', { name: m.common_conflict_heading() })).toBeNull();
  });

  it('sends the user who opened the profile edit and hands account_changed to the account switch', async () => {
    const sent = refuse('/api/items/i1/profile');
    // 실제 클라이언트까지 그대로 보낸다(응답 해석·계정 전환 흐름도 함께 본다).
    const save = vi.spyOn(api, 'setItemProfile');
    const onchanged = vi.fn();
    render(ItemProfilePanel, { props: { item, kind: 'project', onchanged } });
    await fireEvent.click(await screen.findByRole('button', { name: m.modules_edit() }));
    const goal = document.querySelector<HTMLTextAreaElement>('#item-goal');
    if (goal === null) throw new Error('item-goal');
    await fireEvent.input(goal, { target: { value: '내 목표' } });
    // 이 창의 사용자가 바뀌면 편집 초안이 새 사용자로 다시 묶여 저장 자체가 나가지 않는다(form-draft). 그래서 여기서는
    // 패널이 편집을 연 사용자를 클라이언트에 명시해 넘기는지 본다(전역 기본값에 기대지 않음).
    await fireEvent.click(screen.getByRole('button', { name: m.common_save() }));
    await waitFor(() => {
      expect(sent).toEqual(['user-a']);
    });
    expect(save.mock.calls[0]?.[3]).toBe('user-a');
    await waitFor(() => {
      expect(reload).toHaveBeenCalledOnce();
    });
    expect(accountSafety.switching).toBe(true);
    expect(onchanged).not.toHaveBeenCalled();
    expect(screen.queryByRole('region', { name: m.common_conflict_heading() })).toBeNull();
  });

  it('pins the expected user given to the item info and profile clients', async () => {
    me.userId = 'user-b';
    const sent = refuse('/api/items/i1');
    await expect(
      api.updateItem('i1', 1, { title: '제목', description: '' }, 'user-a'),
    ).rejects.toMatchObject({ code: 'account_changed' });
    expect(sent).toEqual(['user-a']);
    // 앞 거절이 시작한 계정 전환은 다음 요청을 막는다. 두 번째 클라이언트를 보려고 전환 상태만 되돌린다.
    accountSafety.switching = false;
    const profileSent = refuse('/api/items/i1/profile');
    await expect(
      api.setItemProfile(
        'i1',
        1,
        {
          leader_id: null,
          goal: '',
          scope: '',
          starts_on: null,
          ends_on: null,
          status: 'active',
          details: {},
        },
        'user-a',
      ),
    ).rejects.toMatchObject({ code: 'account_changed' });
    expect(profileSent).toEqual(['user-a']);
  });
});
