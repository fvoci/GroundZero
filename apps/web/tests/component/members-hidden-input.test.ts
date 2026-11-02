import type { MembersView, Workspace } from '@fvoci/api-client';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { api, ApiError, NetworkError } from '$lib/api/client';
import { organizationsApi } from '$lib/features/organizations/api';
import MembersScreen from '$lib/features/members/MembersScreen.svelte';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';
import { router } from '$lib/router.svelte';
import { unloadBlocked } from './fixtures/leave-warning';

// #289: 초대 이메일·새 그룹 이름은 관리자에게, 삭제 확인 이름은 owner에게만 보인다. 다시 읽은 응답에서 그 권한이
// 빠지면 숨은 입력이 떠날 때 경고를 남기면 안 된다. 계속 보이는 입력은 그대로 보호한다(D-27, D-113).

const workspace: Workspace = {
  access: 'owner',
  id: 'ws-hidden',
  organization_id: 'org-1',
  name: '연구실',
  role: 'admin',
  is_owner: true,
};

function membersView(admin: boolean, owner: boolean): MembersView {
  return {
    can_manage_members: admin,
    members: [
      {
        user_id: 'u1',
        display_name: '나',
        email: 'me@example.test',
        role: admin ? 'admin' : 'member',
        is_owner: owner,
      },
    ],
    my_role: admin ? 'admin' : 'member',
    i_am_owner: owner,
    pending_owner: null,
  };
}

const deleteLabel = (): string => m.members_delete_name_label({ name: workspace.name });

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  me.userId = 'u1';
  localStorage.clear();
  vi.spyOn(api, 'members').mockResolvedValue(membersView(true, true));
  vi.spyOn(api, 'groups').mockResolvedValue([]);
  vi.spyOn(api, 'invitations').mockResolvedValue([]);
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  me.userId = '';
  localStorage.clear();
});

function renderScreen(tab: 'invitations' | 'groups' | 'settings') {
  return render(MembersScreen, {
    props: { workspace, tab, meId: 'u1', onchanged: () => undefined },
  });
}

it('drops hidden invitation and group input when a reload shows admin access ended', async () => {
  const createGroup = vi.spyOn(api, 'createGroup');
  const rendered = renderScreen('settings');
  await fireEvent.input(await screen.findByLabelText(deleteLabel()), {
    target: { value: '보이는 삭제 이름' },
  });
  await rendered.rerender({ tab: 'groups' });
  await fireEvent.input(await screen.findByLabelText(m.members_new_group_name()), {
    target: { value: '숨을 그룹' },
  });
  await rendered.rerender({ tab: 'invitations' });
  const email = await screen.findByLabelText(m.members_email());
  await fireEvent.input(email, { target: { value: 'gone@example.test' } });
  // 초대가 거절(409)되면 다시 읽고, 다시 읽은 응답에서 관리 권한이 빠졌다. owner는 남는다.
  vi.mocked(api.members).mockResolvedValue(membersView(false, true));
  vi.spyOn(organizationsApi, 'invite').mockRejectedValue(new ApiError(409, 'conflict', 'stale'));
  await fireEvent.submit(email.closest('form') ?? email);
  await waitFor(() => {
    expect(screen.queryByLabelText(m.members_email())).toBeNull();
  });
  await rendered.rerender({ tab: 'groups' });
  expect(screen.queryByLabelText(m.members_new_group_name())).toBeNull();
  // 삭제 확인 이름은 owner에게 계속 보이므로 경고가 남는다.
  await rendered.rerender({ tab: 'settings' });
  expect(await screen.findByLabelText(deleteLabel())).toHaveValue('보이는 삭제 이름');
  expect(unloadBlocked()).toBe(true);
  await fireEvent.input(screen.getByLabelText(deleteLabel()), { target: { value: '' } });
  expect(unloadBlocked()).toBe(false);
  expect(router.guard?.(vi.fn(), '/')).toBe(true);
  // 다시 관리자가 되면 서버 값(빈 칸)에서 시작한다.
  vi.mocked(api.members).mockResolvedValue(membersView(true, true));
  cleanup();
  renderScreen('invitations');
  expect(await screen.findByLabelText(m.members_email())).toHaveValue('');
  expect(createGroup).not.toHaveBeenCalled();
});

it('drops the hidden delete confirmation name when a reload shows ownership ended', async () => {
  vi.spyOn(api, 'createGroup').mockResolvedValue({ id: 'g1', name: '새 그룹', members: [] });
  const deleteWorkspace = vi.spyOn(api, 'deleteWorkspace');
  const rendered = renderScreen('settings');
  await fireEvent.input(await screen.findByLabelText(deleteLabel()), {
    target: { value: workspace.name },
  });
  await rendered.rerender({ tab: 'groups' });
  const group = await screen.findByLabelText(m.members_new_group_name());
  await fireEvent.input(group, { target: { value: '새 그룹' } });
  expect(unloadBlocked()).toBe(true);
  // 그룹을 만든 뒤 다시 읽은 응답에서 소유권이 빠졌다. 관리 권한은 남는다.
  vi.mocked(api.members).mockResolvedValue(membersView(true, false));
  await fireEvent.submit(group.closest('form') ?? group);
  await waitFor(() => {
    expect(screen.getByLabelText(m.members_new_group_name())).toHaveValue('');
  });
  await rendered.rerender({ tab: 'settings' });
  await waitFor(() => {
    expect(screen.queryByLabelText(deleteLabel())).toBeNull();
  });
  expect(unloadBlocked()).toBe(false);
  expect(router.guard?.(vi.fn(), '/')).toBe(true);
  expect(deleteWorkspace).not.toHaveBeenCalled();
});

it('keeps a sent invitation locked until its response, then drops the hidden email', async () => {
  let reject: (e: Error) => void = () => undefined;
  vi.spyOn(organizationsApi, 'invite').mockReturnValue(
    new Promise((_, fail) => {
      reject = fail;
    }),
  );
  vi.spyOn(api, 'createGroup').mockResolvedValue({ id: 'g1', name: '그룹', members: [] });
  const rendered = renderScreen('invitations');
  const email = await screen.findByLabelText(m.members_email());
  await fireEvent.input(email, { target: { value: 'sent@example.test' } });
  await fireEvent.submit(email.closest('form') ?? email);
  await rendered.rerender({ tab: 'groups' });
  const group = await screen.findByLabelText(m.members_new_group_name());
  await fireEvent.input(group, { target: { value: '그룹' } });
  vi.mocked(api.members).mockResolvedValue(membersView(false, false));
  await fireEvent.submit(group.closest('form') ?? group);
  await waitFor(() => {
    expect(screen.queryByLabelText(m.members_new_group_name())).toBeNull();
  });
  // 응답 전에는 이동을 막는다.
  expect(router.guard?.(vi.fn(), '/')).toBe(false);
  reject(new NetworkError('offline'));
  await waitFor(() => {
    expect(unloadBlocked()).toBe(false);
  });
  expect(router.guard?.(vi.fn(), '/')).toBe(true);
});
