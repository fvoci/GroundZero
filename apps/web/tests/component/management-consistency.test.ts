import type { MembersView, Workspace } from '@fvoci/api-client';
import { fireEvent, render, screen, within, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { api, ApiError } from '$lib/api/client';
import MembersScreen from '$lib/features/members/MembersScreen.svelte';
import { workspaceLimitsApi } from '$lib/features/members/limits-api';
import { organizationsApi } from '$lib/features/organizations/api';
import OrganizationScreen from '$lib/features/organizations/OrganizationScreen.svelte';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
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
  members: members.map((member) => ({ ...member, email: null })),
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
  vi.spyOn(workspaceLimitsApi, 'get').mockResolvedValue({});
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

function jsonResponse(value: unknown, status = 200): Response {
  return new Response(JSON.stringify(value), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

it.each([
  {
    kind: 'GET 500',
    fail: () =>
      Promise.resolve(jsonResponse({ error: 'internal', request_id: 'sr-257-members-500' }, 500)),
    expectedError: () => m.common_server_error({ requestId: 'sr-257-members-500' }),
  },
  {
    kind: 'network',
    fail: () => Promise.reject(new TypeError('Failed to fetch')),
    expectedError: () => m.organizations_load_failed(),
  },
  {
    kind: 'parsing',
    fail: () => Promise.resolve(jsonResponse({ invalid: 'members' })),
    expectedError: () => m.organizations_load_failed(),
  },
])(
  'SR-257-3: keeps direct settings on a member $kind failure and retries only members',
  async ({ fail, expectedError }) => {
    vi.mocked(organizationsApi.members).mockRestore();
    const fetch = vi
      .spyOn(globalThis, 'fetch')
      .mockImplementationOnce(fail)
      .mockResolvedValue(jsonResponse(members));
    const rendered = render(OrganizationScreen, {
      organizationId: organization.id,
      tab: 'settings',
      meId: me.userId,
      onchanged: () => undefined,
    });
    expect(await screen.findByRole('textbox', { name: '이름' })).toHaveValue(organization.name);
    expect(screen.getByRole('tab', { name: '설정' })).toHaveAttribute('aria-selected', 'true');
    expect(screen.getAllByRole('tab')).toHaveLength(4);
    expect(screen.getByRole('tab', { name: '멤버' })).toBeInTheDocument();
    expect(screen.queryByRole('tab', { name: '멤버 11명' })).toBeNull();
    await rendered.rerender({ tab: 'members' });
    const panel = screen.getByRole('tabpanel');
    expect(await within(panel).findByRole('alert')).toHaveTextContent(expectedError());
    expect(within(panel).queryByRole('list', { name: '멤버' })).toBeNull();
    expect(fetch).toHaveBeenCalledOnce();
    await fireEvent.click(within(panel).getByRole('button', { name: '다시 시도' }));
    await screen.findByRole('tab', { name: '멤버 11명' });
    expect(
      within(screen.getByRole('list', { name: '멤버' })).getAllByRole('listitem'),
    ).toHaveLength(11);
    expect(fetch).toHaveBeenCalledTimes(2);
    expect(api.organization).toHaveBeenCalledOnce();
  },
);

it.each(['invitations', 'workspaces'] as const)(
  'SR-257-3: keeps the $0 tab on a member server failure',
  async (tab) => {
    vi.mocked(organizationsApi.members).mockRejectedValue(
      new ApiError(500, 'internal', 'members unavailable'),
    );
    vi.spyOn(organizationsApi, 'invitations').mockResolvedValue([]);
    vi.spyOn(organizationsApi, 'workspaces').mockResolvedValue([]);
    render(OrganizationScreen, {
      organizationId: organization.id,
      tab,
      meId: me.userId,
      onchanged: () => undefined,
    });
    const label = tab === 'invitations' ? '초대' : '공간';
    expect(await screen.findByRole('tab', { name: label })).toHaveAttribute(
      'aria-selected',
      'true',
    );
    expect(screen.getAllByRole('tab')).toHaveLength(4);
    expect(screen.getByRole('tab', { name: '멤버' })).toBeInTheDocument();
    expect(screen.queryByRole('tab', { name: '멤버 11명' })).toBeNull();
  },
);

it.each([403, 404])(
  'SR-257-3: treats a member %s as loss of organization access',
  async (status) => {
    vi.mocked(organizationsApi.members).mockRejectedValue(
      new ApiError(status, status === 403 ? 'forbidden' : 'not_found', 'no access'),
    );
    render(OrganizationScreen, {
      organizationId: organization.id,
      tab: 'settings',
      meId: me.userId,
      onchanged: () => undefined,
    });
    await screen.findByText(m.organizations_unavailable());
    expect(screen.queryByTestId('management-screen')).toBeNull();
    expect(screen.queryAllByRole('tab')).toHaveLength(0);
  },
);

it('SR-257-3: opens settings while members are pending and keeps an edit when the count arrives', async () => {
  let finish: ((value: typeof members) => void) | undefined;
  const pending = new Promise<typeof members>((resolve) => {
    finish = resolve;
  });
  vi.mocked(organizationsApi.members).mockReturnValue(pending);
  render(OrganizationScreen, {
    organizationId: organization.id,
    tab: 'settings',
    meId: me.userId,
    onchanged: () => undefined,
  });
  const input = await screen.findByRole('textbox', { name: '이름' });
  expect(screen.getByRole('tab', { name: '멤버' })).toBeInTheDocument();
  await fireEvent.input(input, { target: { value: '편집 중인 조직 이름' } });
  if (finish === undefined) throw new Error('member read was not captured');
  finish(members);
  await screen.findByRole('tab', { name: '멤버 11명' });
  expect(input).toHaveValue('편집 중인 조직 이름');
  expect(screen.getByRole('tab', { name: '설정' })).toHaveAttribute('aria-selected', 'true');
});

it.each(['invitations', 'workspaces', 'settings'] as const)(
  'loads the member count on a direct organization $0 entry and reuses it across tabs',
  async (tab) => {
    vi.spyOn(organizationsApi, 'invitations').mockResolvedValue([]);
    vi.spyOn(organizationsApi, 'workspaces').mockResolvedValue([]);
    const rendered = render(OrganizationScreen, {
      organizationId: organization.id,
      tab,
      meId: me.userId,
      onchanged: () => undefined,
    });
    await screen.findByRole('tab', { name: '멤버 11명' });
    expect(organizationsApi.members).toHaveBeenCalledExactlyOnceWith(organization.id);
    await rendered.rerender({ tab: 'members' });
    expect(await screen.findByRole('list', { name: '멤버' })).toHaveTextContent('연구자 8');
    expect(organizationsApi.members).toHaveBeenCalledExactlyOnceWith(organization.id);
    await rendered.rerender({ tab });
    await rendered.rerender({ tab: 'members' });
    await screen.findByRole('list', { name: '멤버' });
    expect(organizationsApi.members).toHaveBeenCalledExactlyOnceWith(organization.id);
  },
);

it('clears the previous organization count and ignores a late old member response', async () => {
  let finish: ((value: typeof members) => void) | undefined;
  const oldMembers = new Promise<typeof members>((resolve) => {
    finish = resolve;
  });
  vi.mocked(organizationsApi.members)
    .mockReturnValueOnce(oldMembers)
    .mockResolvedValue(members.slice(0, 2));
  const rendered = render(OrganizationScreen, {
    organizationId: organization.id,
    tab: 'settings',
    meId: me.userId,
    onchanged: () => undefined,
  });
  await waitFor(() => {
    expect(api.organization).toHaveBeenCalledWith(organization.id);
  });
  const nextOrganization = { ...organization, id: 'org-2', name: '다음 조직' };
  vi.mocked(api.organization).mockResolvedValue(nextOrganization);
  await rendered.rerender({ organizationId: nextOrganization.id });
  await screen.findByRole('tab', { name: '멤버 2명' });
  if (finish === undefined) throw new Error('old organization member read was not captured');
  finish(members);
  await oldMembers;
  await waitFor(() => {
    expect(screen.getByRole('tab', { name: '멤버 2명' })).toBeInTheDocument();
    expect(screen.queryByRole('tab', { name: '멤버 11명' })).toBeNull();
  });
  await rendered.rerender({ tab: 'members' });
  const list = await screen.findByRole('list', { name: '멤버' });
  expect(within(list).getAllByRole('listitem')).toHaveLength(2);
  expect(organizationsApi.members).toHaveBeenCalledTimes(2);
});

it('clears a loaded organization count while switching to the next organization', async () => {
  const rendered = organizationView();
  await screen.findByRole('tab', { name: '멤버 11명' });
  const nextOrganization = { ...organization, id: 'org-2', name: '다음 조직' };
  vi.mocked(api.organization).mockResolvedValue(nextOrganization);
  let finish: ((value: typeof members) => void) | undefined;
  const nextMembers = new Promise<typeof members>((resolve) => {
    finish = resolve;
  });
  vi.mocked(organizationsApi.members).mockReturnValue(nextMembers);
  await rendered.rerender({ organizationId: nextOrganization.id });
  expect(screen.queryByRole('tab', { name: '멤버 11명' })).toBeNull();
  expect(screen.queryByText('연구자 8')).toBeNull();
  if (finish === undefined) throw new Error('next organization member read was not captured');
  finish(members.slice(0, 2));
  await screen.findByRole('tab', { name: '멤버 2명' });
});

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
    members.map((member) => ({
      user_id: member.user_id,
      display_name: member.display_name,
      role: member.role,
      created_at: member.created_at,
    })),
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
  vi.mocked(api.members).mockResolvedValue({ ...view, can_manage_members: true, members });
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

it.each([
  { locale: 'ko', label: '이름으로 멤버 검색', role: 'member' },
  { locale: 'ko', label: '이름으로 멤버 검색', role: 'readonly' },
  { locale: 'en', label: 'Search members by name', role: 'member' },
  { locale: 'en', label: 'Search members by name', role: 'readonly' },
] as const)(
  'SR-257-1: filters workspace $role by name in $locale with null emails',
  async ({ locale, label, role }) => {
    await setLocale(locale, { reload: false });
    vi.mocked(api.members).mockResolvedValue({ ...view, my_role: role });
    render(MembersScreen, {
      workspace: { ...workspace, role: 'admin' },
      meId: me.userId,
      onchanged: () => undefined,
    });
    const filter = await screen.findByRole('searchbox', { name: label });
    const list = screen.getByTestId('member-list');
    expect(list).not.toHaveTextContent('@example.test');
    await fireEvent.input(filter, { target: { value: '연구자 8' } });
    expect(within(list).getAllByRole('listitem')).toHaveLength(1);
    expect(list).toHaveTextContent('연구자 8');
    await fireEvent.input(filter, { target: { value: 'person8@example.test' } });
    expect(within(list).queryAllByRole('listitem')).toHaveLength(0);
    expect(api.members).toHaveBeenCalledOnce();
    expect(workspaceLimitsApi.get).not.toHaveBeenCalled();
  },
);

it.each([
  { kind: 'workspace', locale: 'ko', empty: '검색 조건에 맞는 멤버가 없습니다.' },
  { kind: 'organization', locale: 'ko', empty: '검색 조건에 맞는 멤버가 없습니다.' },
  { kind: 'workspace', locale: 'en', empty: 'No members match your search.' },
  { kind: 'organization', locale: 'en', empty: 'No members match your search.' },
] as const)(
  'SR-257-2: keeps the $kind search status mounted before and after filtering in $locale',
  async ({ kind, locale, empty }) => {
    await setLocale(locale, { reload: false });
    if (kind === 'organization') organizationView();
    else render(MembersScreen, { workspace, meId: me.userId, onchanged: () => undefined });
    const filter = await screen.findByRole('searchbox');
    const status = screen.getByRole('status', { hidden: true });
    expect(status).not.toHaveAttribute('hidden');
    expect(status.textContent).toBe('');
    await fireEvent.input(filter, { target: { value: '없는 이름' } });
    expect(screen.getByRole('status', { hidden: true })).toBe(status);
    expect(status).toHaveTextContent(empty);
    expect(status).toBeVisible();
    await fireEvent.input(filter, { target: { value: '연구자 8' } });
    expect(screen.getByRole('status', { hidden: true })).toBe(status);
    expect(status.textContent).toBe('');
  },
);

it.each([0, 10])('does not show filters for %s members in either screen', async (count) => {
  vi.mocked(organizationsApi.members).mockResolvedValue(members.slice(0, count));
  const rendered = organizationView();
  await screen.findByRole('tab', { name: `멤버 ${String(count)}명` });
  expect(screen.queryByRole('searchbox')).toBeNull();
  rendered.unmount();
  vi.mocked(api.members).mockResolvedValue({ ...view, members: view.members.slice(0, count) });
  render(MembersScreen, { workspace, meId: me.userId, onchanged: () => undefined });
  await screen.findByTestId('member-list');
  expect(screen.getByRole('heading', { name: `멤버 ${String(count)}명` })).toBeVisible();
  expect(screen.queryByRole('searchbox')).toBeNull();
});
