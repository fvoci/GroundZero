import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { api, type SearchResultView } from '$lib/api/client';
import SearchDialog from '$lib/features/search/SearchDialog.svelte';
import { org } from '$lib/org.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';
import { TEST_ORGANIZATION } from './fixtures/organization';

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  vi.spyOn(api, 'modules').mockResolvedValue([]);
  vi.spyOn(api, 'members').mockResolvedValue({
    members: [],
    i_am_owner: false,
    my_role: 'member',
    pending_owner: null,
  });
});
afterEach(() => vi.restoreAllMocks());

it('shows organization scope and uses workspace_id only to narrow it', async () => {
  const search = vi.spyOn(api, 'search').mockResolvedValue([]);
  render(SearchDialog, {
    open: true,
    memberOf: () => true,
    workspace: { id: 'ws1', name: '연구실' },
  });
  expect(
    await screen.findByText(m.search_organization_scope({ name: TEST_ORGANIZATION.name })),
  ).toBeVisible();
  await fireEvent.input(screen.getByRole('searchbox'), { target: { value: '계획' } });
  await waitFor(() => {
    expect(search).toHaveBeenLastCalledWith('org-1', '계획', null, {});
  });
  await fireEvent.click(
    screen.getByRole('checkbox', { name: m.search_workspace_scope({ name: '연구실' }) }),
  );
  await waitFor(() => {
    expect(search).toHaveBeenLastCalledWith('org-1', '계획', 'ws1', {});
  });
  await fireEvent.input(screen.getByLabelText(m.search_task_status()), {
    target: { value: ' 검토 대기 ' },
  });
  await waitFor(() => {
    expect(search).toHaveBeenLastCalledWith('org-1', '계획', 'ws1', {
      kind: 'task',
      status: '검토 대기',
    });
  });
});

it('does not search without an organization and discards an old organization response', async () => {
  const second = { ...TEST_ORGANIZATION, id: 'org-2', name: '다른 조직' };
  org.load('user-a', [TEST_ORGANIZATION, second], TEST_ORGANIZATION.id);
  let finish: (results: SearchResultView[]) => void = () => undefined;
  const search = vi
    .spyOn(api, 'search')
    .mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    )
    .mockResolvedValue([]);
  render(SearchDialog, { open: true, memberOf: () => true });
  await fireEvent.input(await screen.findByRole('searchbox'), { target: { value: '계획' } });
  await waitFor(() => {
    expect(search).toHaveBeenCalledOnce();
  });
  org.select(second.id);
  await waitFor(() => {
    expect(search).toHaveBeenLastCalledWith(second.id, '계획', null, {});
  });
  finish([]);
  org.reset();
  expect(await screen.findByText(m.app_organization_required())).toBeVisible();
  expect(screen.getByRole('searchbox')).toBeDisabled();
  expect(search).toHaveBeenCalledTimes(2);
});
