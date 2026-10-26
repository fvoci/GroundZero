import type { Group, MembersView, ReceivedInvitation, Workspace } from '@fvoci/api-client';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { api } from '$lib/api/client';
import MembersScreen from '$lib/features/members/MembersScreen.svelte';
import ReceivedInvitations from '$lib/features/members/ReceivedInvitations.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

// 멤버 화면: 첫 읽기 실패 뒤 다시 시도, 그룹 삭제 확인, 초대·받은 초대의 중복 요청 막기.
const workspace: Workspace = {
  id: 'ws-1',
  organization_id: 'org-1',
  name: '연구실',
  role: 'admin',
  is_owner: true,
  realtime_enabled: false,
};

const view: MembersView = {
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

const group: Group = { id: 'g-1', name: '세미나 준비', members: ['u1'] };

const pending = <T>(): Promise<T> => new Promise<T>(() => undefined);

beforeEach(async () => {
  await setLocale('ko', { reload: false });
});
afterEach(() => {
  vi.restoreAllMocks();
});

describe('members screen', () => {
  it('replaces "loading" with a retry after the first load fails', async () => {
    const members = vi
      .spyOn(api, 'members')
      .mockRejectedValueOnce(new Error('offline'))
      .mockResolvedValue(view);
    vi.spyOn(api, 'groups').mockResolvedValue([]);
    vi.spyOn(api, 'invitations').mockResolvedValue([]);
    render(MembersScreen, { props: { workspace, meId: 'u1', onchanged: () => undefined } });

    const retry = await screen.findByRole('button', { name: '다시 시도' });
    expect(screen.queryByText('불러오는 중…')).toBeNull();
    await fireEvent.click(retry);
    await waitFor(() => {
      expect(members).toHaveBeenCalledTimes(2);
    });
    expect(await screen.findByTestId('member-list')).toBeInTheDocument();
  });

  it('asks before deleting a group', async () => {
    vi.spyOn(api, 'members').mockResolvedValue(view);
    vi.spyOn(api, 'groups').mockResolvedValue([group]);
    vi.spyOn(api, 'invitations').mockResolvedValue([]);
    const del = vi.spyOn(api, 'deleteGroup').mockResolvedValue(undefined);
    render(MembersScreen, { props: { workspace, meId: 'u1', onchanged: () => undefined } });

    await fireEvent.click(await screen.findByRole('button', { name: '그룹 삭제' }));
    expect(del).not.toHaveBeenCalled();
    const dialog = await screen.findByRole('dialog');
    expect(dialog.textContent).toContain('‘세미나 준비’ 그룹을 삭제할까요?');
    await fireEvent.click(screen.getByRole('button', { name: '확인' }));
    await waitFor(() => {
      expect(del).toHaveBeenCalledWith('g-1');
    });
  });

  it('sends one invite even when submitted twice', async () => {
    vi.spyOn(api, 'members').mockResolvedValue(view);
    vi.spyOn(api, 'groups').mockResolvedValue([]);
    vi.spyOn(api, 'invitations').mockResolvedValue([]);
    const invite = vi.spyOn(api, 'invite').mockReturnValue(pending());
    render(MembersScreen, { props: { workspace, meId: 'u1', onchanged: () => undefined } });

    const input = await screen.findByLabelText('이메일');
    await fireEvent.input(input, { target: { value: 'kim@lab.example' } });
    const form = input.closest('form');
    if (form === null) throw new Error('form');
    await fireEvent.submit(form);
    await fireEvent.submit(form);
    expect(invite).toHaveBeenCalledTimes(1);
    expect(screen.getByRole('button', { name: '초대' })).toBeDisabled();
  });

  it('completion notice changes inside a status region that is always present', async () => {
    vi.spyOn(api, 'members').mockResolvedValue(view);
    vi.spyOn(api, 'groups').mockResolvedValue([]);
    vi.spyOn(api, 'invitations').mockResolvedValue([]);
    vi.spyOn(api, 'invite').mockResolvedValue({
      id: 'inv-1',
      email: 'kim@lab.example',
      role: 'member',
      expires_at: '2026-11-01T00:00:00Z',
    });
    render(MembersScreen, { props: { workspace, meId: 'u1', onchanged: () => undefined } });

    const status = screen.getByRole('status');
    expect(status.textContent).toBe('');
    const input = await screen.findByLabelText('이메일');
    await fireEvent.input(input, { target: { value: 'kim@lab.example' } });
    const form = input.closest('form');
    if (form === null) throw new Error('form');
    await fireEvent.submit(form);
    await waitFor(() => {
      expect(status.textContent).toContain('kim@lab.example');
    });
  });
});

describe('received invitations', () => {
  it('answers an invitation once even when clicked twice', async () => {
    const inv: ReceivedInvitation = {
      kind: 'workspace',
      id: 'inv-1',
      organization_id: 'org-1',
      organization_name: '조직',
      workspace_id: 'ws-1',
      workspace_name: '연구실',
      invited_by: '관리자',
      role: 'member',
      expires_at: '2026-11-01T00:00:00Z',
    };
    vi.spyOn(api, 'myInvitations').mockResolvedValue([inv]);
    const decline = vi.spyOn(api, 'declineInvitation').mockReturnValue(pending());
    render(ReceivedInvitations, { props: { onaccepted: () => Promise.resolve() } });

    const button = await screen.findByRole('button', { name: '거절' });
    await fireEvent.click(button);
    await fireEvent.click(button);
    expect(decline).toHaveBeenCalledTimes(1);
    expect(screen.getByRole('button', { name: '수락' })).toBeDisabled();
  });
});
