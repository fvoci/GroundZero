import { fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { api } from '$lib/api/client';
import { organizationsApi } from '$lib/features/organizations/api';
import OrganizationScreen from '$lib/features/organizations/OrganizationScreen.svelte';
import { me } from '$lib/me.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';
import { organizationPath, parseRoute, router } from '$lib/router.svelte';

const id = '00000000-0000-0000-0000-000000000001';
const organization = {
  id,
  name: '시험 조직',
  role: 'admin' as const,
  public_links_enabled: true,
  limits: {},
  created_at: '2026-10-01T00:00:00Z',
};
const members = [
  {
    user_id: 'user-a',
    display_name: '첫 관리자',
    email: 'admin@example.test',
    role: 'admin' as const,
    created_at: organization.created_at,
  },
];

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  me.userId = 'user-a';
  vi.spyOn(api, 'organization').mockResolvedValue(organization);
  vi.spyOn(organizationsApi, 'members').mockResolvedValue(members);
  router.replace(organizationPath(id));
});
afterEach(() => {
  vi.restoreAllMocks();
  me.userId = '';
  router.guard = undefined;
});

it('recognizes the four organization routes and defaults to members', () => {
  for (const tab of ['members', 'invitations', 'workspaces', 'settings'] as const)
    expect(parseRoute(organizationPath(id, tab))).toEqual({
      name: 'organization',
      organizationId: id,
      tab,
    });
  expect(parseRoute(`/o/${id}`)).toEqual({
    name: 'organization',
    organizationId: id,
    tab: 'members',
  });
  expect(parseRoute('/admin')).toEqual({ name: 'not-found' });
});

it('shows four tabs and member email to an administrator', async () => {
  render(OrganizationScreen, {
    organizationId: id,
    tab: 'members',
    meId: 'user-a',
    onchanged: () => undefined,
  });
  await screen.findByText('admin@example.test');
  expect(screen.getAllByRole('tab')).toHaveLength(4);
  expect(screen.getByRole('heading', { level: 1 })).toHaveTextContent('시험 조직');
});

it('shows only the member tab without email or actions to a member', async () => {
  vi.mocked(api.organization).mockResolvedValue({ ...organization, role: 'member' });
  render(OrganizationScreen, {
    organizationId: id,
    tab: 'members',
    meId: 'user-a',
    onchanged: () => undefined,
  });
  await screen.findByText('첫 관리자');
  expect(screen.getAllByRole('tab')).toHaveLength(1);
  expect(screen.queryByText('admin@example.test')).toBeNull();
  expect(screen.queryByRole('button', { name: /동작/ })).toBeNull();
});

it('pushes history when switching tabs', async () => {
  const push = vi.spyOn(window.history, 'pushState');
  render(OrganizationScreen, {
    organizationId: id,
    tab: 'members',
    meId: 'user-a',
    onchanged: () => undefined,
  });
  await screen.findByText('첫 관리자');
  await fireEvent.click(screen.getByRole('tab', { name: '초대' }));
  expect(push).toHaveBeenCalledWith(null, '', organizationPath(id, 'invitations'));
});
