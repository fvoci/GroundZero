import { workApi as work } from '$lib/features/tasks/api';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { api, NetworkError } from '$lib/api/client';
import HomeDashboard from '$lib/features/home/HomeDashboard.svelte';
import { me } from '$lib/me.svelte';
import { org } from '$lib/org.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';
import { homeView, taskViews } from '../../stories/fixtures/synthetic';
import { homeExtras } from './fixtures/home';

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  localStorage.clear();
  me.userId = 'user-a';
  homeExtras();
  const home = homeView();
  const item = home.active_items.find((value) => value.can_write);
  if (item === undefined) throw new Error('missing writable item');
  localStorage.setItem(org.key('quick-add-item'), item.id);
  vi.spyOn(api, 'home').mockResolvedValue(home);
  vi.spyOn(api, 'notifications').mockResolvedValue({ notifications: [], next: null });
});
afterEach(() => {
  cleanup();
  me.userId = '';
  vi.restoreAllMocks();
  localStorage.clear();
});

async function open(): Promise<void> {
  render(HomeDashboard, { workspaces: [] });
  await fireEvent.click(await screen.findByRole('button', { name: m.home_quick_add() }));
  await screen.findByLabelText(m.home_title());
}

it('offers the quick title and request ID after reload without sending before restore', async () => {
  const created = taskViews()[0];
  if (created === undefined) throw new Error('missing task');
  const add = vi
    .spyOn(work, 'createQuickTask')
    .mockRejectedValueOnce(new NetworkError('offline'))
    .mockResolvedValue({ ...created, checklist: [] });
  await open();
  await fireEvent.input(screen.getByLabelText(m.home_title()), {
    target: { value: '잃지 않을 제목' },
  });
  await fireEvent.click(screen.getByRole('button', { name: m.home_add() }));
  await screen.findByRole('alert');
  const first = add.mock.calls[0];
  cleanup();
  await open();
  expect(add).toHaveBeenCalledTimes(1);
  await fireEvent.click(await screen.findByRole('button', { name: m.common_draft_restore() }));
  expect(screen.getByLabelText(m.home_title())).toHaveValue('잃지 않을 제목');
  await fireEvent.click(screen.getByRole('button', { name: m.home_add() }));
  await waitFor(() => {
    expect(add).toHaveBeenCalledTimes(2);
  });
  expect(add.mock.calls[1]).toEqual(first);
});

it('warns on leaving with only a quick-add title', async () => {
  await open();
  await fireEvent.input(screen.getByLabelText(m.home_title()), { target: { value: '제목' } });
  const unload = new Event('beforeunload', { cancelable: true });
  window.dispatchEvent(unload);
  expect(unload.defaultPrevented).toBe(true);
});

it('keeps the next account input when an earlier account request finishes', async () => {
  const created = taskViews()[0];
  if (created === undefined) throw new Error('missing task');
  let complete: ((value: Awaited<ReturnType<typeof work.createQuickTask>>) => void) | undefined;
  const request = new Promise<Awaited<ReturnType<typeof work.createQuickTask>>>((resolve) => {
    complete = resolve;
  });
  const add = vi.spyOn(work, 'createQuickTask').mockReturnValue(request);
  await open();
  await fireEvent.input(screen.getByLabelText(m.home_title()), { target: { value: 'A의 제목' } });
  await fireEvent.click(screen.getByRole('button', { name: m.home_add() }));
  await waitFor(() => {
    expect(add).toHaveBeenCalledTimes(1);
  });

  me.userId = 'user-b';
  await waitFor(() => {
    expect(screen.getByRole('button', { name: m.home_quick_add() })).toHaveAttribute(
      'aria-expanded',
      'false',
    );
  });
  await fireEvent.click(screen.getByRole('button', { name: m.home_quick_add() }));
  const nextTitle = await screen.findByLabelText(m.home_title());
  expect(nextTitle).toHaveValue('');
  expect(screen.queryByRole('button', { name: m.common_draft_restore() })).toBeNull();
  await fireEvent.input(nextTitle, { target: { value: 'B의 제목' } });
  if (complete === undefined) throw new Error('missing completion');
  complete({ ...created, checklist: [] });
  await request;
  await waitFor(() => {
    expect(nextTitle).toHaveValue('B의 제목');
  });
  expect(api.home).toHaveBeenCalledTimes(1);
  expect(add.mock.calls[0]?.[0]).toBe('user-a');
  expect(add.mock.calls[0]?.[4]).toBe('user-a');

  cleanup();
  me.userId = 'user-a';
  await open();
  await fireEvent.click(await screen.findByRole('button', { name: m.common_draft_restore() }));
  expect(screen.getByLabelText(m.home_title())).toHaveValue('A의 제목');
});
