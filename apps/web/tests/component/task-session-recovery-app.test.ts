import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { tick } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { Me } from '@fvoci/api-client';
import App from '../../src/app/App.svelte';
import { api, ApiError } from '$lib/api/client';
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

it.each(
  (['view', 'close-detail'] as const).flatMap((navigation) =>
    (['success', 'failure'] as const).flatMap((outcome) =>
      (['same', 'other'] as const).map((user) => ({ navigation, outcome, user })),
    ),
  ),
)(
  'SR-298-8 reloads after $navigation discards the first $outcome only for the $user user',
  async ({ navigation, outcome, user }) => {
    if (navigation === 'close-detail')
      router.navigate(itemTaskPath(WORKSPACE.id, item.id, task.id));
    vi.spyOn(taskScreenApi, 'detail').mockResolvedValue(detail);
    const first = held<BoardRead>();
    const authority = held<Me | undefined>();
    vi.mocked(api.me).mockResolvedValueOnce(identity).mockReturnValue(authority.promise);
    vi.mocked(taskScreenApi.board)
      .mockReturnValueOnce(first.promise)
      .mockResolvedValue({
        ...board,
        tasks: [{ ...detail.task, title: 'current navigation board' }],
      });
    render(App, { reload: vi.fn(), replace: vi.fn() });
    if (navigation === 'close-detail') await screen.findByLabelText('제목');
    else await screen.findByRole('heading', { name: /^업무$/ });
    await waitFor(() => {
      expect(taskScreenApi.board).toHaveBeenCalledOnce();
    });
    if (navigation === 'view') {
      await fireEvent.click(screen.getByRole('button', { name: /^목록$/ }));
      expect(router.path).toBe(itemViewPath(WORKSPACE.id, item.id, 'list'));
    } else {
      await fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
      await waitFor(() => {
        expect(screen.queryByRole('dialog')).toBeNull();
      });
      expect(router.path).toBe(itemViewPath(WORKSPACE.id, item.id, 'board'));
    }
    if (outcome === 'failure')
      first.reject(new ApiError(403, 'forbidden', 'discarded board error'));
    else first.resolve(board);
    await waitFor(() => {
      expect(api.me).toHaveBeenCalledTimes(2);
    });
    expect(taskScreenApi.board).toHaveBeenCalledOnce();
    expect(screen.queryByText(detail.task.title)).toBeNull();
    const region = screen.getByRole('region', { name: /^업무$/ });
    expect(within(region).getByRole('status')).toHaveTextContent('불러오는 중');
    authority.resolve(user === 'same' ? identity : { ...identity, id: 'other-viewer' });
    if (user === 'other') {
      await waitFor(() => {
        expect(accountSafety.switching).toBe(true);
      });
      expect(taskScreenApi.board).toHaveBeenCalledOnce();
      return;
    }
    await screen.findByText('current navigation board');
    expect(within(region).queryByRole('status')).toBeNull();
    await settled();
    expect(taskScreenApi.board).toHaveBeenCalledTimes(2);
    expect(api.me).toHaveBeenCalledTimes(2);
    expect(screen.queryByRole('alert')).toBeNull();
  },
);

it.each(['other-item', 'other-screen'] as const)(
  'SR-298-8 never reloads a discarded board after leaving for %s',
  async (destination) => {
    const first = held<BoardRead>();
    vi.mocked(taskScreenApi.board)
      .mockReturnValueOnce(first.promise)
      .mockResolvedValue({
        ...board,
        tasks: [],
      });
    vi.spyOn(api, 'myNotes').mockResolvedValue([]);
    render(App, { reload: vi.fn(), replace: vi.fn() });
    await screen.findByRole('heading', { name: /^업무$/ });
    await waitFor(() => {
      expect(taskScreenApi.board).toHaveBeenCalledOnce();
    });
    const otherItem = { ...item, id: '00000000-0000-4000-8000-000000000099' };
    vi.mocked(taskScreenApi.context).mockResolvedValue({
      viewer_id: identity.id,
      item: otherItem,
      module: PROJECT_MODULE,
      documents: [],
    });
    router.navigate(
      destination === 'other-screen' ? '/notes' : itemViewPath(WORKSPACE.id, otherItem.id, 'board'),
    );
    if (destination === 'other-screen')
      await waitFor(() => {
        expect(api.myNotes).toHaveBeenCalledOnce();
      });
    else
      await waitFor(() => {
        expect(taskScreenApi.board).toHaveBeenCalledWith(otherItem.id);
      });
    first.resolve(board);
    await settled();
    expect(vi.mocked(taskScreenApi.board).mock.calls.filter(([id]) => id === item.id)).toHaveLength(
      1,
    );
    expect(api.me).toHaveBeenCalledOnce();
    expect(screen.queryByText(detail.task.title)).toBeNull();
  },
);

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

it('SR-298-4 waits for an in-flight same-user verification even with an expiry banner', async () => {
  const authority = held<Me | undefined>();
  const first = held<BoardRead>();
  vi.mocked(api.me).mockResolvedValueOnce(identity).mockReturnValue(authority.promise);
  vi.mocked(taskScreenApi.board).mockReturnValueOnce(first.promise);
  render(App, { reload: vi.fn(), replace: vi.fn() });
  await screen.findByRole('heading', { name: /^업무$/ });
  signal();
  await waitFor(() => {
    expect(api.me).toHaveBeenCalledTimes(2);
  });
  auth.expired = true;
  first.resolve(board);
  await settled();
  expect(taskScreenApi.board).toHaveBeenCalledOnce();
  authority.resolve(identity);
  await screen.findByText(detail.task.title);
  expect(auth.expired).toBe(false);
  expect(taskScreenApi.board).toHaveBeenCalledTimes(2);
});

it('SR-298-1 follow-up keeps rechecking authority across notification churn beyond the safety limit', async () => {
  const first = held<BoardRead>();
  vi.mocked(taskScreenApi.board).mockReturnValueOnce(first.promise);
  let changes = 0;
  vi.mocked(api.me)
    .mockResolvedValueOnce(identity)
    .mockImplementation(() => {
      if (changes < 5) {
        changes += 1;
        localStorage.setItem(
          'fvoci:auth-session',
          JSON.stringify({
            state: 'settled',
            generation: readSessionGeneration() + 1,
          }),
        );
      }
      return Promise.resolve(identity);
    });
  render(App, { reload: vi.fn(), replace: vi.fn() });
  await screen.findByRole('heading', { name: /^업무$/ });
  signal();
  first.resolve(board);
  await screen.findByText(detail.task.title);
  expect(api.me).toHaveBeenCalledTimes(7);
  expect(taskScreenApi.board).toHaveBeenCalledTimes(2);
  expect(accountSafety.switching).toBe(false);
  expect(auth.expired).toBe(false);
});

it.each(
  (['board', 'seed', 'detail', 'options', 'reopen'] as const).flatMap((kind) =>
    (['success', 'failure'] as const).flatMap((outcome) =>
      (['same', 'other'] as const).map((user) => ({ kind, outcome, user })),
    ),
  ),
)(
  'SR-298-4 recovers $kind $outcome only after App verifies the $user user',
  async ({ kind, outcome, user }) => {
    if (kind === 'seed' || kind === 'options' || kind === 'reopen')
      router.navigate(itemTaskPath(WORKSPACE.id, item.id, task.id));
    const readKind = kind === 'seed' || kind === 'reopen' ? 'detail' : kind;
    const authority = held<Me | undefined>();
    vi.mocked(api.me).mockResolvedValueOnce(identity).mockReturnValue(authority.promise);
    vi.spyOn(taskScreenApi, 'detail').mockResolvedValue(detail);
    const firstBoard = held<BoardRead>();
    const firstDetail = held<DetailRead>();
    const firstOptions = held<DetailOptionsRead>();
    const first =
      readKind === 'board' ? firstBoard : readKind === 'detail' ? firstDetail : firstOptions;
    if (readKind === 'board')
      vi.mocked(taskScreenApi.board).mockReturnValueOnce(firstBoard.promise);
    else if (readKind === 'detail') {
      if (kind === 'reopen') vi.mocked(taskScreenApi.detail).mockResolvedValueOnce(detail);
      vi.mocked(taskScreenApi.detail).mockReturnValueOnce(firstDetail.promise);
    } else vi.mocked(taskScreenApi.options).mockReturnValueOnce(firstOptions.promise);
    render(App, { reload: vi.fn(), replace: vi.fn() });
    if (kind === 'detail') {
      await screen.findByText(detail.task.title);
      router.navigate(itemTaskPath(WORKSPACE.id, item.id, task.id));
    } else if (kind === 'reopen') {
      await screen.findByLabelText('제목');
      router.navigate(itemViewPath(WORKSPACE.id, item.id, 'board'));
      await waitFor(() => {
        expect(screen.queryByLabelText('제목')).toBeNull();
      });
      router.navigate(itemTaskPath(WORKSPACE.id, item.id, task.id));
    }
    const before = kind === 'reopen' ? 2 : 1;
    await waitFor(() => {
      expect(taskScreenApi[readKind]).toHaveBeenCalledTimes(before);
    });
    if (kind === 'options') await screen.findByLabelText('제목');
    else await screen.findByRole('heading', { name: /^업무$/ });
    signal();
    await waitFor(() => {
      expect(api.me).toHaveBeenCalledTimes(2);
    });
    if (outcome === 'failure') first.reject(new ApiError(403, 'forbidden', 'old session failure'));
    else if (readKind === 'board') firstBoard.resolve(board);
    else if (readKind === 'detail') firstDetail.resolve(detail);
    else firstOptions.resolve(options);
    await settled();
    expect(taskScreenApi[readKind]).toHaveBeenCalledTimes(before);
    if (kind === 'board') {
      const region = screen.getByRole('region', { name: /^업무$/ });
      expect(within(region).getByRole('status')).toHaveTextContent('불러오는 중');
    }
    authority.resolve(user === 'same' ? identity : { ...identity, id: 'other-viewer' });
    if (user === 'other') {
      await waitFor(() => {
        expect(accountSafety.switching).toBe(true);
      });
      expect(taskScreenApi[readKind]).toHaveBeenCalledTimes(before);
      return;
    }
    if (kind === 'options') await screen.findByText('confirmed candidate recovery');
    else if (readKind === 'detail')
      expect(await screen.findByLabelText('제목')).toHaveValue(detail.task.title);
    else await screen.findByText(detail.task.title);
    expect(taskScreenApi[readKind]).toHaveBeenCalledTimes(before + 1);
  },
);

it.each(['board', 'detail', 'options'] as const)(
  'SR-298-4 preserves a current permission failure from the recovered %s read',
  async (kind) => {
    if (kind !== 'board') router.navigate(itemTaskPath(WORKSPACE.id, item.id, task.id));
    const authority = held<Me | undefined>();
    vi.mocked(api.me).mockResolvedValueOnce(identity).mockReturnValue(authority.promise);
    vi.spyOn(taskScreenApi, 'detail').mockResolvedValue(detail);
    const firstBoard = held<BoardRead>();
    const firstDetail = held<DetailRead>();
    const firstOptions = held<DetailOptionsRead>();
    const first = kind === 'board' ? firstBoard : kind === 'detail' ? firstDetail : firstOptions;
    const failure = new ApiError(403, 'forbidden', 'fresh request failure');
    if (kind === 'board')
      vi.mocked(taskScreenApi.board)
        .mockReturnValueOnce(firstBoard.promise)
        .mockRejectedValue(failure);
    else if (kind === 'detail')
      vi.mocked(taskScreenApi.detail)
        .mockReturnValueOnce(firstDetail.promise)
        .mockRejectedValue(failure);
    else
      vi.mocked(taskScreenApi.options)
        .mockReturnValueOnce(firstOptions.promise)
        .mockRejectedValue(failure);
    render(App, { reload: vi.fn(), replace: vi.fn() });
    await waitFor(() => {
      expect(taskScreenApi[kind]).toHaveBeenCalledOnce();
    });
    await screen.findByRole('heading', { name: /^업무$/ });
    signal();
    await waitFor(() => {
      expect(api.me).toHaveBeenCalledTimes(2);
    });
    first.reject(new ApiError(403, 'forbidden', 'discarded request failure'));
    await settled();
    expect(taskScreenApi[kind]).toHaveBeenCalledOnce();
    authority.resolve(identity);
    await screen.findByRole('alert');
    expect(taskScreenApi[kind]).toHaveBeenCalledTimes(2);
    expect(screen.queryByLabelText('제목')).toBeNull();
  },
);
