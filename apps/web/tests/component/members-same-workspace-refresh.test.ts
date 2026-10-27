import type { MembersView, Workspace } from '@fvoci/api-client';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { api } from '$lib/api/client';
import MembersScreen from '$lib/features/members/MembersScreen.svelte';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';

// R5 delta2 fixes §7: 초대·그룹 만들기가 성공하면 `onchanged()` → 앱이 워크스페이스 목록을 다시 읽어 같은 id의 새
// 워크스페이스 객체를 넘긴다. "다른 워크스페이스로 바뀌면 다시 읽는다" 효과가 객체가 바뀔 때마다 돌아 화면을 비우고
// 다시 그려, 성공 안내와 입력 요소(초점)를 지웠다. 그 효과는 워크스페이스 id가 바뀔 때만 돌고, 같은 id의 갱신(이름 등)은
// 화면에 그대로 반영된다.

const workspace: Workspace = {
  access: 'owner',
  id: 'ws-1',
  organization_id: 'org-1',
  name: '연구실',
  role: 'admin',
  is_owner: true,
  realtime_enabled: false,
};
const other: Workspace = { ...workspace, id: 'ws-2', name: '다른 공간' };
const invitation = {
  id: 'inv-1',
  email: 'kim@lab.example',
  role: 'member' as const,
  expires_at: '2026-11-03T00:00:00Z',
};

const view: MembersView = {
  can_manage_members: true,
  members: [
    {
      user_id: 'u1',
      display_name: '관리자',
      email: 'admin@example.test',
      role: 'admin',
      is_owner: true,
    },
  ],
  my_role: 'admin',
  i_am_owner: true,
  pending_owner: null,
};

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  me.userId = 'u1';
  localStorage.clear();
  vi.spyOn(api, 'members').mockResolvedValue(view);
  vi.spyOn(api, 'groups').mockResolvedValue([]);
  vi.spyOn(api, 'invitations').mockResolvedValue([]);
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  me.userId = '';
  localStorage.clear();
});

/** 앱처럼 `onchanged`에서 같은 id의 새 워크스페이스 객체(이름 바뀜)를 넘긴다. */
function renderWithAppRefresh(): void {
  let refreshed = 0;
  const view = render(MembersScreen, {
    props: {
      workspace,
      meId: 'u1',
      onchanged: () => {
        refreshed += 1;
        void view.rerender({ workspace: { ...workspace, name: `연구실 ${String(refreshed)}` } });
      },
    },
  });
}

async function submit(input: HTMLElement, value: string): Promise<void> {
  await fireEvent.input(input, { target: { value } });
  const form = input.closest('form');
  if (form === null) throw new Error('form');
  await fireEvent.submit(form);
}

describe('members screen after a refresh of the same workspace', () => {
  it('keeps the invite notice and input after the app passes a new object for the same id', async () => {
    const invite = vi.spyOn(api, 'invite').mockResolvedValue(invitation);
    renderWithAppRefresh();
    const email = await screen.findByLabelText('이메일');
    await submit(email, 'kim@lab.example');
    await waitFor(() => {
      expect(invite).toHaveBeenCalledOnce();
    });
    // 이름 같은 갱신은 화면에 반영된다.
    expect(
      await screen.findByRole('heading', {
        level: 1,
        name: m.members_heading({ workspace: '연구실 1' }),
      }),
    ).toBeInTheDocument();
    expect(screen.getByRole('status')).toHaveTextContent(
      m.members_invited({ email: 'kim@lab.example' }),
    );
    expect(screen.getByLabelText('이메일')).toBe(email);
    // 첫 읽기 + 초대 뒤 다시 읽기. 같은 id의 갱신으로 또 읽지 않는다.
    expect(api.members).toHaveBeenCalledTimes(2);
  });

  it('keeps the group name input element after creating a group', async () => {
    vi.spyOn(api, 'createGroup').mockResolvedValue({
      id: 'g1',
      name: '조교',
      members: [],
    });
    renderWithAppRefresh();
    const name = await screen.findByLabelText(m.members_new_group_name());
    await submit(name, '조교');
    await waitFor(() => {
      expect(api.createGroup).toHaveBeenCalledOnce();
    });
    await screen.findByRole('heading', {
      level: 1,
      name: m.members_heading({ workspace: '연구실 1' }),
    });
    expect(screen.getByLabelText(m.members_new_group_name())).toBe(name);
    expect(api.members).toHaveBeenCalledTimes(2);
  });

  it('starts over when only the user changes on the same workspace', async () => {
    vi.spyOn(api, 'invite').mockResolvedValue(invitation);
    render(MembersScreen, { props: { workspace, meId: 'u1', onchanged: () => undefined } });
    await submit(await screen.findByLabelText('이메일'), 'kim@lab.example');
    await screen.findByText(m.members_invited({ email: 'kim@lab.example' }));
    expect(api.members).toHaveBeenCalledTimes(2);

    // 같은 워크스페이스에서 사용자만 바뀌었다(다른 탭의 계정 전환). 앞 사용자의 화면을 두지 않고 다시 읽는다.
    me.userId = 'u2';
    await waitFor(() => {
      expect(api.members).toHaveBeenCalledTimes(3);
    });
    expect(api.members).toHaveBeenLastCalledWith('ws-1');
    expect(screen.getByRole('status')).toHaveTextContent('');
  });

  it('still starts over for another workspace', async () => {
    vi.spyOn(api, 'invite').mockResolvedValue(invitation);
    const view = render(MembersScreen, {
      props: { workspace, meId: 'u1', onchanged: () => undefined },
    });
    await submit(await screen.findByLabelText('이메일'), 'kim@lab.example');
    await screen.findByText(m.members_invited({ email: 'kim@lab.example' }));
    await view.rerender({ workspace: other });
    await waitFor(() => {
      expect(api.members).toHaveBeenLastCalledWith('ws-2');
    });
    expect(screen.getByRole('status')).toHaveTextContent('');
  });
});
