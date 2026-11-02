import './fixtures/authenticated';
import './fixtures/task-statuses';
import { render, screen, waitFor } from '@testing-library/svelte';
import { tick } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { Module } from '@fvoci/api-client';
import { api, ApiError, type BacklinkView, type ItemView } from '$lib/api/client';
import { accountSafety } from '$lib/features/feedback/feedback.svelte';
import ItemScreen from '$lib/features/modules/ItemScreen.svelte';
import { me } from '$lib/me.svelte';
import { org } from '$lib/org.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';
import { itemGroupPath, itemPath, router } from '$lib/router.svelte';
import { writeSession } from '$lib/storage';
import { PROJECT_MODULE, TASK_ITEM, WORKSPACE } from '../../stories/fixtures/synthetic';
import { TEST_ORGANIZATION } from './fixtures/organization';
import { openItemGroup, resetRoute } from './fixtures/item-groups';

function held<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((done, fail) => {
    resolve = done;
    reject = fail;
  });
  return { promise, resolve, reject };
}

const otherId = 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa';
const item = (id = TASK_ITEM.id): ItemView => ({ ...TASK_ITEM, id, title: `item ${id}` });
const links = (title: string): BacklinkView[] => [
  {
    id: 'bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb',
    title,
    reference: 'DEV-99',
    workspace_id: WORKSPACE.id,
    item_id: TASK_ITEM.id,
  },
];
const props = (itemId = TASK_ITEM.id) => ({
  workspace: WORKSPACE,
  itemId,
  workspaces: [WORKSPACE],
});
const docsPath = (id = TASK_ITEM.id) => itemGroupPath(WORKSPACE.id, id, 'docs');

async function settle(): Promise<void> {
  await tick();
  await Promise.resolve();
  await tick();
}

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  sessionStorage.clear();
  router.guard = undefined;
  accountSafety.restoring = false;
  vi.stubGlobal('fetch', () => new Promise<Response>(() => undefined));
  vi.spyOn(api, 'item').mockImplementation((id) => Promise.resolve(item(id)));
  vi.spyOn(api, 'module').mockResolvedValue(PROJECT_MODULE);
  vi.spyOn(api, 'itemDocuments').mockResolvedValue([]);
  vi.spyOn(api, 'itemBacklinks').mockResolvedValue([]);
});
afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  sessionStorage.clear();
  router.guard = undefined;
  accountSafety.restoring = false;
  resetRoute();
});

it('starts item backlinks after item success alongside the pending module, then consumes once', async () => {
  const resource = held<ItemView>();
  const module = held<Module>();
  vi.mocked(api.item).mockReturnValue(resource.promise);
  vi.mocked(api.module).mockReturnValue(module.promise);
  vi.mocked(api.itemBacklinks).mockResolvedValue(links('parallel backlink'));
  router.replace(docsPath());
  render(ItemScreen, { props: props() });
  await waitFor(() => {
    expect(api.item).toHaveBeenCalledOnce();
  });
  expect(api.itemBacklinks).not.toHaveBeenCalled();
  resource.resolve(item());
  await waitFor(() => {
    expect(api.module).toHaveBeenCalledOnce();
  });
  await waitFor(() => {
    expect(api.itemBacklinks).toHaveBeenCalledExactlyOnceWith(TASK_ITEM.id);
  });
  expect(screen.queryByText('parallel backlink')).toBeNull();
  module.resolve(PROJECT_MODULE);
  await screen.findByRole('link', { name: /parallel backlink/ });
  expect(api.itemBacklinks).toHaveBeenCalledOnce();
});

it('renders the document group while the parallel backlinks request is still pending', async () => {
  const read = held<BacklinkView[]>();
  vi.mocked(api.itemBacklinks).mockReturnValue(read.promise);
  router.replace(docsPath());
  render(ItemScreen, { props: props() });
  await screen.findByRole('heading', { name: /^문서$/ });
  read.resolve(links('late backlink'));
  await screen.findByRole('link', { name: /late backlink/ });
  expect(api.itemBacklinks).toHaveBeenCalledOnce();
});

it('keeps an early backlinks failure for the validated document group without a duplicate retry', async () => {
  const module = held<Module>();
  const read = held<BacklinkView[]>();
  vi.mocked(api.module).mockReturnValue(module.promise);
  vi.mocked(api.itemBacklinks).mockReturnValue(read.promise);
  router.replace(docsPath());
  render(ItemScreen, { props: props() });
  await waitFor(() => {
    expect(api.itemBacklinks).toHaveBeenCalledOnce();
  });
  read.reject(new Error('offline'));
  await settle();
  expect(screen.queryByText(m.references_backlinks_failed())).toBeNull();
  module.resolve(PROJECT_MODULE);
  await screen.findByText(m.references_backlinks_failed());
  expect(api.itemBacklinks).toHaveBeenCalledOnce();
});

it('does not expose successful backlinks when the module request fails', async () => {
  const module = held<Module>();
  vi.mocked(api.module).mockReturnValue(module.promise);
  vi.mocked(api.itemBacklinks).mockResolvedValue(links('unvalidated backlink'));
  router.replace(docsPath());
  render(ItemScreen, { props: props() });
  await waitFor(() => {
    expect(api.itemBacklinks).toHaveBeenCalledOnce();
  });
  module.reject(new ApiError(404, 'not_found', 'not found'));
  await settle();
  expect(screen.queryByText('unvalidated backlink')).toBeNull();
  expect(document.getElementById('backlinks-heading')).toBeNull();
});

it('does not start backlinks after an inaccessible item', async () => {
  vi.mocked(api.item).mockRejectedValue(new ApiError(404, 'not_found', 'not found'));
  router.replace(docsPath());
  render(ItemScreen, { props: props() });
  await settle();
  expect(api.module).not.toHaveBeenCalled();
  expect(api.itemBacklinks).not.toHaveBeenCalled();
});

it.each(['tasks', 'events', 'records'] as const)(
  'keeps backlinks lazy on the %s group',
  async (group) => {
    const module = held<Module>();
    vi.mocked(api.module).mockReturnValue(module.promise);
    router.replace(itemGroupPath(WORKSPACE.id, TASK_ITEM.id, group));
    render(ItemScreen, { props: props() });
    await waitFor(() => {
      expect(api.module).toHaveBeenCalledOnce();
    });
    expect(api.itemBacklinks).not.toHaveBeenCalled();
    module.resolve(PROJECT_MODULE);
    await screen.findByRole('navigation', { name: '항목 묶음' });
    expect(api.itemBacklinks).not.toHaveBeenCalled();
  },
);

it('reads remembered documents only after module validation on a root entry', async () => {
  const module = held<Module>();
  vi.mocked(api.module).mockReturnValue(module.promise);
  writeSession(
    org.key(`item-group:${TASK_ITEM.id}`),
    JSON.stringify({ group: 'docs', view: 'board' }),
  );
  router.replace(itemPath(WORKSPACE.id, TASK_ITEM.id));
  render(ItemScreen, { props: props() });
  await waitFor(() => {
    expect(api.module).toHaveBeenCalledOnce();
  });
  expect(api.itemBacklinks).not.toHaveBeenCalled();
  module.resolve(PROJECT_MODULE);
  await waitFor(() => {
    expect(api.itemBacklinks).toHaveBeenCalledOnce();
  });
});

it('reads again after leaving and reopening the documents group', async () => {
  vi.mocked(api.itemBacklinks)
    .mockResolvedValueOnce(links('old backlink'))
    .mockResolvedValueOnce(links('fresh backlink'));
  router.replace(docsPath());
  render(ItemScreen, { props: props() });
  await screen.findByRole('link', { name: /old backlink/ });
  await openItemGroup('일정');
  await openItemGroup('문서');
  await screen.findByRole('link', { name: /fresh backlink/ });
  expect(screen.queryByText('old backlink')).toBeNull();
  expect(api.itemBacklinks).toHaveBeenCalledTimes(2);
});

it('drops the first A response after A to B to A navigation', async () => {
  const first = held<BacklinkView[]>();
  vi.mocked(api.itemBacklinks)
    .mockReturnValueOnce(first.promise)
    .mockResolvedValueOnce([])
    .mockResolvedValueOnce(links('new A backlink'));
  router.replace(docsPath());
  const view = render(ItemScreen, { props: props() });
  await waitFor(() => {
    expect(api.itemBacklinks).toHaveBeenCalledTimes(1);
  });
  router.navigate(docsPath(otherId));
  await view.rerender(props(otherId));
  await waitFor(() => {
    expect(api.itemBacklinks).toHaveBeenCalledTimes(2);
  });
  router.navigate(docsPath());
  await view.rerender(props());
  await screen.findByRole('link', { name: /new A backlink/ });
  first.resolve(links('old A backlink'));
  await settle();
  expect(screen.queryByText('old A backlink')).toBeNull();
  expect(api.itemBacklinks).toHaveBeenCalledTimes(3);
});

it.each(['account', 'organization', 'session'] as const)(
  'drops late backlinks after a %s context change',
  async (change) => {
    const read = held<BacklinkView[]>();
    vi.mocked(api.itemBacklinks).mockReturnValue(read.promise);
    router.replace(docsPath());
    render(ItemScreen, { props: props() });
    await waitFor(() => {
      expect(api.itemBacklinks).toHaveBeenCalledOnce();
    });
    if (change === 'account') me.userId = 'user-b';
    else if (change === 'session') accountSafety.generation += 1;
    else org.load('user-a', [{ ...TEST_ORGANIZATION, id: 'org-2' }], 'org-2');
    read.resolve(links('previous context backlink'));
    await settle();
    expect(screen.queryByText('previous context backlink')).toBeNull();
    expect(api.itemBacklinks).toHaveBeenCalledOnce();
  },
);

it.each(['account', 'organization', 'session'] as const)(
  'does not adopt a prefetched document group after a %s change during module validation',
  async (change) => {
    const module = held<Module>();
    vi.mocked(api.module).mockReturnValue(module.promise);
    vi.mocked(api.itemBacklinks).mockResolvedValue(links('previous entry backlink'));
    router.replace(docsPath());
    render(ItemScreen, { props: props() });
    await waitFor(() => {
      expect(api.itemBacklinks).toHaveBeenCalledOnce();
    });
    if (change === 'account') me.userId = 'user-b';
    else if (change === 'session') accountSafety.generation += 1;
    else org.load('user-a', [{ ...TEST_ORGANIZATION, id: 'org-2' }], 'org-2');
    module.resolve(PROJECT_MODULE);
    await settle();
    expect(screen.queryByText('previous entry backlink')).toBeNull();
    expect(document.getElementById('backlinks-heading')).toBeNull();
  },
);

it('does not start the old viewer request after an account change during item validation', async () => {
  const read = held<ItemView>();
  vi.mocked(api.item).mockReturnValue(read.promise);
  router.replace(docsPath());
  render(ItemScreen, { props: props() });
  await waitFor(() => {
    expect(api.item).toHaveBeenCalledOnce();
  });
  me.userId = 'user-b';
  read.resolve(item());
  await settle();
  expect(api.itemBacklinks).not.toHaveBeenCalled();
});

it('does not start a stale item request after unmount', async () => {
  const read = held<ItemView>();
  vi.mocked(api.item).mockReturnValue(read.promise);
  router.replace(docsPath());
  const view = render(ItemScreen, { props: props() });
  await waitFor(() => {
    expect(api.item).toHaveBeenCalledOnce();
  });
  view.unmount();
  read.resolve(item());
  await settle();
  expect(api.itemBacklinks).not.toHaveBeenCalled();
});
