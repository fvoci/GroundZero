import type { MembersView, Workspace } from '@fvoci/api-client';
import { fireEvent, render, screen, within } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { api } from '$lib/api/client';
import MembersScreen from '$lib/features/members/MembersScreen.svelte';
import { organizationsApi } from '$lib/features/organizations/api';
import OrganizationScreen from '$lib/features/organizations/OrganizationScreen.svelte';
import { me } from '$lib/me.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';
import { router } from '$lib/router.svelte';

const organization = {
  id: 'org-1',
  name: '조직',
  role: 'admin' as const,
  public_links_enabled: true,
  created_at: '2026-10-01T00:00:00Z',
  limits: {},
};
const members = Array.from({ length: 11 }, (_, index) => ({
  user_id: `user-${String(index)}`,
  display_name: `연구자 ${String(index)}`,
  email: `person${String(index)}@example.test`,
  role: 'member' as const,
  created_at: organization.created_at,
  is_owner: false,
}));
const workspace: Workspace = {
  id: 'ws-1',
  organization_id: organization.id,
  name: '공간',
  access: 'read',
  role: 'readonly',
  is_owner: false,
};
const view: MembersView = {
  members,
  can_manage_members: false,
  my_role: 'readonly',
  i_am_owner: false,
  pending_owner: null,
};

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  me.userId = 'user-0';
  vi.spyOn(api, 'organization').mockResolvedValue(organization);
  vi.spyOn(api, 'members').mockResolvedValue(view);
  vi.spyOn(api, 'groups').mockResolvedValue([]);
  vi.spyOn(api, 'invitations').mockResolvedValue([]);
  vi.spyOn(organizationsApi, 'members').mockResolvedValue(members);
});
afterEach(() => {
  vi.restoreAllMocks();
  me.userId = '';
  router.guard = undefined;
  localStorage.clear();
});

function organizationView() {
  return render(OrganizationScreen, {
    organizationId: organization.id,
    tab: 'members',
    meId: me.userId,
    onchanged: () => undefined,
  });
}

it.each([
  { locale: 'ko', hint: '서버 기본값을 사용합니다. 공간 값은 이 값 이하로만 정할 수 있습니다.' },
  { locale: 'en', hint: 'Uses the server default. Workspace values cannot exceed this value.' },
] as const)('separates the organization help sentences in $locale', async ({ locale, hint }) => {
  await setLocale(locale, { reload: false });
  render(OrganizationScreen, {
    organizationId: organization.id,
    tab: 'settings',
    meId: me.userId,
    onchanged: () => undefined,
  });
  expect(await screen.findAllByText(hint)).toHaveLength(2);
});

it('shows the organization total on its tab and filters locally by name or email', async () => {
  organizationView();
  const filter = await screen.findByRole('searchbox', { name: '이름·이메일로 멤버 검색' });
  expect(screen.getByRole('tab', { name: '멤버 11명' })).toBeVisible();
  const list = screen.getByRole('list', { name: '멤버' });
  await fireEvent.input(filter, { target: { value: '  PERSON3@EXAMPLE.TEST  ' } });
  expect(within(list).getAllByRole('listitem')).toHaveLength(1);
  expect(list).toHaveTextContent('연구자 3');
  const action = within(list).getByRole('button', { name: '연구자 3 동작' });
  expect(action.querySelector('svg')).not.toBeNull();
  expect(action).not.toHaveTextContent('동작');
  await fireEvent.input(filter, { target: { value: '연구자 10' } });
  expect(within(list).getAllByRole('listitem')).toHaveLength(1);
  expect(list).toHaveTextContent('연구자 10');
  await fireEvent.input(filter, { target: { value: '없는 이름' } });
  expect(within(list).queryAllByRole('listitem')).toHaveLength(0);
  expect(screen.getByText('검색 조건에 맞는 멤버가 없습니다.')).toBeVisible();
  expect(screen.getByRole('tab', { name: '멤버 11명' })).toBeVisible();
  await fireEvent.input(filter, { target: { value: '' } });
  expect(within(list).getAllByRole('listitem')).toHaveLength(11);
  expect(organizationsApi.members).toHaveBeenCalledOnce();
});

it('offers name search to ordinary organization members without exposing email or actions', async () => {
  vi.mocked(api.organization).mockResolvedValue({ ...organization, role: 'member' });
  vi.mocked(organizationsApi.members).mockResolvedValue(
    members.map(({ email: _email, ...member }) => member),
  );
  organizationView();
  const filter = await screen.findByRole('searchbox', { name: '이름으로 멤버 검색' });
  const list = screen.getByRole('list', { name: '멤버' });
  expect(list).not.toHaveTextContent('@example.test');
  expect(within(list).queryAllByRole('button')).toHaveLength(0);
  await fireEvent.input(filter, { target: { value: '연구자 4' } });
  expect(within(list).getAllByRole('listitem')).toHaveLength(1);
  expect(list).toHaveTextContent('연구자 4');
  expect(screen.getAllByRole('tab')).toHaveLength(1);
});

it('shows the workspace total and filters only the member list without another read', async () => {
  render(MembersScreen, { workspace, meId: me.userId, onchanged: () => undefined });
  const filter = await screen.findByRole('searchbox', { name: '이름·이메일로 멤버 검색' });
  expect(screen.getByRole('heading', { name: '멤버 11명' })).toBeVisible();
  const list = screen.getByTestId('member-list');
  await fireEvent.input(filter, { target: { value: '  PERSON8@EXAMPLE.TEST  ' } });
  expect(within(list).getAllByRole('listitem')).toHaveLength(1);
  expect(list).toHaveTextContent('연구자 8');
  await fireEvent.input(filter, { target: { value: '연구자 10' } });
  expect(within(list).getAllByRole('listitem')).toHaveLength(1);
  expect(list).toHaveTextContent('연구자 10');
  await fireEvent.input(filter, { target: { value: '없는 이름' } });
  expect(within(list).queryAllByRole('listitem')).toHaveLength(0);
  expect(screen.getByText('검색 조건에 맞는 멤버가 없습니다.')).toBeVisible();
  await fireEvent.input(filter, { target: { value: '' } });
  expect(within(list).getAllByRole('listitem')).toHaveLength(11);
  expect(api.members).toHaveBeenCalledOnce();
});

it.each([0, 10])('does not show filters for %s members in either screen', async (count) => {
  vi.mocked(organizationsApi.members).mockResolvedValue(members.slice(0, count));
  const rendered = organizationView();
  await screen.findByRole('tab', { name: `멤버 ${String(count)}명` });
  expect(screen.queryByRole('searchbox')).toBeNull();
  rendered.unmount();
  vi.mocked(api.members).mockResolvedValue({ ...view, members: members.slice(0, count) });
  render(MembersScreen, { workspace, meId: me.userId, onchanged: () => undefined });
  await screen.findByTestId('member-list');
  expect(screen.getByRole('heading', { name: `멤버 ${String(count)}명` })).toBeVisible();
  expect(screen.queryByRole('searchbox')).toBeNull();
});
