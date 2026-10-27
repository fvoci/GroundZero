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
  // bits-ui Select는 jsdom에 없는 포인터 캡처·스크롤을 부른다(search-module-filter와 같은 준비).
  Element.prototype.hasPointerCapture = (): boolean => false;
  Element.prototype.setPointerCapture = (): void => undefined;
  Element.prototype.releasePointerCapture = (): void => undefined;
  Element.prototype.scrollIntoView = (): void => undefined;
  vi.spyOn(api, 'modules').mockResolvedValue([]);
  vi.spyOn(api, 'members').mockResolvedValue({
    members: [],
    i_am_owner: false,
    my_role: 'member',
    can_manage_members: false,
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
  // 상태 필터는 항목별 이름이 아니라 범주다(D-116): 고르면 업무만, status_category로 보낸다.
  const trigger = document.querySelector<HTMLElement>('[aria-labelledby="search-status-label"]');
  if (trigger === null) throw new Error('status trigger');
  await fireEvent.pointerDown(trigger, { button: 0, pointerType: 'mouse' });
  await fireEvent.click(trigger);
  const option = await screen.findByRole('option', { name: m.tasks_category_doing() });
  await fireEvent.pointerUp(option, { button: 0, pointerType: 'mouse' });
  await fireEvent.click(option);
  await waitFor(() => {
    expect(search).toHaveBeenLastCalledWith('org-1', '계획', 'ws1', {
      kind: 'task',
      statusCategory: 'doing',
    });
  });
});

it('does not search without an organization and discards an old organization response', async () => {
  const second = { ...TEST_ORGANIZATION, id: 'org-2', name: '다른 조직' };
  org.load('user-a', [TEST_ORGANIZATION, second], TEST_ORGANIZATION.id);
  // 두 조직의 결과를 구별되게 둬, 옛 조직 응답이 늦게 와도 새 조직 결과를 덮지 않는지 본다(R5 B 최종 FC-16).
  const found = (title: string): SearchResultView => ({
    kind: 'document',
    id: title,
    title,
    reference: null,
    workspace_id: 'ws1',
    item_id: null,
    document_id: null,
    snippet: null,
  });
  let finish: (results: SearchResultView[]) => void = () => undefined;
  const search = vi
    .spyOn(api, 'search')
    .mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    )
    .mockResolvedValue([found('다른 조직의 계획')]);
  render(SearchDialog, { open: true, memberOf: () => true });
  await fireEvent.input(await screen.findByRole('searchbox'), { target: { value: '계획' } });
  await waitFor(() => {
    expect(search).toHaveBeenCalledOnce();
  });
  org.select(second.id);
  await waitFor(() => {
    expect(search).toHaveBeenLastCalledWith(second.id, '계획', null, {});
  });
  expect(await screen.findByText('다른 조직의 계획')).toBeVisible();
  finish([found('옛 조직의 계획')]);
  await new Promise((r) => setTimeout(r, 0));
  expect(screen.queryByText('옛 조직의 계획')).toBeNull();
  expect(screen.getByText('다른 조직의 계획')).toBeVisible();
  org.reset();
  expect(await screen.findByText(m.app_organization_required())).toBeVisible();
  expect(screen.getByRole('searchbox')).toBeDisabled();
  expect(search).toHaveBeenCalledTimes(2);
});
