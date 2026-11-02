import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { tick } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { Me, OrganizationSummary, Workspace } from '@fvoci/api-client';
import App from '../../src/app/App.svelte';
import { api, ApiError } from '$lib/api/client';
import { activityApi } from '$lib/features/activity/api';
import { taskData } from '$lib/features/tasks/detail-data';
import { taskScreenApi, type TaskContext } from '$lib/features/tasks/screen-api';
import { accountSafety, auth, connection } from '$lib/features/feedback/feedback.svelte';
import { itemPath, itemViewPath, router } from '$lib/router.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';
import { PROJECT_MODULE, TASK_ITEM, WORKSPACE } from '../../stories/fixtures/synthetic';
import { TEST_ORGANIZATION } from './fixtures/organization';
import { readSessionGeneration } from '$lib/features/auth/session-signal';
import { SESSION_CHECK_MAX_ATTEMPTS } from '$lib/features/auth/session-check';
import { request } from '$lib/shared/api';

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
  localStorage.clear();
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
  vi.useRealTimers();
  connection.lost = false;
  if (originalLocks === undefined) Reflect.deleteProperty(navigator, 'locks');
  else Object.defineProperty(navigator, 'locks', originalLocks);
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  router.guard = undefined;
  localStorage.removeItem('fvoci:auth-session');
});

const viewerCases = (['notes', 'tasks', 'restore'] as const).flatMap((path) =>
  (['workspaces', 'organizations'] as const).map((list) => [path, list] as const),
);

const pacedCases = (['notes', 'tasks', 'restore'] as const).flatMap((path) =>
  (['generation', 'workspaces', 'organizations'] as const).map((source) => [path, source] as const),
);

it.each(pacedCases)(
  '#299 E paces %s rechecks from %s and keeps content closed until hints stop',
  async (path, source) => {
    router.navigate(
      path === 'tasks' ? itemViewPath(WORKSPACE.id, TASK_ITEM.id, 'board') : '/notes',
    );
    Object.defineProperty(navigator, 'locks', {
      configurable: true,
      value: { request: () => new Promise<never>(() => undefined) },
    });
    let churning = false;
    let delayed = false;
    vi.spyOn(taskScreenApi, 'context').mockResolvedValue(context);
    vi.mocked(fetch).mockImplementation((input) => {
      const url = typeof input === 'string' ? input : input instanceof URL ? input.href : input.url;
      const workspace = url === '/api/workspaces';
      if (!workspace && url !== '/api/me/organizations')
        return Promise.resolve(new Response('{}', { status: 404 }));
      const respond = (): Response => {
        if (churning && source === 'generation' && workspace)
          localStorage.setItem(
            'fvoci:auth-session',
            JSON.stringify({ state: 'settled', generation: readSessionGeneration() + 1 }),
          );
        const foreign = churning && source === (workspace ? 'workspaces' : 'organizations');
        const items = workspace
          ? [{ ...WORKSPACE, ...(foreign ? { name: 'E private B space' } : {}) }]
          : [{ ...TEST_ORGANIZATION, id: WORKSPACE.organization_id }];
        return new Response(JSON.stringify(items), {
          status: 200,
          headers: { 'x-fvoci-viewer-id': foreign ? 'other-viewer' : identity.id },
        });
      };
      return delayed
        ? new Promise<Response>((resolve) => {
            setTimeout(() => {
              resolve(respond());
            }, 1);
          })
        : Promise.resolve(respond());
    });
    if (path === 'restore') {
      render(App);
      await screen.findByTestId('workspace-switcher');
    }
    const notesBefore = vi.mocked(api.myNotes).mock.calls.length;
    vi.useFakeTimers({ toFake: ['Date', 'setTimeout', 'clearTimeout'] });
    vi.setSystemTime(0);
    const starts: number[] = [];
    vi.mocked(api.me)
      .mockClear()
      .mockImplementation(() => {
        starts.push(Date.now());
        return Promise.resolve(identity);
      });
    churning = true;
    delayed = true;
    if (path === 'restore')
      window.dispatchEvent(new PageTransitionEvent('pageshow', { persisted: true }));
    else render(App);
    await vi.advanceTimersByTimeAsync(100);
    expect(starts).toHaveLength(SESSION_CHECK_MAX_ATTEMPTS);
    expect(screen.queryByTestId('workspace-switcher')).toBeNull();
    expect(screen.queryByTestId('task-board')).toBeNull();
    expect(api.myNotes).toHaveBeenCalledTimes(notesBefore);
    expect(taskScreenApi.board).not.toHaveBeenCalled();
    expect(taskData.syncRecurrences).not.toHaveBeenCalled();
    if (path === 'restore') {
      expect(accountSafety.restoring).toBe(true);
      expect(accountSafety.switching).toBe(true);
      const fetches = vi.mocked(fetch).mock.calls.length;
      await expect(request('GET', '/api/workspaces')).rejects.toMatchObject({
        code: 'account_changed',
      });
      await expect(request('POST', '/api/notes', {})).rejects.toMatchObject({
        code: 'account_changed',
      });
      expect(fetch).toHaveBeenCalledTimes(fetches);
    }
    await vi.advanceTimersByTimeAsync(154);
    expect(starts).toEqual([0, 1, 2, 253]);
    await vi.advanceTimersByTimeAsync(9_746);
    expect(starts).toHaveLength(10);
    expect(starts.slice(3, 7)).toEqual([253, 754, 1755, 3756]);
    expect(screen.queryByTestId('workspace-switcher')).toBeNull();
    expect(api.myNotes).toHaveBeenCalledTimes(notesBefore);
    expect(taskScreenApi.board).not.toHaveBeenCalled();
    expect(screen.queryByRole('alert')).toBeNull();
    churning = false;
    await vi.advanceTimersByTimeAsync(2_001);
    await tick();
    expect(screen.getByTestId('workspace-switcher')).toBeVisible();
    expect(screen.queryByText('E private B space')).toBeNull();
    expect(screen.queryByRole('alert')).toBeNull();
    if (path === 'tasks') expect(screen.getByTestId('task-board')).toBeVisible();
    else expect(api.myNotes).toHaveBeenCalledTimes(notesBefore + 1);
  },
);

it.each(
  (['notes', 'restore'] as const).flatMap((path) =>
    (['network', 'safety', 'unmount'] as const).map((outcome) => [path, outcome] as const),
  ),
)(
  '#299 E preserves the separate %s %s outcome after paced notifications',
  async (path, outcome) => {
    router.navigate('/notes');
    vi.spyOn(api, 'workspaces').mockResolvedValue([WORKSPACE]);
    vi.spyOn(api, 'myOrganizations').mockResolvedValue([
      { ...TEST_ORGANIZATION, id: WORKSPACE.organization_id },
    ]);
    let view!: ReturnType<typeof render>;
    if (path === 'restore') {
      view = render(App);
      await screen.findByTestId('workspace-switcher');
    }
    const notesBefore = vi.mocked(api.myNotes).mock.calls.length;
    vi.useFakeTimers({ toFake: ['Date', 'setTimeout', 'clearTimeout'] });
    vi.setSystemTime(0);
    vi.mocked(api.me).mockClear();
    vi.mocked(api.workspaces).mockImplementation(
      () =>
        new Promise<Workspace[]>((resolve) => {
          setTimeout(() => {
            localStorage.setItem(
              'fvoci:auth-session',
              JSON.stringify({ state: 'settled', generation: readSessionGeneration() + 1 }),
            );
            resolve([WORKSPACE]);
          }, 1);
        }),
    );
    if (path === 'restore')
      window.dispatchEvent(new PageTransitionEvent('pageshow', { persisted: true }));
    else view = render(App);
    await vi.advanceTimersByTimeAsync(254);
    expect(api.me).toHaveBeenCalledTimes(4);
    if (outcome === 'unmount') view.unmount();
    else if (outcome === 'network') {
      connection.lost = true;
      vi.mocked(api.me).mockRejectedValue(new ApiError(503, 'unreachable', 'network unavailable'));
    } else
      vi.mocked(api.me).mockImplementation(() => {
        accountSafety.generation += 1;
        return Promise.resolve(identity);
      });
    await vi.advanceTimersByTimeAsync(10_000);
    expect(api.me).toHaveBeenCalledTimes(
      outcome === 'unmount' ? 4 : outcome === 'network' ? 5 : 4 + SESSION_CHECK_MAX_ATTEMPTS,
    );
    expect(api.myNotes).toHaveBeenCalledTimes(notesBefore);
    expect(screen.queryByTestId('workspace-switcher')).toBeNull();
    if (outcome !== 'unmount') {
      if (path === 'restore') expect(accountSafety.error).not.toBe('');
      else expect(screen.getByRole('alert')).toBeVisible();
    }
  },
);

it.each(viewerCases)(
  'SR-298-5 rechecks %s when %s belongs to B without a generation change',
  async (path, foreignList) => {
    router.navigate(
      path === 'tasks' ? itemViewPath(WORKSPACE.id, TASK_ITEM.id, 'board') : '/notes',
    );
    const generation = readSessionGeneration() + 1;
    localStorage.setItem(
      'fvoci:auth-session',
      JSON.stringify({
        state: 'logout-pending',
        generation,
        startedAt: Date.now(),
        attemptId: 'late',
      }),
    );
    Object.defineProperty(navigator, 'locks', {
      configurable: true,
      value: { request: () => new Promise<never>(() => undefined) },
    });
    let foreignCookie = false;
    let checks = 0;
    vi.mocked(api.me).mockImplementation(() => {
      checks += 1;
      // The /me response was captured as A before B's cookie supplied a later list.
      foreignCookie = checks === (path === 'restore' ? 2 : 1);
      return Promise.resolve(identity);
    });
    vi.mocked(fetch).mockImplementation((input) => {
      const url = typeof input === 'string' ? input : input instanceof URL ? input.href : input.url;
      const isWorkspace = url === '/api/workspaces';
      if (!isWorkspace && url !== '/api/me/organizations')
        return Promise.resolve(new Response('{}', { status: 404 }));
      const foreign =
        foreignCookie && foreignList === (isWorkspace ? 'workspaces' : 'organizations');
      const items = isWorkspace
        ? [{ ...WORKSPACE, ...(foreign ? { id: 'foreign-space', name: 'SR-298-5 B space' } : {}) }]
        : [
            {
              ...TEST_ORGANIZATION,
              id: WORKSPACE.organization_id,
              ...(foreign ? { id: 'foreign-organization', name: 'SR-298-5 B organization' } : {}),
            },
          ];
      return Promise.resolve(
        new Response(JSON.stringify(items), {
          status: 200,
          headers: { 'x-fvoci-viewer-id': foreign ? 'other-viewer' : identity.id },
        }),
      );
    });
    vi.spyOn(taskScreenApi, 'context').mockImplementation(() =>
      foreignCookie
        ? Promise.reject(new ApiError(404, 'not_found', 'not found'))
        : Promise.resolve(context),
    );
    render(App);
    if (path === 'restore') {
      await screen.findByTestId('workspace-switcher');
      window.dispatchEvent(new PageTransitionEvent('pageshow', { persisted: true }));
    }
    await waitFor(() => {
      expect(screen.getByTestId('workspace-switcher')).toBeVisible();
    });
    await fireEvent.click(screen.getByTestId('workspace-switcher'));
    expect(screen.queryByText('SR-298-5 B space')).toBeNull();
    expect(screen.queryByText('SR-298-5 B organization')).toBeNull();
    expect(screen.getByRole('menuitem', { name: WORKSPACE.name })).toBeInTheDocument();
    expect(vi.mocked(api.me).mock.calls.length).toBeGreaterThanOrEqual(path === 'restore' ? 3 : 2);
    expect(readSessionGeneration()).toBe(generation);
  },
);

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
  vi.useFakeTimers({ toFake: ['Date', 'setTimeout', 'clearTimeout'] });
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
  await vi.advanceTimersByTimeAsync(2_000);
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
  vi.useFakeTimers({ toFake: ['Date', 'setTimeout', 'clearTimeout'] });
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
  await vi.advanceTimersByTimeAsync(2_000);
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

it.each(['missing-locks', 'logout-pending', 'never-granted', 'churn', 'granted'] as const)(
  'SR-298-1 follow-up accepts its own verified task startup with %s',
  async (mode) => {
    vi.spyOn(api, 'workspaces').mockResolvedValue([WORKSPACE]);
    vi.spyOn(api, 'myOrganizations').mockResolvedValue([
      { ...TEST_ORGANIZATION, id: WORKSPACE.organization_id },
    ]);
    const read = vi.spyOn(taskScreenApi, 'context').mockResolvedValue(context);
    if (mode === 'missing-locks') {
      Reflect.deleteProperty(navigator, 'locks');
      expect('locks' in navigator).toBe(false);
    } else if (mode === 'logout-pending') {
      localStorage.setItem(
        'fvoci:auth-session',
        JSON.stringify({
          state: 'logout-pending',
          generation: readSessionGeneration() + 1,
          startedAt: Date.now(),
          attemptId: 'startup-pending',
        }),
      );
    } else if (mode === 'never-granted') {
      Object.defineProperty(navigator, 'locks', {
        configurable: true,
        value: { request: () => new Promise<never>(() => undefined) },
      });
    } else if (mode === 'churn') {
      vi.useFakeTimers({ toFake: ['Date', 'setTimeout', 'clearTimeout'] });
      vi.setSystemTime(0);
      let churning = true;
      const starts: number[] = [];
      vi.mocked(api.me).mockImplementation(() => {
        starts.push(Date.now());
        return Promise.resolve(identity);
      });
      read.mockImplementation(
        () =>
          new Promise<TaskContext>((resolve) => {
            setTimeout(() => {
              if (churning)
                localStorage.setItem(
                  'fvoci:auth-session',
                  JSON.stringify({
                    state: 'settled',
                    generation: readSessionGeneration() + 1,
                  }),
                );
              resolve(context);
            }, 1);
          }),
      );
      render(App);
      await vi.advanceTimersByTimeAsync(100);
      expect(api.me).toHaveBeenCalledTimes(SESSION_CHECK_MAX_ATTEMPTS);
      expect(screen.queryByText(context.item.title)).toBeNull();
      expect(screen.queryByTestId('task-board')).toBeNull();
      expect(taskData.syncRecurrences).not.toHaveBeenCalled();
      expect(taskScreenApi.board).not.toHaveBeenCalled();
      await vi.advanceTimersByTimeAsync(154);
      expect(starts).toEqual([0, 1, 2, 253]);
      expect(screen.queryByTestId('task-board')).toBeNull();
      await vi.advanceTimersByTimeAsync(9_746);
      expect(api.me).toHaveBeenCalledTimes(10);
      expect(starts.slice(3, 7)).toEqual([253, 754, 1755, 3756]);
      expect(screen.queryByRole('alert')).toBeNull();
      expect(screen.queryByText(context.item.title)).toBeNull();
      expect(screen.queryByTestId('task-board')).toBeNull();
      churning = false;
      await vi.advanceTimersByTimeAsync(2_001);
      await tick();
      expect(screen.getByTestId('task-board')).toBeVisible();
      expect(taskData.syncRecurrences).toHaveBeenCalledOnce();
      expect(taskScreenApi.board).toHaveBeenCalledOnce();
      expect(read).toHaveBeenCalledTimes(vi.mocked(api.me).mock.calls.length);
      expect(screen.queryByRole('alert')).toBeNull();
      return;
    }
    render(App);
    await screen.findByTestId('task-board');
    expect(taskData.syncRecurrences).toHaveBeenCalledOnce();
    expect(taskScreenApi.board).toHaveBeenCalledOnce();
    expect(api.me).toHaveBeenCalledOnce();
    expect(read).toHaveBeenCalledOnce();
  },
);

it.each(['network', 'safety', 'unmount'] as const)(
  'SR-298-6 preserves the separate %s outcome after paced notifications',
  async (outcome) => {
    vi.useFakeTimers({ toFake: ['Date', 'setTimeout', 'clearTimeout'] });
    vi.setSystemTime(0);
    vi.spyOn(api, 'workspaces').mockResolvedValue([WORKSPACE]);
    vi.spyOn(api, 'myOrganizations').mockResolvedValue([
      { ...TEST_ORGANIZATION, id: WORKSPACE.organization_id },
    ]);
    vi.spyOn(taskScreenApi, 'context').mockImplementation(
      () =>
        new Promise<TaskContext>((resolve) => {
          setTimeout(() => {
            localStorage.setItem(
              'fvoci:auth-session',
              JSON.stringify({
                state: 'settled',
                generation: readSessionGeneration() + 1,
              }),
            );
            resolve(context);
          }, 1);
        }),
    );
    const view = render(App);
    await vi.advanceTimersByTimeAsync(254);
    expect(api.me).toHaveBeenCalledTimes(4);
    if (outcome === 'unmount') view.unmount();
    else if (outcome === 'network') {
      connection.lost = true;
      vi.mocked(api.me).mockRejectedValue(new ApiError(503, 'unreachable', 'network unavailable'));
    } else
      vi.mocked(api.me).mockImplementation(() => {
        accountSafety.generation += 1;
        return Promise.resolve(identity);
      });
    await vi.advanceTimersByTimeAsync(10_000);
    expect(api.me).toHaveBeenCalledTimes(
      outcome === 'unmount' ? 4 : outcome === 'network' ? 5 : 4 + SESSION_CHECK_MAX_ATTEMPTS,
    );
    expect(taskScreenApi.board).not.toHaveBeenCalled();
    expect(taskData.syncRecurrences).not.toHaveBeenCalled();
    expect(screen.queryByText(context.item.title)).toBeNull();
    if (outcome !== 'unmount') expect(screen.getByRole('alert')).toBeVisible();
  },
);

it('SR-298-1 retries /me before starting task reads when its response sees a newer session', async () => {
  const recheck = held<Me | undefined>();
  vi.mocked(api.me)
    .mockImplementationOnce(() => {
      queueMicrotask(() => {
        localStorage.setItem(
          'fvoci:auth-session',
          JSON.stringify({
            state: 'settled',
            generation: readSessionGeneration() + 1,
          }),
        );
        window.dispatchEvent(new StorageEvent('storage', { key: 'fvoci:auth-session' }));
      });
      return Promise.resolve(identity);
    })
    .mockReturnValueOnce(recheck.promise);
  vi.spyOn(api, 'workspaces').mockResolvedValue([WORKSPACE]);
  vi.spyOn(api, 'myOrganizations').mockResolvedValue([
    { ...TEST_ORGANIZATION, id: WORKSPACE.organization_id },
  ]);
  vi.spyOn(taskScreenApi, 'context').mockResolvedValue(context);
  render(App);
  await waitFor(() => {
    expect(api.me).toHaveBeenCalledTimes(2);
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

it('SR-298-1 bounds actual safety changes without starting task reads', async () => {
  vi.mocked(api.me).mockImplementation(() => {
    accountSafety.generation += 1;
    return Promise.resolve(identity);
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

it('SR-298-1 discards B lists and context after a hint until its own /me confirms A again', async () => {
  const spaces = held<Workspace[]>();
  const organizations = held<OrganizationSummary[]>();
  const early = held<TaskContext>();
  const authority = held<Me | undefined>();
  vi.mocked(api.me).mockResolvedValueOnce(identity).mockReturnValueOnce(authority.promise);
  vi.spyOn(api, 'workspaces').mockReturnValueOnce(spaces.promise).mockResolvedValue([WORKSPACE]);
  vi.spyOn(api, 'myOrganizations')
    .mockReturnValueOnce(organizations.promise)
    .mockResolvedValue([{ ...TEST_ORGANIZATION, id: WORKSPACE.organization_id }]);
  vi.spyOn(taskScreenApi, 'context').mockReturnValueOnce(early.promise).mockResolvedValue(context);
  render(App);
  await waitFor(() => {
    expect(taskScreenApi.context).toHaveBeenCalledOnce();
  });
  localStorage.setItem(
    'fvoci:auth-session',
    JSON.stringify({
      state: 'settled',
      generation: readSessionGeneration() + 1,
    }),
  );
  spaces.resolve([{ ...WORKSPACE, name: 'private B workspace' }]);
  organizations.resolve([{ ...TEST_ORGANIZATION, name: 'private B organization' }]);
  early.resolve({ ...context, viewer_id: 'B', item: { ...context.item, title: 'private B task' } });
  await waitFor(() => {
    expect(api.me).toHaveBeenCalledTimes(2);
  });
  for (const title of ['private B workspace', 'private B organization', 'private B task'])
    expect(screen.queryByText(title)).toBeNull();
  expect(taskData.syncRecurrences).not.toHaveBeenCalled();
  expect(taskScreenApi.board).not.toHaveBeenCalled();
  expect(api.itemProfile).not.toHaveBeenCalled();
  authority.resolve(identity);
  await screen.findByTestId('task-board');
  expect(api.workspaces).toHaveBeenCalledTimes(2);
  expect(taskScreenApi.context).toHaveBeenCalledTimes(2);
  expect(taskScreenApi.board).toHaveBeenCalledOnce();
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
