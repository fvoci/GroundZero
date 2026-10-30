// #211 / #245: behavior tests before connecting speculative reads to the UI.
import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { api } from '$lib/api/client';
import { homeApi } from '$lib/features/home/api';
import MyTasksScreen from '$lib/features/home/MyTasksScreen.svelte';
import ModuleScreen from '$lib/features/modules/ModuleScreen.svelte';
import {
  accountSafety,
  auth,
  hideSession,
  reportSessionExpired,
} from '$lib/features/feedback/feedback.svelte';
import {
  prefetchItem,
  prefetchMyTasks,
  readItem,
  readMyTasks,
} from '$lib/features/common/navigation-intent';
import { me } from '$lib/me.svelte';
import { org } from '$lib/org.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';
import { router } from '$lib/router.svelte';
import { homeView, PROJECT_MODULE, TASK_ITEM, WORKSPACE } from '../../stories/fixtures/synthetic';
import { TEST_ORGANIZATION } from './fixtures/organization';

const tasks = homeView().my_tasks;
const today = '2026-10-31';

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  hideSession('account');
  accountSafety.switching = false;
  accountSafety.restoring = false;
  auth.expired = false;
  me.userId = 'user-a';
  me.timeZone = 'Asia/Seoul';
  router.path = '/';
  vi.setSystemTime(new Date('2026-10-31T03:00:00Z'));
  vi.spyOn(homeApi, 'myTasks').mockResolvedValue(tasks);
  vi.spyOn(api, 'item').mockResolvedValue(TASK_ITEM);
});

afterEach(() => {
  cleanup();
  hideSession('account');
  accountSafety.switching = false;
  accountSafety.restoring = false;
  auth.expired = false;
  me.userId = '';
  router.path = '/';
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

it('deduplicates intent and consumes exactly one my-tasks read', async () => {
  prefetchMyTasks();
  prefetchMyTasks();
  expect(homeApi.myTasks).toHaveBeenCalledTimes(1);
  expect(await readMyTasks('org-1', today)).toEqual(tasks);
  expect(homeApi.myTasks).toHaveBeenCalledTimes(1);
  await readMyTasks('org-1', today);
  expect(homeApi.myTasks).toHaveBeenCalledTimes(2);
});

it('does not prefetch either read with Save-Data but still loads the destination', async () => {
  vi.stubGlobal('navigator', { connection: { saveData: true } });
  prefetchMyTasks();
  prefetchItem(TASK_ITEM.id);
  expect(homeApi.myTasks).not.toHaveBeenCalled();
  expect(api.item).not.toHaveBeenCalled();
  await readMyTasks('org-1', today);
  await readItem(TASK_ITEM.id);
  expect(homeApi.myTasks).toHaveBeenCalledTimes(1);
  expect(api.item).toHaveBeenCalledTimes(1);
});

it('does not reuse cached private data or start intent reads after a known session expiry', async () => {
  prefetchMyTasks();
  await Promise.resolve();
  reportSessionExpired();
  prefetchMyTasks();
  await readMyTasks('org-1', today);
  expect(homeApi.myTasks).toHaveBeenCalledTimes(2);
});

it('discards completed intent data as soon as account safety starts, even for the same user', async () => {
  prefetchMyTasks();
  prefetchItem(TASK_ITEM.id);
  await Promise.resolve();
  hideSession('restore');
  accountSafety.switching = false;
  accountSafety.restoring = false;
  await readMyTasks('org-1', today);
  await readItem(TASK_ITEM.id);
  expect(homeApi.myTasks).toHaveBeenCalledTimes(2);
  expect(api.item).toHaveBeenCalledTimes(2);
});

it('rejects an in-flight consumed response after the account changes', async () => {
  let finish: (value: typeof tasks) => void = () => undefined;
  vi.mocked(homeApi.myTasks).mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  prefetchMyTasks();
  const pending = readMyTasks('org-1', today);
  hideSession('account');
  me.userId = 'user-b';
  accountSafety.switching = false;
  finish(tasks);
  await expect(pending).rejects.toMatchObject({ code: 'account_changed' });
});

it('does not reuse across users or organizations, or after the short lifetime', async () => {
  prefetchItem(TASK_ITEM.id);
  me.userId = 'user-b';
  await readItem(TASK_ITEM.id);
  expect(api.item).toHaveBeenCalledTimes(2);
  prefetchMyTasks();
  org.load('user-b', [{ ...TEST_ORGANIZATION, id: 'org-2' }], 'org-2');
  await readMyTasks('org-2', today);
  expect(homeApi.myTasks).toHaveBeenCalledTimes(2);
  prefetchItem(TASK_ITEM.id);
  vi.setSystemTime(new Date('2026-10-31T03:00:06Z'));
  await readItem(TASK_ITEM.id);
  expect(api.item).toHaveBeenCalledTimes(4);
});

it('retries a failed speculative read only when the destination asks for it', async () => {
  vi.mocked(api.item).mockRejectedValueOnce(new Error('unavailable'));
  prefetchItem(TASK_ITEM.id);
  await Promise.resolve();
  expect(api.item).toHaveBeenCalledTimes(1);
  expect(await readItem(TASK_ITEM.id)).toEqual(TASK_ITEM);
  expect(api.item).toHaveBeenCalledTimes(2);
});

it('ignores intent while the account is hidden or the organization is unselected', () => {
  accountSafety.switching = true;
  prefetchItem(TASK_ITEM.id);
  prefetchMyTasks();
  accountSafety.switching = false;
  org.reset();
  prefetchItem(TASK_ITEM.id);
  prefetchMyTasks();
  expect(api.item).not.toHaveBeenCalled();
  expect(homeApi.myTasks).not.toHaveBeenCalled();
});

it.each(['mouseEnter', 'focus', 'touchStart'] as const)(
  'home my-tasks link handles %s without a write',
  async (event) => {
    render(MyTasksScreen, { source: { kind: 'summary', data: tasks } });
    const link = await screen.findByRole('link', { name: '내 업무 모두 보기' });
    await fireEvent[event](link);
    expect(homeApi.myTasks).toHaveBeenCalledTimes(1);
    await readMyTasks('org-1', today);
    expect(homeApi.myTasks).toHaveBeenCalledTimes(1);
  },
);

it.each(['mouseEnter', 'focus', 'touchStart'] as const)(
  'module item link handles %s and reuses only the item read',
  async (event) => {
    vi.spyOn(api, 'module').mockResolvedValue(PROJECT_MODULE);
    vi.spyOn(api, 'items').mockResolvedValue([TASK_ITEM]);
    render(ModuleScreen, { workspace: WORKSPACE, moduleId: PROJECT_MODULE.id });
    const link = await screen.findByRole('link', { name: new RegExp(TASK_ITEM.title) });
    await fireEvent[event](link);
    expect(api.item).toHaveBeenCalledTimes(1);
    await readItem(TASK_ITEM.id);
    expect(api.item).toHaveBeenCalledTimes(1);
  },
);
