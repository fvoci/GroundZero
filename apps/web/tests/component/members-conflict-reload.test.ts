import type { Invitation, MembersView, Workspace } from '@fvoci/api-client';
import { fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { api, ApiError } from '$lib/api/client';
import MembersScreen from '$lib/features/members/MembersScreen.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

// 초대 취소가 409이면(그사이 초대가 수락·철회됨, 서버 0081) 오류만 보이지 않고 지금 상태를 다시 읽어, 이미 닫힌
// 초대를 목록에서 뺀다. 이유는 다시 읽은 뒤에도 남는다.
const workspace: Workspace = {
  access: 'owner',
  id: 'ws-1',
  organization_id: 'org-1',
  name: '연구실',
  role: 'admin',
  is_owner: true,
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

const invitation: Invitation = {
  id: 'inv-1',
  email: 'kim@lab.example',
  role: 'member',
  expires_at: '2026-11-01T00:00:00Z',
};

describe('members screen after a conflicting request', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
    vi.spyOn(api, 'members').mockResolvedValue(view);
    vi.spyOn(api, 'groups').mockResolvedValue([]);
  });
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('reloads the list when revoking an invitation that was accepted meanwhile', async () => {
    const invitations = vi
      .spyOn(api, 'invitations')
      .mockResolvedValueOnce([invitation])
      .mockResolvedValue([]);
    const revoke = vi
      .spyOn(api, 'revokeInvitation')
      .mockRejectedValue(new ApiError(409, 'invalid_state', 'invitation is no longer open'));
    const changed = vi.fn();
    render(MembersScreen, { props: { workspace, meId: 'u1', onchanged: changed } });

    await screen.findByText('kim@lab.example');
    await fireEvent.click(screen.getByRole('button', { name: '취소' }));
    await vi.waitFor(() => {
      expect(revoke).toHaveBeenCalledWith('inv-1');
    });
    await vi.waitFor(() => {
      expect(invitations).toHaveBeenCalledTimes(2);
    });
    await vi.waitFor(() => {
      expect(screen.queryByText('kim@lab.example')).toBeNull();
    });
    expect(changed).toHaveBeenCalled();
    expect(
      await screen.findByText(/요청을 처리하지 못했습니다\. 지금 상태에서는 할 수 없습니다/),
    ).toBeTruthy();
  });

  it('does not reload for an ordinary failure', async () => {
    const invitations = vi.spyOn(api, 'invitations').mockResolvedValue([invitation]);
    vi.spyOn(api, 'revokeInvitation').mockRejectedValue(
      new ApiError(403, 'forbidden', 'admins only'),
    );
    render(MembersScreen, { props: { workspace, meId: 'u1', onchanged: () => undefined } });

    await screen.findByText('kim@lab.example');
    await fireEvent.click(screen.getByRole('button', { name: '취소' }));
    await screen.findByText(/요청을 처리하지 못했습니다/);
    expect(invitations).toHaveBeenCalledTimes(1);
    expect(screen.getByText('kim@lab.example')).toBeTruthy();
  });
});
