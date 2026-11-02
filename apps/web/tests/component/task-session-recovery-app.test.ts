import { cleanup, render, waitFor } from '@testing-library/svelte';
import { tick } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { Me } from '@fvoci/api-client';
import App from '../../src/app/App.svelte';
import { api } from '$lib/api/client';
import { activityApi } from '$lib/features/activity/api';
import { accountSafety, auth } from '$lib/features/feedback/feedback.svelte';
import { readSessionGeneration } from '$lib/features/auth/session-signal';
import {
  taskScreenApi,
  type BoardRead,
  type DetailRead,
  type DetailOptionsRead,
} from '$lib/features/tasks/screen-api';
import { timeData } from '$lib/features/time/data';
import { savedViewsApi } from '$lib/features/events/saved-views-api';
import { itemTaskPath, itemViewPath, router } from '$lib/router.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';
import { PROJECT_MODULE, TASK_ITEM, WORKSPACE, taskViews } from '../../stories/fixtures/synthetic';
import { TEST_ORGANIZATION } from './fixtures/organization';
import { TEST_STATUSES } from './fixtures/task-statuses';

function held<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<T>((ok, fail) => {
    resolve = ok;
    reject = fail;
  });
  return { promise, resolve, reject };
}
async function settled(): Promise<void> {
  await new Promise<void>((resolve) => {
    setTimeout(resolve, 0);
  });
  await tick();
}
function signal(): void {
  localStorage.setItem(
    'fvoci:auth-session',
    JSON.stringify({
      state: 'settled',
      generation: readSessionGeneration() + 1,
    }),
  );
  window.dispatchEvent(new StorageEvent('storage', { key: 'fvoci:auth-session' }));
}
const identity: Me = {
  id: 'viewer',
  display_name: 'viewer',
  is_server_admin: false,
  hour_cycle: '',
  locale: 'ko',
  time_zone: '',
  week_start: '',
  theme: 'system',
  limits: {
    max_title_chars: 200,
    max_document_bytes: 2_000_000,
    max_references_per_document: 500,
    max_document_depth: 10,
  },
};
const task = taskViews()[0];
if (task === undefined) throw new Error('missing task fixture');
const item = { ...TASK_ITEM, access: 'read' as const };
const detail: DetailRead = {
  viewer_id: identity.id,
  item,
  task: { ...task, title: 'confirmed task recovery' },
  tasks: [task],
  archived: false,
  assignees: [],
  dependencies: [],
  series: [],
  checklist: [],
};
const board: BoardRead = {
  ...detail,
  tasks: [detail.task],
  item_access: 'read',
  statuses: TEST_STATUSES,
};
const options: DetailOptionsRead = {
  viewer_id: identity.id,
  item_id: item.id,
  statuses: { state: 'ready', value: TEST_STATUSES },
  assignable: {
    state: 'ready',
    value: [{ user_id: 'person', display_name: 'confirmed candidate recovery' }],
  },
};
const profile = {
  leader: null,
  goal: '',
  scope: '',
  starts_on: null,
  ends_on: null,
  status: 'active' as const,
  details: {},
  members: [],
  groups: [],
  can_edit: false,
  can_manage_members: false,
  version: 1,
};
let originalLocks: PropertyDescriptor | undefined;
beforeEach(async () => {
  localStorage.clear();
  originalLocks = Object.getOwnPropertyDescriptor(navigator, 'locks');
  Object.defineProperty(navigator, 'locks', {
    configurable: true,
    value: { request: (_name: string, update: () => unknown) => Promise.resolve(update()) },
  });
  await setLocale('ko', { reload: false });
  router.guard = undefined;
  router.navigate(itemViewPath(WORKSPACE.id, item.id, 'board'));
  accountSafety.switching = false;
  accountSafety.restoring = false;
  auth.expired = false;
  vi.stubGlobal('matchMedia', (query: string) => ({
    matches: false,
    media: query,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
  }));
  vi.spyOn(globalThis, 'fetch').mockImplementation(() =>
    Promise.resolve(new Response('{}', { status: 404 })),
  );
  vi.spyOn(api, 'me').mockResolvedValue(identity);
  vi.spyOn(api, 'workspaces').mockResolvedValue([WORKSPACE]);
  vi.spyOn(api, 'myOrganizations').mockResolvedValue([
    { ...TEST_ORGANIZATION, id: WORKSPACE.organization_id },
  ]);
  vi.spyOn(api, 'modules').mockResolvedValue([]);
  vi.spyOn(api, 'wikiDocuments').mockResolvedValue([]);
  vi.spyOn(api, 'currentTimer').mockResolvedValue(null);
  vi.spyOn(api, 'itemProfile').mockResolvedValue(profile);
  vi.spyOn(api, 'taskBacklinks').mockResolvedValue([]);
  vi.spyOn(savedViewsApi, 'savedViews').mockResolvedValue([]);
  vi.spyOn(activityApi, 'unreadCount').mockResolvedValue(0);
  vi.spyOn(activityApi, 'mark').mockResolvedValue({ pin: false, favorite: false });
  vi.spyOn(activityApi, 'comments').mockResolvedValue([]);
  vi.spyOn(timeData, 'taskTime').mockResolvedValue({ estimate_minutes: null, mine_seconds: 0 });
  vi.spyOn(taskScreenApi, 'context').mockResolvedValue({
    viewer_id: identity.id,
    item,
    module: PROJECT_MODULE,
    documents: [],
  });
  vi.spyOn(taskScreenApi, 'board').mockResolvedValue(board);
  vi.spyOn(taskScreenApi, 'options').mockResolvedValue(options);
});
afterEach(() => {
  cleanup();
  if (originalLocks === undefined) Reflect.deleteProperty(navigator, 'locks');
  else Object.defineProperty(navigator, 'locks', originalLocks);
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  accountSafety.switching = false;
  accountSafety.restoring = false;
  auth.expired = false;
  router.guard = undefined;
});

it.each([401, 409])(
  'SR-298-3 never retries a discarded startup detail before /me, including %s',
  async (status) => {
    router.navigate(itemTaskPath(WORKSPACE.id, item.id, task.id));
    const initial = held<DetailRead>();
    const authority = held<Me | undefined>();
    vi.mocked(api.me).mockResolvedValueOnce(identity).mockReturnValue(authority.promise);
    const original = taskScreenApi.detail;
    const reads = vi
      .spyOn(taskScreenApi, 'detail')
      .mockReturnValueOnce(initial.promise)
      .mockImplementation(original);
    vi.mocked(globalThis.fetch).mockImplementation((input) => {
      const path =
        typeof input === 'string' ? input : input instanceof URL ? input.pathname : input.url;
      return Promise.resolve(
        path.endsWith('/detail-read')
          ? new Response(
              JSON.stringify({ error: status === 409 ? 'account_changed' : 'unauthenticated' }),
              { status, headers: { 'Content-Type': 'application/json' } },
            )
          : new Response('{}', { status: 404 }),
      );
    });
    vi.mocked(api.itemProfile).mockImplementationOnce(() => {
      signal();
      return Promise.resolve(profile);
    });
    render(App, { reload: vi.fn(), replace: vi.fn() });
    await waitFor(() => {
      expect(api.me).toHaveBeenCalledTimes(2);
    });
    initial.resolve(detail);
    await settled();
    expect(reads).toHaveBeenCalledOnce();
    expect(auth.expired).toBe(false);
    expect(accountSafety.switching).toBe(false);
  },
);

it('SR-298-3 does not continue authority checks or detail reads after App unmounts', async () => {
  router.navigate(itemTaskPath(WORKSPACE.id, item.id, task.id));
  const initial = held<DetailRead>();
  const authority = held<Me | undefined>();
  vi.mocked(api.me).mockResolvedValueOnce(identity).mockReturnValue(authority.promise);
  vi.spyOn(taskScreenApi, 'detail').mockReturnValueOnce(initial.promise).mockResolvedValue(detail);
  vi.mocked(api.itemProfile).mockImplementationOnce(() => {
    signal();
    return Promise.resolve(profile);
  });
  const view = render(App, { reload: vi.fn(), replace: vi.fn() });
  await waitFor(() => {
    expect(api.me).toHaveBeenCalledTimes(2);
  });
  initial.resolve(detail);
  await settled();
  view.unmount();
  authority.resolve(identity);
  await settled();
  expect(api.me).toHaveBeenCalledTimes(2);
  expect(taskScreenApi.detail).toHaveBeenCalledOnce();
});
