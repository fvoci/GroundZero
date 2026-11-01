import { cleanup, render, screen, waitFor } from '@testing-library/svelte';
import { tick } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { Me, OrganizationSummary, Workspace } from '@fvoci/api-client';
import App from '../../src/app/App.svelte';
import { api } from '$lib/api/client';
import { activityApi } from '$lib/features/activity/api';
import { taskData } from '$lib/features/tasks/detail-data';
import { taskScreenApi, type TaskContext } from '$lib/features/tasks/screen-api';
import { accountSafety, auth } from '$lib/features/feedback/feedback.svelte';
import { itemViewPath, router } from '$lib/router.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';
import { PROJECT_MODULE, TASK_ITEM, WORKSPACE } from '../../stories/fixtures/synthetic';
import { TEST_ORGANIZATION } from './fixtures/organization';

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
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  router.guard = undefined;
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
