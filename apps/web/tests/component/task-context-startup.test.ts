import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { tick } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { Me, OrganizationSummary, Workspace } from '@fvoci/api-client';
import App from '../../src/app/App.svelte';
import { api } from '$lib/api/client';
import { activityApi } from '$lib/features/activity/api';
import { taskData } from '$lib/features/tasks/detail-data';
import { taskScreenApi, type TaskContext } from '$lib/features/tasks/screen-api';
import { accountSafety, auth } from '$lib/features/feedback/feedback.svelte';
import { itemPath, itemViewPath, router } from '$lib/router.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';
import { PROJECT_MODULE, TASK_ITEM, WORKSPACE } from '../../stories/fixtures/synthetic';
import { TEST_ORGANIZATION } from './fixtures/organization';
import { readSessionGeneration } from '$lib/features/auth/session-signal';
import { SESSION_CHECK_MAX_ATTEMPTS } from '$lib/features/auth/session-check';

let originalLocks: PropertyDescriptor | undefined;

function held<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => (resolve = done));
  return { promise, resolve };
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
const context: TaskContext = {
  viewer_id: identity.id,
  item: { ...TASK_ITEM, title: 'private task context' },
  module: PROJECT_MODULE,
  documents: [],
};

beforeEach(async () => {
  originalLocks = Object.getOwnPropertyDescriptor(navigator, 'locks');
  Object.defineProperty(navigator, 'locks', {
    configurable: true,
    value: { request: (_name: string, update: () => unknown) => Promise.resolve(update()) },
  });
  await setLocale('ko', { reload: false });
  router.guard = undefined;
  router.navigate(itemViewPath(WORKSPACE.id, TASK_ITEM.id, 'board'));
  accountSafety.switching = false;
  accountSafety.restoring = false;
  auth.expired = false;
  vi.stubGlobal('matchMedia', (query: string) => ({
    matches: false,
    media: query,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
  }));
  vi.spyOn(globalThis, 'fetch').mockResolvedValue(new Response('{}', { status: 404 }));
  vi.spyOn(api, 'me').mockResolvedValue(identity);
  vi.spyOn(api, 'modules').mockResolvedValue([]);
  vi.spyOn(api, 'wikiDocuments').mockResolvedValue([]);
  vi.spyOn(api, 'currentTimer').mockResolvedValue(null);
  vi.spyOn(api, 'myNotes').mockResolvedValue([]);
  vi.spyOn(api, 'item').mockRejectedValue(new Error('must consume the context'));
  vi.spyOn(api, 'module').mockRejectedValue(new Error('must consume the context'));
  vi.spyOn(api, 'itemProfile').mockResolvedValue({
    leader: null,
    goal: '',
    scope: '',
    starts_on: null,
    ends_on: null,
    status: 'active',
    details: {},
    members: [],
    groups: [],
    can_edit: true,
    can_manage_members: true,
    version: 1,
  });
  vi.spyOn(activityApi, 'unreadCount').mockResolvedValue(0);
  vi.spyOn(activityApi, 'mark').mockResolvedValue({ pin: false, favorite: false });
  vi.spyOn(taskData, 'syncRecurrences').mockResolvedValue(0);
  vi.spyOn(taskScreenApi, 'board').mockResolvedValue({
    viewer_id: identity.id,
    tasks: [],
    statuses: [],
    assignees: [],
    dependencies: [],
    series: [],
    checklist: [],
    item_access: 'owner',
    archived: false,
  });
});
afterEach(() => {
  cleanup();
  if (originalLocks === undefined) Reflect.deleteProperty(navigator, 'locks');
  else Object.defineProperty(navigator, 'locks', originalLocks);
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  router.guard = undefined;
  localStorage.removeItem('fvoci:auth-session');
});

it.each(['settled', 'logout-pending'] as const)(
  '#299 rejects a foreign non-task startup list after A returns with a %s signal',
  async (state) => {
    router.navigate('/notes');
    const publication = held<boolean>();
    const spaces = held<Workspace[]>();
    let publish: (() => boolean) | undefined;
    Object.defineProperty(navigator, 'locks', {
      configurable: true,
      value: {
        request: (_name: string, update: () => boolean) => {
          if (publish !== undefined) return Promise.resolve(update());
          publish = update;
          return publication.promise;
        },
      },
    });
    vi.spyOn(api, 'workspaces').mockImplementation(() =>
      vi.mocked(api.me).mock.calls.length >= 2 ? Promise.resolve([WORKSPACE]) : spaces.promise,
    );
    vi.spyOn(api, 'myOrganizations').mockResolvedValue([
      { ...TEST_ORGANIZATION, id: WORKSPACE.organization_id },
    ]);
    render(App);
    await waitFor(() => {
      expect(api.workspaces).toHaveBeenCalledOnce();
    });
    for (const step of ['B', 'A']) {
      localStorage.setItem(
        'fvoci:auth-session',
        JSON.stringify({
          state,
          generation: readSessionGeneration() + 1,
          sequence: step,
          startedAt: Date.now(),
          attemptId: `pending-${step}`,
        }),
      );
      window.dispatchEvent(new StorageEvent('storage', { key: 'fvoci:auth-session' }));
    }
    if (publish !== undefined) {
      const accepted = publish();
      expect(accepted).toBe(false);
      publication.resolve(accepted);
    }
    // A request started before the rejected check would have used B's session cookie.
    spaces.resolve([{ ...WORKSPACE, name: 'SR-298-1 B-session space' }]);
    await fireEvent.click(await screen.findByTestId('workspace-switcher'));
    await waitFor(() => {
      expect(vi.mocked(api.me).mock.calls.length).toBeGreaterThanOrEqual(2);
    });
    expect(screen.queryByRole('menuitem', { name: 'SR-298-1 B-session space' })).toBeNull();
    expect(screen.getByRole('menuitem', { name: WORKSPACE.name })).toBeInTheDocument();
  },
);

it('#299 rechecks notification churn beyond the safety retry limit without a retry error', async () => {
  router.navigate('/notes');
  let changes = 0;
  vi.spyOn(api, 'workspaces').mockImplementation(() => {
    if (changes > SESSION_CHECK_MAX_ATTEMPTS) return Promise.resolve([WORKSPACE]);
    changes += 1;
    localStorage.setItem(
      'fvoci:auth-session',
      JSON.stringify({ state: 'settled', generation: readSessionGeneration() + 1 }),
    );
    window.dispatchEvent(new StorageEvent('storage', { key: 'fvoci:auth-session' }));
    return Promise.resolve([{ ...WORKSPACE, name: 'SR-298-1 B-session space' }]);
  });
  vi.spyOn(api, 'myOrganizations').mockResolvedValue([
    { ...TEST_ORGANIZATION, id: WORKSPACE.organization_id },
  ]);
  render(App);
  await fireEvent.click(await screen.findByTestId('workspace-switcher'));
  expect(screen.queryByRole('menuitem', { name: 'SR-298-1 B-session space' })).toBeNull();
  expect(screen.getByRole('menuitem', { name: WORKSPACE.name })).toBeInTheDocument();
  expect(screen.queryByRole('alert')).toBeNull();
  expect(api.workspaces).toHaveBeenCalledTimes(SESSION_CHECK_MAX_ATTEMPTS + 2);
});

it.each(['notes', 'tasks'] as const)(
  '#299 restore rejects a foreign list after the session returns to A on %s',
  async (path) => {
    router.navigate(
      path === 'notes' ? '/notes' : itemViewPath(WORKSPACE.id, TASK_ITEM.id, 'board'),
    );
    vi.spyOn(taskScreenApi, 'context').mockResolvedValue(context);
    const publication = held<boolean>();
    const spaces = held<Workspace[]>();
    vi.spyOn(api, 'workspaces').mockResolvedValue([WORKSPACE]);
    vi.spyOn(api, 'myOrganizations').mockResolvedValue([
      { ...TEST_ORGANIZATION, id: WORKSPACE.organization_id },
    ]);
    render(App);
    await screen.findByTestId('workspace-switcher');
    let publish: (() => boolean) | undefined;
    Object.defineProperty(navigator, 'locks', {
      configurable: true,
      value: {
        request: (_name: string, update: () => boolean) => {
          publish = update;
          return publication.promise;
        },
      },
    });
    vi.mocked(api.workspaces).mockReturnValueOnce(spaces.promise);
    window.dispatchEvent(new PageTransitionEvent('pageshow', { persisted: true }));
    await waitFor(() => {
      expect(api.workspaces).toHaveBeenCalledTimes(2);
    });
    for (const step of ['B', 'A']) {
      localStorage.setItem(
        'fvoci:auth-session',
        JSON.stringify({
          state: 'settled',
          generation: readSessionGeneration() + 1,
          sequence: step,
        }),
      );
      window.dispatchEvent(new StorageEvent('storage', { key: 'fvoci:auth-session' }));
    }
    if (publish !== undefined) {
      const accepted = publish();
      expect(accepted).toBe(false);
      publication.resolve(accepted);
    }
    spaces.resolve([{ ...WORKSPACE, name: 'SR-298-1 restored B-session space' }]);
    await waitFor(() => {
      expect(screen.getByTestId('workspace-switcher')).toBeVisible();
    });
    await fireEvent.click(screen.getByTestId('workspace-switcher'));
    await waitFor(() => {
      expect(vi.mocked(api.me).mock.calls.length).toBeGreaterThanOrEqual(3);
    });
    expect(
      screen.queryByRole('menuitem', { name: 'SR-298-1 restored B-session space' }),
    ).toBeNull();
    expect(screen.getByRole('menuitem', { name: WORKSPACE.name })).toBeInTheDocument();
  },
);

it('#299 restore rechecks notification churn beyond the safety retry limit', async () => {
  router.navigate('/notes');
  vi.spyOn(api, 'workspaces').mockResolvedValue([WORKSPACE]);
  vi.spyOn(api, 'myOrganizations').mockResolvedValue([
    { ...TEST_ORGANIZATION, id: WORKSPACE.organization_id },
  ]);
  render(App);
  await screen.findByTestId('workspace-switcher');
  let changes = 0;
  vi.mocked(api.workspaces).mockImplementation(() => {
    if (changes > SESSION_CHECK_MAX_ATTEMPTS) return Promise.resolve([WORKSPACE]);
    changes += 1;
    localStorage.setItem(
      'fvoci:auth-session',
      JSON.stringify({ state: 'settled', generation: readSessionGeneration() + 1 }),
    );
    window.dispatchEvent(new StorageEvent('storage', { key: 'fvoci:auth-session' }));
    return Promise.resolve([{ ...WORKSPACE, name: 'SR-298-1 restored B-session space' }]);
  });
  window.dispatchEvent(new PageTransitionEvent('pageshow', { persisted: true }));
  await waitFor(() => {
    expect(screen.getByTestId('workspace-switcher')).toBeVisible();
  });
  await fireEvent.click(screen.getByTestId('workspace-switcher'));
  expect(screen.queryByRole('menuitem', { name: 'SR-298-1 restored B-session space' })).toBeNull();
  expect(screen.getByRole('menuitem', { name: WORKSPACE.name })).toBeInTheDocument();
  expect(screen.queryByRole('alert')).toBeNull();
  expect(api.workspaces).toHaveBeenCalledTimes(SESSION_CHECK_MAX_ATTEMPTS + 3);
});

it('#299 restore checks the original user again before accepting retried lists', async () => {
  router.navigate('/notes');
  vi.spyOn(api, 'workspaces').mockResolvedValue([WORKSPACE]);
  vi.spyOn(api, 'myOrganizations').mockResolvedValue([
    { ...TEST_ORGANIZATION, id: WORKSPACE.organization_id },
  ]);
  const replace = vi.fn();
  const reload = vi.fn();
  render(App, { replace, reload });
  await screen.findByTestId('workspace-switcher');
  vi.mocked(api.me)
    .mockResolvedValueOnce(identity)
    .mockResolvedValue({ ...identity, id: 'other-viewer' });
  vi.mocked(api.workspaces).mockImplementationOnce(() => {
    localStorage.setItem(
      'fvoci:auth-session',
      JSON.stringify({ state: 'settled', generation: readSessionGeneration() + 1 }),
    );
    window.dispatchEvent(new StorageEvent('storage', { key: 'fvoci:auth-session' }));
    return Promise.resolve([{ ...WORKSPACE, name: 'SR-298-1 restored B-session space' }]);
  });
  window.dispatchEvent(new PageTransitionEvent('pageshow', { persisted: true }));
  await waitFor(() => {
    expect(replace).toHaveBeenCalledWith('/notes');
  });
  expect(reload).not.toHaveBeenCalled();
  expect(api.workspaces).toHaveBeenCalledTimes(2);
  expect(screen.queryByTestId('workspace-switcher')).toBeNull();
});

it('SR-298-1 retries /me before starting task reads when publication sees a newer session', async () => {
  const publication = held<boolean>();
  const recheck = held<Me | undefined>();
  let publish: (() => boolean) | undefined;
  Object.defineProperty(navigator, 'locks', {
    configurable: true,
    value: {
      request: (_name: string, update: () => boolean) => {
        if (publish !== undefined) return Promise.resolve(update());
        publish = update;
        return publication.promise;
      },
    },
  });
  vi.mocked(api.me).mockResolvedValueOnce(identity).mockReturnValueOnce(recheck.promise);
  vi.spyOn(api, 'workspaces').mockResolvedValue([WORKSPACE]);
  vi.spyOn(api, 'myOrganizations').mockResolvedValue([
    { ...TEST_ORGANIZATION, id: WORKSPACE.organization_id },
  ]);
  vi.spyOn(taskScreenApi, 'context').mockResolvedValue(context);
  render(App);
  await waitFor(() => {
    expect(publish).toBeDefined();
  });
  localStorage.setItem(
    'fvoci:auth-session',
    JSON.stringify({ state: 'settled', generation: readSessionGeneration() + 1 }),
  );
  window.dispatchEvent(new StorageEvent('storage', { key: 'fvoci:auth-session' }));
  if (publish === undefined) throw new Error('publication did not start');
  const accepted = publish();
  expect(accepted).toBe(false);
  publication.resolve(accepted);
  await waitFor(() => {
    expect(
      vi.mocked(api.me).mock.calls.length >= 2 || vi.mocked(api.workspaces).mock.calls.length > 0,
    ).toBe(true);
  });
  expect(api.workspaces).not.toHaveBeenCalled();
  expect(api.myOrganizations).not.toHaveBeenCalled();
  expect(taskScreenApi.context).not.toHaveBeenCalled();
  expect(taskScreenApi.board).not.toHaveBeenCalled();
  expect(taskData.syncRecurrences).not.toHaveBeenCalled();
  expect(screen.queryByText(context.item.title)).toBeNull();
  recheck.resolve(identity);
  await screen.findByTestId('task-board');
});

it('SR-298-1 bounds repeated stale publication retries without starting task reads', async () => {
  Object.defineProperty(navigator, 'locks', {
    configurable: true,
    value: {
      request: (_name: string, update: () => boolean) => {
        localStorage.setItem(
          'fvoci:auth-session',
          JSON.stringify({ state: 'settled', generation: readSessionGeneration() + 1 }),
        );
        return Promise.resolve(update());
      },
    },
  });
  vi.spyOn(api, 'workspaces').mockResolvedValue([WORKSPACE]);
  vi.spyOn(api, 'myOrganizations').mockResolvedValue([]);
  vi.spyOn(taskScreenApi, 'context').mockResolvedValue(context);
  render(App);
  await waitFor(() => {
    expect(
      vi.mocked(api.me).mock.calls.length === SESSION_CHECK_MAX_ATTEMPTS ||
        vi.mocked(api.workspaces).mock.calls.length > 0,
    ).toBe(true);
  });
  expect(api.workspaces).not.toHaveBeenCalled();
  expect(taskScreenApi.context).not.toHaveBeenCalled();
  expect(taskScreenApi.board).not.toHaveBeenCalled();
  expect(taskData.syncRecurrences).not.toHaveBeenCalled();
  await screen.findByRole('alert');
  expect(api.me).toHaveBeenCalledTimes(SESSION_CHECK_MAX_ATTEMPTS);
});

it('starts context with the session lists, then displays and synchronizes only after both gates', async () => {
  const spaces = held<Workspace[]>();
  const organizations = held<OrganizationSummary[]>();
  const early = held<TaskContext>();
  vi.spyOn(api, 'workspaces').mockReturnValue(spaces.promise);
  vi.spyOn(api, 'myOrganizations').mockReturnValue(organizations.promise);
  vi.spyOn(taskScreenApi, 'context').mockReturnValue(early.promise);
  render(App);
  await waitFor(() => {
    expect(taskScreenApi.context).toHaveBeenCalledOnce();
  });
  expect(api.workspaces).toHaveBeenCalledOnce();
  expect(api.myOrganizations).toHaveBeenCalledOnce();
  early.resolve(context);
  await Promise.resolve();
  expect(screen.queryByText(context.item.title)).toBeNull();
  expect(taskData.syncRecurrences).not.toHaveBeenCalled();
  spaces.resolve([WORKSPACE]);
  organizations.resolve([{ ...TEST_ORGANIZATION, id: WORKSPACE.organization_id }]);
  await screen.findByTestId('task-board');
  await waitFor(() => {
    expect(taskData.syncRecurrences).toHaveBeenCalledOnce();
  });
  expect(api.item).not.toHaveBeenCalled();
  expect(api.module).not.toHaveBeenCalled();
  expect(taskScreenApi.context).toHaveBeenCalledOnce();
});

it.each(['board', 'gantt'] as const)(
  'starts authorized %s reads before constructing item information',
  async (view) => {
    router.navigate(itemViewPath(WORKSPACE.id, TASK_ITEM.id, view));
    const order: string[] = [];
    vi.spyOn(api, 'workspaces').mockResolvedValue([WORKSPACE]);
    vi.spyOn(api, 'myOrganizations').mockResolvedValue([
      { ...TEST_ORGANIZATION, id: WORKSPACE.organization_id },
    ]);
    vi.spyOn(taskScreenApi, 'context').mockResolvedValue(context);
    const profile = await api.itemProfile(TASK_ITEM.id);
    vi.mocked(api.itemProfile).mockImplementation(() => {
      order.push('profile');
      return Promise.resolve(profile);
    });
    vi.mocked(taskData.syncRecurrences).mockImplementation(() => {
      order.push('sync');
      return Promise.resolve(0);
    });
    render(App);
    await waitFor(() => {
      expect(order).toContain('profile');
    });
    expect(order[0]).toBe('sync');
    expect(taskData.syncRecurrences).toHaveBeenCalledOnce();
    expect(taskScreenApi.board).toHaveBeenCalledOnce();
  },
);

it('leaves a discarded early result out of the next screen state', async () => {
  const early = held<TaskContext>();
  vi.spyOn(api, 'workspaces').mockResolvedValue([WORKSPACE]);
  vi.spyOn(api, 'myOrganizations').mockResolvedValue([
    { ...TEST_ORGANIZATION, id: WORKSPACE.organization_id },
  ]);
  vi.spyOn(taskScreenApi, 'context').mockReturnValue(early.promise);
  render(App);
  await waitFor(() => {
    expect(taskScreenApi.context).toHaveBeenCalledOnce();
  });
  router.navigate('/notes');
  early.resolve(context);
  await waitFor(() => {
    expect(api.myNotes).toHaveBeenCalledOnce();
  });
  expect(screen.queryByText(context.item.title)).toBeNull();
  expect(screen.queryByTestId('task-board')).toBeNull();
  expect(api.itemProfile).not.toHaveBeenCalled();
  expect(taskData.syncRecurrences).not.toHaveBeenCalled();
  expect(taskScreenApi.board).not.toHaveBeenCalled();
});

it.each(['pending', 'resolved'] as const)(
  'discards the first context after actual A → B → A navigation while it is %s',
  async (timing) => {
    const spaces = held<Workspace[]>();
    const organizations = held<OrganizationSummary[]>();
    const early = held<TaskContext>();
    const latest = held<TaskContext>();
    const workspaceList = [WORKSPACE];
    const organizationList = [{ ...TEST_ORGANIZATION, id: WORKSPACE.organization_id }];
    vi.spyOn(api, 'workspaces')
      .mockReturnValueOnce(spaces.promise)
      .mockResolvedValue(workspaceList);
    vi.spyOn(api, 'myOrganizations')
      .mockReturnValueOnce(organizations.promise)
      .mockResolvedValue(organizationList);
    vi.spyOn(taskScreenApi, 'context')
      .mockReturnValueOnce(early.promise)
      .mockReturnValueOnce(latest.promise);
    render(App);
    await waitFor(() => {
      expect(taskScreenApi.context).toHaveBeenCalledOnce();
    });
    if (timing === 'resolved') {
      early.resolve(context);
      await tick();
    }
    // Both changes happen before Svelte's next effect flush.
    router.navigate('/notes');
    router.navigate(itemViewPath(WORKSPACE.id, TASK_ITEM.id, 'board'));
    if (timing === 'pending') early.resolve(context);
    spaces.resolve(workspaceList);
    organizations.resolve(organizationList);
    await waitFor(() => {
      expect(taskScreenApi.context).toHaveBeenCalledTimes(2);
    });
    expect(screen.queryByText(context.item.title)).toBeNull();
    expect(taskData.syncRecurrences).not.toHaveBeenCalled();
    expect(taskScreenApi.board).not.toHaveBeenCalled();
    expect(api.itemProfile).not.toHaveBeenCalled();
    latest.resolve({ ...context, item: { ...context.item, title: 'fresh navigation context' } });
    await screen.findByTestId('task-board');
    expect(screen.queryByText(context.item.title)).toBeNull();
    expect(api.me).toHaveBeenCalledTimes(2);
  },
);

it('does not reuse a consumed startup context after leaving and returning', async () => {
  const latest = held<TaskContext>();
  vi.spyOn(api, 'workspaces').mockResolvedValue([WORKSPACE]);
  vi.spyOn(api, 'myOrganizations').mockResolvedValue([
    { ...TEST_ORGANIZATION, id: WORKSPACE.organization_id },
  ]);
  vi.spyOn(taskScreenApi, 'context')
    .mockResolvedValueOnce(context)
    .mockReturnValueOnce(latest.promise);
  render(App);
  await screen.findByTestId('task-board');
  expect(taskScreenApi.context).toHaveBeenCalledOnce();
  router.navigate('/notes');
  await waitFor(() => {
    expect(api.myNotes).toHaveBeenCalledOnce();
  });
  router.navigate(itemViewPath(WORKSPACE.id, TASK_ITEM.id, 'board'));
  await waitFor(() => {
    expect(taskScreenApi.context).toHaveBeenCalledTimes(2);
  });
  expect(screen.queryByText(context.item.title)).toBeNull();
  latest.resolve({ ...context, item: { ...context.item, title: 'fresh return context' } });
  await screen.findByTestId('task-board');
  expect(screen.queryByText(context.item.title)).toBeNull();
});

it.each([
  ['displayed', 'navigate'],
  ['pending', 'navigate'],
  ['displayed', 'replace'],
  ['pending', 'replace'],
] as const)(
  'invalidates a %s context on same-tick %s after session startup',
  async (timing, method) => {
    const oldFallback = held<TaskContext>();
    const latest = held<TaskContext>();
    vi.spyOn(api, 'workspaces').mockResolvedValue([WORKSPACE]);
    vi.spyOn(api, 'myOrganizations').mockResolvedValue([
      { ...TEST_ORGANIZATION, id: WORKSPACE.organization_id },
    ]);
    const read = vi.spyOn(taskScreenApi, 'context').mockResolvedValueOnce(context);
    if (timing === 'pending') read.mockReturnValueOnce(oldFallback.promise);
    read.mockReturnValueOnce(latest.promise);
    render(App);
    await screen.findByTestId('task-board');
    const target = itemViewPath(WORKSPACE.id, TASK_ITEM.id, 'board');
    if (timing === 'pending') {
      router.navigate('/notes');
      await waitFor(() => {
        expect(api.myNotes).toHaveBeenCalledOnce();
      });
      router.navigate(target);
      await waitFor(() => {
        expect(taskScreenApi.context).toHaveBeenCalledTimes(2);
      });
    }
    router[method]('/notes');
    router[method](target);
    oldFallback.resolve({ ...context, item: { ...context.item, title: 'stale fallback title' } });
    await waitFor(() => {
      expect(taskScreenApi.context).toHaveBeenCalledTimes(timing === 'pending' ? 3 : 2);
    });
    expect(screen.queryByText(context.item.title)).toBeNull();
    expect(screen.queryByText('stale fallback title')).toBeNull();
    latest.resolve({ ...context, item: { ...context.item, title: 'fresh same-tick context' } });
    await screen.findByTestId('task-board');
    expect(screen.queryByText(context.item.title)).toBeNull();
    expect(screen.queryByText('stale fallback title')).toBeNull();
  },
);

it('invalidates the item context after a generic item URL normalizes into tasks', async () => {
  const latest = held<TaskContext>();
  router.navigate(itemPath(WORKSPACE.id, TASK_ITEM.id));
  vi.spyOn(api, 'workspaces').mockResolvedValue([WORKSPACE]);
  vi.spyOn(api, 'myOrganizations').mockResolvedValue([
    { ...TEST_ORGANIZATION, id: WORKSPACE.organization_id },
  ]);
  vi.mocked(api.item).mockResolvedValue(context.item);
  vi.mocked(api.module).mockResolvedValue(context.module);
  vi.spyOn(api, 'itemDocuments').mockResolvedValue([]);
  vi.spyOn(taskScreenApi, 'context').mockReturnValue(latest.promise);
  render(App);
  await screen.findByTestId('task-board');
  expect(taskScreenApi.context).not.toHaveBeenCalled();
  router.replace('/notes');
  router.replace(itemViewPath(WORKSPACE.id, TASK_ITEM.id, 'board'));
  await waitFor(() => {
    expect(taskScreenApi.context).toHaveBeenCalledOnce();
  });
  expect(screen.queryByText(context.item.title)).toBeNull();
  latest.resolve({ ...context, item: { ...context.item, title: 'fresh normalized context' } });
  await screen.findByTestId('task-board');
  expect(screen.queryByText(context.item.title)).toBeNull();
});
