import type { MembersView, Workspace } from '@fvoci/api-client';
import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { api } from '$lib/api/client';
import MembersScreen from '$lib/features/members/MembersScreen.svelte';
import { workspaceLimitsApi } from '$lib/features/members/limits-api';
import { me } from '$lib/me.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';
import { membersPath, parseRoute, router } from '$lib/router.svelte';

const id = '0198f1f2-0000-7000-8000-000000000002';
const workspace: Workspace = {
  id,
  organization_id: 'org-1',
  name: '연구실',
  access: 'write',
  role: 'admin',
  is_owner: false,
};
const view: MembersView = {
  can_manage_members: true,
  my_role: 'admin',
  i_am_owner: false,
  pending_owner: null,
  members: [
    { user_id: 'u1', display_name: '공간 A 멤버', email: null, role: 'admin', is_owner: false },
  ],
};
const props = { workspace, meId: 'u1', onchanged: () => undefined };

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  me.userId = 'u1';
  localStorage.clear();
  router.replace(membersPath(id));
  vi.spyOn(api, 'members').mockResolvedValue(view);
  vi.spyOn(api, 'groups').mockResolvedValue([]);
  vi.spyOn(api, 'invitations').mockResolvedValue([]);
  vi.spyOn(api, 'organization').mockResolvedValue({
    id: 'org-1',
    name: '조직',
    role: 'admin',
    public_links_enabled: true,
    created_at: '2026-10-30T00:00:00Z',
    limits: {},
  });
  vi.spyOn(workspaceLimitsApi, 'get').mockResolvedValue({});
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  router.guard = undefined;
  me.userId = '';
  localStorage.clear();
});

describe('workspace management tabs (D-121, n1)', () => {
  it.each(['members', 'invitations', 'groups', 'settings'] as const)(
    'opens %s directly with the same workspace route context',
    (tab) => {
      expect(parseRoute(`/w/${id}/${tab}`)).toEqual({ name: 'members', workspaceId: id, tab });
      expect(parseRoute(`/w/${id}/${tab}/`)).toEqual({ name: 'members', workspaceId: id, tab });
    },
  );

  it.each([
    { tab: 'invitations', label: '초대', allowed: true },
    { tab: 'settings', label: '설정', allowed: true },
    { tab: 'invitations', label: '초대', allowed: false },
    { tab: 'settings', label: '설정', allowed: false },
  ] as const)(
    'keeps the requested $tab panel while permissions load (allowed: $allowed)',
    async ({ tab, label, allowed }) => {
      let finish: ((value: MembersView) => void) | undefined;
      vi.mocked(api.members).mockReturnValueOnce(
        new Promise((resolve) => {
          finish = resolve;
        }),
      );
      router.replace(membersPath(id, tab));
      render(MembersScreen, { props: { ...props, tab } });

      await vi.waitFor(() => {
        expect(api.members).toHaveBeenCalledWith(id);
      });
      expect(screen.getByRole('tab', { name: '멤버' })).toHaveAttribute('aria-selected', 'false');
      expect(screen.queryByRole('button', { name: '역할별 공유 상한' })).toBeNull();
      expect(screen.queryByTestId('member-list')).toBeNull();
      expect(screen.queryByLabelText('이메일')).toBeNull();
      expect(screen.queryByRole('tab', { name: label })).toBeNull();
      const loading = screen.getByLabelText('불러오는 중…');
      expect(loading).toHaveAttribute('aria-busy', 'true');
      expect(screen.getByRole('tabpanel')).toContainElement(loading);
      expect(router.path).toBe(membersPath(id, tab));
      expect(workspaceLimitsApi.get).not.toHaveBeenCalled();
      expect(api.invitations).not.toHaveBeenCalled();

      if (finish === undefined) throw new Error('missing permissions read');
      finish({ ...view, can_manage_members: allowed, my_role: allowed ? 'admin' : 'readonly' });
      if (allowed) {
        if (tab === 'invitations') await screen.findByLabelText('이메일');
        else await screen.findByRole('region', { name: '한도·보존' });
        expect(screen.getByRole('tab', { name: label })).toHaveAttribute('aria-selected', 'true');
        expect(screen.queryByRole('button', { name: '역할별 공유 상한' })).toBeNull();
        expect(router.path).toBe(membersPath(id, tab));
      } else {
        await screen.findByTestId('member-list');
        expect(screen.getByRole('tab', { name: '멤버' })).toHaveAttribute('aria-selected', 'true');
        expect(screen.queryByRole('tab', { name: label })).toBeNull();
        expect(router.path).toBe(membersPath(id));
        expect(workspaceLimitsApi.get).not.toHaveBeenCalled();
      }
    },
  );

  it.each(['admin', 'member', 'readonly'] as const)(
    'keeps existing visibility for %s',
    async (role) => {
      vi.mocked(api.members).mockResolvedValue({
        ...view,
        can_manage_members: role === 'admin',
        my_role: role,
      });
      render(MembersScreen, { props });
      await screen.findByTestId('member-list');
      expect(screen.getAllByRole('tab').map((tab) => tab.textContent.trim())).toEqual(
        role === 'admin' ? ['멤버', '초대', '그룹', '설정'] : ['멤버', '그룹'],
      );
      expect(screen.queryByLabelText('이메일')).toBeNull();
      expect(screen.queryByRole('region', { name: '한도·보존' })).toBeNull();
      expect(screen.queryByRole('button', { name: '워크스페이스 탈퇴' })).toBeNull();
      expect(workspaceLimitsApi.get).not.toHaveBeenCalled();
    },
  );

  it('collapses role limits above the member list', async () => {
    render(MembersScreen, { props });
    await screen.findByTestId('member-list');
    const toggle = screen.getByRole('button', { name: '역할별 공유 상한' });
    expect(toggle).toHaveAttribute('aria-expanded', 'false');
    await fireEvent.click(toggle);
    expect(toggle).toHaveAttribute('aria-expanded', 'true');
    expect(screen.getByText('공간 밖 조직원')).toBeVisible();
  });

  it('navigates tabs through history and guards invitation input', async () => {
    const push = vi.spyOn(window.history, 'pushState');
    const rendered = render(MembersScreen, { props });
    await screen.findByTestId('member-list');
    await fireEvent.click(screen.getByRole('tab', { name: '초대' }));
    expect(router.path).toBe(`/w/${id}/invitations`);
    expect(push).toHaveBeenCalledWith(null, '', `/w/${id}/invitations`);
    await rendered.rerender({ tab: 'invitations' });
    expect(screen.queryByTestId('member-list')).toBeNull();
    await fireEvent.input(await screen.findByLabelText('이메일'), {
      target: { value: 'draft@lab.test' },
    });
    const groupsTab = screen.getByRole('tab', { name: '그룹' });
    await fireEvent.mouseDown(groupsTab, { button: 0 });
    await fireEvent.click(groupsTab);
    await screen.findByRole('alertdialog');
    expect(router.path).toBe(`/w/${id}/invitations`);
    await fireEvent.click(screen.getByRole('button', { name: '계속 편집' }));
    expect(screen.getByRole('tab', { name: '초대' })).toHaveAttribute('aria-selected', 'true');
    expect(groupsTab).toHaveAttribute('aria-selected', 'false');
    expect(screen.getByRole('tabpanel')).toBeVisible();
    expect(screen.getByLabelText('이메일')).toHaveValue('draft@lab.test');
  });

  it.each(['member', 'readonly'] as const)('keeps groups and leaving for %s', async (role) => {
    vi.mocked(api.members).mockResolvedValue({ ...view, can_manage_members: false, my_role: role });
    render(MembersScreen, { props: { ...props, tab: 'groups' } });
    expect(await screen.findByRole('button', { name: '워크스페이스 탈퇴' })).toBeVisible();
    expect(screen.getByRole('heading', { name: '그룹' })).toBeVisible();
    expect(screen.queryByLabelText('새 그룹 이름')).toBeNull();
    expect(screen.queryByTestId('member-list')).toBeNull();
  });

  it.each(['invitations', 'settings'] as const)(
    'redirects a hidden %s tab to members',
    async (tab) => {
      vi.mocked(api.members).mockResolvedValue({
        ...view,
        can_manage_members: false,
        my_role: 'readonly',
      });
      router.replace(`/w/${id}/${tab}`);
      render(MembersScreen, { props: { ...props, tab } });
      await screen.findByTestId('member-list');
      expect(router.path).toBe(membersPath(id));
      expect(screen.queryByLabelText('이메일')).toBeNull();
      expect(workspaceLimitsApi.get).not.toHaveBeenCalled();
    },
  );

  it('orders limits, export and owner-only danger zone in settings', async () => {
    vi.mocked(api.members).mockResolvedValue({ ...view, i_am_owner: true });
    render(MembersScreen, { props: { ...props, tab: 'settings' } });
    await screen.findByRole('region', { name: '한도·보존' });
    expect(screen.getAllByRole('heading').map((heading) => heading.textContent.trim())).toEqual([
      '연구실 · 공간 관리',
      '한도·보존',
      '데이터 반출',
      '위험 구역',
      '워크스페이스 삭제',
    ]);
    expect(screen.queryByTestId('member-list')).toBeNull();
  });

  it('shows limits and export without deletion for a non-owner administrator', async () => {
    render(MembersScreen, { props: { ...props, tab: 'settings' } });
    await screen.findByRole('region', { name: '한도·보존' });
    expect(screen.getByRole('link', { name: 'ZIP으로 내려받기' })).toHaveAttribute(
      'href',
      `/api/workspaces/${id}/export`,
    );
    expect(screen.queryByRole('heading', { name: '위험 구역' })).toBeNull();
    expect(screen.queryByLabelText('이메일')).toBeNull();
  });

  it('keeps settings when the owner can only see the deletion section', async () => {
    vi.mocked(api.members).mockResolvedValue({
      ...view,
      can_manage_members: false,
      i_am_owner: true,
    });
    render(MembersScreen, { props: { ...props, tab: 'settings' } });
    await screen.findByRole('heading', { name: '위험 구역' });
    expect(screen.getAllByRole('tab').map((tab) => tab.textContent.trim())).toEqual([
      '멤버',
      '그룹',
      '설정',
    ]);
    expect(workspaceLimitsApi.get).not.toHaveBeenCalled();
    expect(screen.queryByRole('link', { name: 'ZIP으로 내려받기' })).toBeNull();
  });

  it('clears the previous workspace immediately and ignores a late read', async () => {
    let finish: ((value: MembersView) => void) | undefined;
    vi.mocked(api.members)
      .mockReturnValueOnce(
        new Promise((resolve) => {
          finish = resolve;
        }),
      )
      .mockResolvedValueOnce({
        ...view,
        can_manage_members: false,
        my_role: 'readonly',
        members: [],
      });
    const rendered = render(MembersScreen, { props: { ...props, tab: 'settings' } });
    await vi.waitFor(() => {
      expect(api.members).toHaveBeenCalledWith(id);
    });
    await rendered.rerender({ workspace: { ...workspace, id: 'ws-b' } });
    await screen.findByTestId('member-list');
    if (finish === undefined) throw new Error('missing workspace A read');
    finish({ ...view, i_am_owner: true });
    await vi.waitFor(() => {
      expect(screen.queryByText('공간 A 멤버')).toBeNull();
    });
    expect(screen.getAllByRole('tab')).toHaveLength(2);
    expect(workspaceLimitsApi.get).not.toHaveBeenCalled();
    expect(screen.queryByLabelText('공간 이름을 입력하세요')).toBeNull();
  });
});
