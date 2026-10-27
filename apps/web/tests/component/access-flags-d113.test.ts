import type { Workspace } from '@fvoci/api-client';
import { render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { api } from '$lib/api/client';
import MembersScreen from '$lib/features/members/MembersScreen.svelte';
import WorkspaceScreen from '$lib/features/workspaces/WorkspaceScreen.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';

// D-113: 웹의 쓰기·관리 표시는 역할 이름이 아니라 서버가 app_access로 계산한 칸(`access`, `can_manage_members`)을
// 따른다. 역할이 admin이어도 등급이 read면 관리 UI를 보이지 않는다(역할로 다시 계산하지 않는다).

function workspace(access: Workspace['access']): Workspace {
  return {
    id: 'ws-1',
    organization_id: 'org-1',
    name: '공간',
    role: 'admin',
    is_owner: false,
    realtime_enabled: false,
    access,
  };
}

const REALTIME = '실시간 공동 편집(문서를 여럿이 동시에 고침)';

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  vi.spyOn(api, 'wikiDocuments').mockResolvedValue([]);
  vi.spyOn(api, 'modules').mockResolvedValue([]);
  vi.spyOn(api, 'documentTemplates').mockResolvedValue([]);
  vi.spyOn(api, 'groups').mockResolvedValue([]);
  vi.spyOn(api, 'invitations').mockResolvedValue([]);
});
afterEach(() => {
  vi.restoreAllMocks();
});

describe('permission flags come from app_access (D-113)', () => {
  it('shows workspace administration only for the owner grade, whatever the role says', () => {
    const { unmount } = render(WorkspaceScreen, { props: { workspace: workspace('read') } });
    expect(screen.queryByRole('checkbox', { name: REALTIME })).toBeNull();
    unmount();
    render(WorkspaceScreen, { props: { workspace: workspace('owner') } });
    expect(screen.getByRole('checkbox', { name: REALTIME })).toBeInTheDocument();
  });

  it('shows member management only when the server says it is allowed', async () => {
    const view = (can: boolean) => ({
      members: [
        { user_id: 'u1', display_name: '나', email: null, role: 'admin' as const, is_owner: false },
      ],
      my_role: 'admin' as const,
      i_am_owner: false,
      can_manage_members: can,
      pending_owner: null,
    });
    const members = vi.spyOn(api, 'members').mockResolvedValue(view(false));
    const invitations = vi.spyOn(api, 'invitations').mockResolvedValue([]);
    const { unmount } = render(MembersScreen, {
      props: { workspace: workspace('read'), meId: 'u1', onchanged: () => undefined },
    });
    await screen.findByText('나');
    expect(screen.queryByRole('heading', { name: m.members_invite_heading() })).toBeNull();
    expect(invitations).not.toHaveBeenCalled();
    unmount();

    members.mockResolvedValue(view(true));
    render(MembersScreen, {
      props: { workspace: workspace('owner'), meId: 'u1', onchanged: () => undefined },
    });
    expect(
      await screen.findByRole('heading', { name: m.members_invite_heading() }),
    ).toBeInTheDocument();
    expect(invitations).toHaveBeenCalled();
  });
});
