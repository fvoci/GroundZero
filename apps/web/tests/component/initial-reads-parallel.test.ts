// #211 / #245 A: hold one response to prove independent reads are already in flight.
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { api, NetworkError, type CalendarView, type ItemView } from '$lib/api/client';
import { activityApi } from '$lib/features/activity/api';
import { homeApi } from '$lib/features/home/api';
import HomeDashboard from '$lib/features/home/HomeDashboard.svelte';
import ItemScreen from '$lib/features/modules/ItemScreen.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';
import { emptyHomeView, WORKSPACE } from '../../stories/fixtures/synthetic';

function held<T>(): {
  promise: Promise<T>;
  resolve: (value: T) => void;
  reject: (e: Error) => void;
} {
  let resolve: (value: T) => void = () => undefined;
  let reject: (e: Error) => void = () => undefined;
  const promise = new Promise<T>((ok, fail) => {
    resolve = ok;
    reject = fail;
  });
  return { promise, resolve, reject };
}

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  vi.spyOn(homeApi, 'home').mockResolvedValue(emptyHomeView());
  vi.spyOn(activityApi, 'notifications').mockResolvedValue({ notifications: [], next: null });
  vi.spyOn(activityApi, 'recentActivity').mockResolvedValue([]);
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

it('starts notifications and recent activity while a home calendar is pending', async () => {
  const calendar = held<CalendarView>();
  vi.spyOn(api, 'calendar').mockReturnValue(calendar.promise);
  const notes = vi.mocked(activityApi.notifications);
  const activity = vi.mocked(activityApi.recentActivity);
  render(HomeDashboard, { workspaces: [{ ...WORKSPACE, organization_id: 'org-1' }] });

  await waitFor(() => {
    expect(api.calendar).toHaveBeenCalledOnce();
  });
  await waitFor(() => {
    expect(notes).toHaveBeenCalledOnce();
    expect(activity).toHaveBeenCalledOnce();
  });
  expect(screen.queryByTestId('my-tasks-counts')).toBeNull();
  calendar.reject(new NetworkError('one calendar failed'));
  expect(await screen.findByTestId('my-tasks-counts')).toBeVisible();
  expect(activity).toHaveBeenCalledOnce();
});

it('keeps recent activity failure and retry independent of a loaded home', async () => {
  vi.spyOn(activityApi, 'recentActivity')
    .mockRejectedValueOnce(new NetworkError('activity failed'))
    .mockResolvedValueOnce([]);
  render(HomeDashboard, { workspaces: [] });
  expect(await screen.findByTestId('my-tasks-counts')).toBeVisible();
  await screen.findByRole('alert');
  await fireEvent.click(screen.getByRole('button', { name: '다시 시도' }));
  await waitFor(() => {
    expect(screen.queryByRole('alert')).toBeNull();
  });
  expect(activityApi.recentActivity).toHaveBeenCalledTimes(2);
});

it('starts item documents without waiting for the item response', async () => {
  const item = held<ItemView>();
  vi.spyOn(api, 'item').mockReturnValue(item.promise);
  const documents = vi.spyOn(api, 'itemDocuments').mockResolvedValue([]);
  render(ItemScreen, { workspace: WORKSPACE, workspaces: [WORKSPACE], itemId: 'parallel-item' });
  await waitFor(() => {
    expect(documents).toHaveBeenCalledWith('parallel-item');
  });
});
