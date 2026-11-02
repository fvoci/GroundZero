import type { Me } from '@fvoci/api-client';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import App from '../../src/app/App.svelte';
import { api, ApiError } from '$lib/api/client';
import { activityApi } from '$lib/features/activity/api';
import { accountSafety, auth } from '$lib/features/feedback/feedback.svelte';
import { readSessionGeneration } from '$lib/features/auth/session-signal';
import { SESSION_CHECK_MAX_ATTEMPTS } from '$lib/features/auth/session-check';
import { membersPath, router } from '$lib/router.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';
import { request } from '$lib/shared/api';
import { WORKSPACE } from '../../stories/fixtures/synthetic';
import { TEST_ORGANIZATION } from './fixtures/organization';

function held<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

const identity: Me = {
  id: 'viewer',
  display_name: 'A viewer',
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
type List = 'workspaces' | 'organizations';
let originalLocks: PropertyDescriptor | undefined;
let mode: 'initial' | 'foreign' | 'verified';
let foreignList: List;
let foreign: ReturnType<typeof held<Response>>;
let verified: ReturnType<typeof held<Response>> | undefined;

function listResponse(list: List, viewer = identity.id): Response {
  const other = viewer !== identity.id;
  const items =
    list === 'workspaces'
      ? [
          {
            ...WORKSPACE,
            name: other
              ? 'B private space'
              : mode === 'verified'
                ? 'A refreshed space'
                : WORKSPACE.name,
          },
        ]
      : [
          {
            ...TEST_ORGANIZATION,
            id: WORKSPACE.organization_id,
            name: other ? 'B private organization' : TEST_ORGANIZATION.name,
          },
        ];
  return new Response(JSON.stringify(items), {
    status: 200,
    headers: { 'x-fvoci-viewer-id': viewer },
  });
}

beforeEach(async () => {
  localStorage.clear();
  originalLocks = Object.getOwnPropertyDescriptor(navigator, 'locks');
  Object.defineProperty(navigator, 'locks', {
    configurable: true,
    value: { request: () => new Promise<never>(() => undefined) },
  });
  await setLocale('ko', { reload: false });
  router.guard = undefined;
  router.navigate(membersPath(WORKSPACE.id, 'groups'));
  accountSafety.switching = false;
  accountSafety.restoring = false;
  auth.expired = false;
  mode = 'initial';
  foreign = held<Response>();
  verified = undefined;
  vi.stubGlobal('matchMedia', (query: string) => ({
    matches: false,
    media: query,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
  }));
  vi.spyOn(globalThis, 'fetch').mockImplementation((input) => {
    const url = typeof input === 'string' ? input : input instanceof URL ? input.href : input.url;
    const list =
      url === '/api/workspaces'
        ? 'workspaces'
        : url === '/api/me/organizations'
          ? 'organizations'
          : undefined;
    if (list === undefined) return Promise.resolve(new Response('{}', { status: 404 }));
    if (mode === 'foreign' && list === foreignList)
      return foreign.promise.then((response) => response.clone());
    if (mode === 'verified' && list === foreignList && verified !== undefined)
      return verified.promise.then((response) => response.clone());
    return Promise.resolve(listResponse(list));
  });
  vi.spyOn(api, 'me').mockResolvedValue(identity);
  vi.spyOn(api, 'workspaces');
  vi.spyOn(api, 'myOrganizations');
  vi.spyOn(api, 'currentTimer').mockResolvedValue(null);
  vi.spyOn(api, 'members').mockResolvedValue({
    can_manage_members: true,
    members: [],
    my_role: 'admin',
    i_am_owner: true,
    pending_owner: null,
  });
  vi.spyOn(api, 'groups').mockResolvedValue([]);
  vi.spyOn(api, 'invitations').mockResolvedValue([]);
  vi.spyOn(api, 'createGroup').mockResolvedValue({
    id: 'new-group',
    name: 'created group',
    members: [],
  });
  vi.spyOn(activityApi, 'unreadCount').mockResolvedValue(0);
});

it.each(['workspaces', 'organizations'] as const)(
  'SR-299-1 keeps the mask and queued focus until the verified %s response arrives',
  async (list) => {
    const { heading, input, authority } = await beginReload(list);
    await waitFor(() => {
      expect(api.me).toHaveBeenCalledTimes(2);
    });
    verified = held<Response>();
    mode = 'verified';
    authority.resolve(identity);
    await waitFor(() => {
      expect(api.workspaces).toHaveBeenCalledTimes(3);
    });
    expect(heading).not.toBeVisible();
    expect(input).toHaveValue('preserved next group');
    window.dispatchEvent(new Event('focus'));
    document.dispatchEvent(new Event('visibilitychange'));
    await expect(request('POST', '/api/notes', {})).rejects.toMatchObject({
      code: 'account_changed',
    });
    expect(api.me).toHaveBeenCalledTimes(2);
    verified.resolve(listResponse(list));
    await waitFor(() => {
      expect(heading).toBeVisible();
      expect(api.me).toHaveBeenCalledTimes(3);
    });
    expect(screen.getByLabelText(m.members_new_group_name())).toBe(input);
    expect(input).toHaveValue('preserved next group');
  },
);

it.each(['workspaces', 'organizations'] as const)(
  'SR-299-1 paces repeated %s mismatches and opens after they stop',
  async (list) => {
    const { heading, input, authority } = await beginReload(list);
    await waitFor(() => {
      expect(api.me).toHaveBeenCalledTimes(2);
    });
    vi.useFakeTimers({ toFake: ['Date', 'setTimeout', 'clearTimeout'] });
    vi.setSystemTime(0);
    const starts: number[] = [];
    vi.mocked(api.me).mockImplementation(() => {
      starts.push(Date.now());
      return Promise.resolve(identity);
    });
    authority.resolve(identity);
    await vi.advanceTimersByTimeAsync(100);
    expect(api.me).toHaveBeenCalledTimes(1 + SESSION_CHECK_MAX_ATTEMPTS);
    expect(heading).not.toBeVisible();
    const fetches = vi.mocked(fetch).mock.calls.length;
    await expect(request('GET', '/api/workspaces')).rejects.toMatchObject({
      code: 'account_changed',
    });
    expect(fetch).toHaveBeenCalledTimes(fetches);
    await vi.advanceTimersByTimeAsync(154);
    expect(starts).toEqual([0, 0, 250]);
    await vi.advanceTimersByTimeAsync(9_746);
    expect(api.me).toHaveBeenCalledTimes(11);
    expect(starts.slice(2, 6)).toEqual([250, 750, 1750, 3750]);
    expect(heading).not.toBeVisible();
    expect(input).toHaveValue('preserved next group');
    mode = 'verified';
    await vi.advanceTimersByTimeAsync(2_000);
    expect(heading).toBeVisible();
    expect(screen.getByLabelText(m.members_new_group_name())).toBe(input);
  },
);

it.each(['network', 'safety', 'unmount'] as const)(
  'SR-299-1 preserves the %s outcome after paced mismatches',
  async (outcome) => {
    const { heading, input, authority, view } = await beginReload('workspaces');
    await waitFor(() => {
      expect(api.me).toHaveBeenCalledTimes(2);
    });
    vi.useFakeTimers({ toFake: ['Date', 'setTimeout', 'clearTimeout'] });
    vi.setSystemTime(0);
    vi.mocked(api.me).mockResolvedValue(identity);
    authority.resolve(identity);
    await vi.advanceTimersByTimeAsync(100);
    expect(api.me).toHaveBeenCalledTimes(4);
    if (outcome === 'unmount') view.unmount();
    else if (outcome === 'network')
      vi.mocked(api.me).mockRejectedValue(new ApiError(503, 'unreachable', 'network unavailable'));
    else
      vi.mocked(api.me).mockImplementation(() => {
        accountSafety.generation += 1;
        return Promise.resolve(identity);
      });
    await vi.advanceTimersByTimeAsync(10_000);
    expect(api.me).toHaveBeenCalledTimes(
      outcome === 'unmount' ? 4 : outcome === 'network' ? 5 : 4 + SESSION_CHECK_MAX_ATTEMPTS,
    );
    expect(heading).not.toBeVisible();
    if (outcome !== 'unmount') {
      expect(input.isConnected).toBe(true);
      expect(input).toHaveValue('preserved next group');
      expect(accountSafety.error).not.toBe('');
      mode = 'verified';
      vi.mocked(api.me).mockResolvedValue(identity);
      await fireEvent.click(screen.getByRole('button', { name: m.common_retry() }));
      await vi.advanceTimersByTimeAsync(1);
      expect(heading).toBeVisible();
      expect(input).toHaveValue('preserved next group');
    }
  },
);

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  if (originalLocks === undefined) Reflect.deleteProperty(navigator, 'locks');
  else Object.defineProperty(navigator, 'locks', originalLocks);
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  router.guard = undefined;
  localStorage.clear();
});

async function beginReload(list: List) {
  const reload = vi.fn();
  const replace = vi.fn();
  const view = render(App, { reload, replace });
  const input = await screen.findByLabelText(m.members_new_group_name());
  const heading = screen.getByRole('heading', {
    level: 1,
    name: m.members_management_heading({ workspace: WORKSPACE.name }),
  });
  const authority = held<Me | undefined>();
  vi.mocked(api.me).mockReturnValue(authority.promise);
  foreignList = list;
  mode = 'foreign';
  await fireEvent.input(input, { target: { value: 'created group' } });
  const form = input.closest('form');
  if (form === null) throw new Error('missing group form');
  await fireEvent.submit(form);
  await waitFor(() => {
    expect(api.workspaces).toHaveBeenCalledTimes(2);
  });
  await fireEvent.input(input, { target: { value: 'preserved next group' } });
  foreign.resolve(listResponse(list, 'other-viewer'));
  return { view, input, heading, authority, reload, replace };
}

it.each(
  (['workspaces', 'organizations'] as const).flatMap((list) =>
    (['same', 'other', 'anonymous'] as const).map((user) => [list, user] as const),
  ),
)('SR-299-1 masks a foreign %s reload and follows its own %s response', async (list, user) => {
  const generation = readSessionGeneration();
  const { input, heading, authority, reload, replace } = await beginReload(list);
  await waitFor(() => {
    expect(heading).not.toBeVisible();
    expect(api.me).toHaveBeenCalledTimes(2);
  });
  expect(heading.isConnected).toBe(true);
  expect(input).toHaveValue('preserved next group');
  expect(heading.closest('div[style*="display: none"]')).toHaveProperty('inert', true);
  expect(accountSafety.reason).toBe('reauth');
  expect(readSessionGeneration()).toBe(generation);
  const fetches = vi.mocked(fetch).mock.calls.length;
  await expect(request('GET', '/api/workspaces')).rejects.toMatchObject({
    code: 'account_changed',
  });
  await expect(request('POST', '/api/notes', {})).rejects.toMatchObject({
    code: 'account_changed',
  });
  expect(fetch).toHaveBeenCalledTimes(fetches);
  mode = 'verified';
  authority.resolve(
    user === 'anonymous'
      ? undefined
      : user === 'same'
        ? identity
        : { ...identity, id: 'other-viewer' },
  );
  if (user === 'same') {
    await waitFor(() => {
      expect(heading).toBeVisible();
    });
    expect(screen.getByLabelText(m.members_new_group_name())).toBe(input);
    expect(input).toHaveValue('preserved next group');
    await fireEvent.click(screen.getByTestId('workspace-switcher'));
    expect(screen.getByRole('menuitem', { name: 'A refreshed space' })).toBeVisible();
    expect(api.workspaces).toHaveBeenCalledTimes(3);
    expect(reload).not.toHaveBeenCalled();
  } else {
    if (user === 'other') {
      await waitFor(() => {
        expect(reload).toHaveBeenCalledOnce();
      });
      expect(accountSafety.reason).toBe('account');
    } else await screen.findByRole('link', { name: m.common_session_sign_in() });
    expect(heading).not.toBeVisible();
    expect(input.isConnected).toBe(true);
    expect(api.workspaces).toHaveBeenCalledTimes(2);
  }
  expect(replace).not.toHaveBeenCalled();
  expect(screen.queryByText('B private space')).toBeNull();
  expect(screen.queryByText('B private organization')).toBeNull();
});
