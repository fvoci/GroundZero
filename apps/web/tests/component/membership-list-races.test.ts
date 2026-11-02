import type { Me } from '@fvoci/api-client';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import App from '../../src/app/App.svelte';
import { api } from '$lib/api/client';
import { activityApi } from '$lib/features/activity/api';
import { accountSafety, auth } from '$lib/features/feedback/feedback.svelte';
import { router } from '$lib/router.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';
import { WORKSPACE } from '../../stories/fixtures/synthetic';
import { TEST_ORGANIZATION } from './fixtures/organization';

vi.mock('$lib/features/workspaces/HomeScreen.svelte', async () => ({
  default: (await import('./fixtures/SessionListRefreshHost.svelte')).default,
}));

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

function response(list: List, viewer = identity.id): Response {
  const other = viewer !== identity.id;
  const items =
    list === 'workspaces'
      ? [{ ...WORKSPACE, name: other ? 'Foreign B space' : WORKSPACE.name }]
      : [
          {
            ...TEST_ORGANIZATION,
            id: WORKSPACE.organization_id,
            name: other ? 'Foreign B organization' : TEST_ORGANIZATION.name,
          },
        ];
  return new Response(JSON.stringify(items), { headers: { 'x-fvoci-viewer-id': viewer } });
}
function unavailable(): Response {
  return new Response(JSON.stringify({ error: 'unreachable', message: 'list read failure' }), {
    status: 503,
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
  router.navigate('/');
  accountSafety.switching = false;
  accountSafety.restoring = false;
  auth.expired = false;
  vi.stubGlobal('matchMedia', (query: string) => ({
    matches: false,
    media: query,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
  }));
  vi.spyOn(api, 'me').mockResolvedValue(identity);
  vi.spyOn(api, 'workspaces');
  vi.spyOn(api, 'myOrganizations');
  vi.spyOn(api, 'currentTimer').mockResolvedValue(null);
  vi.spyOn(activityApi, 'unreadCount').mockResolvedValue(0);
  vi.spyOn(globalThis, 'fetch').mockImplementation((input) => {
    const url = typeof input === 'string' ? input : input instanceof URL ? input.href : input.url;
    if (url === '/api/workspaces') return Promise.resolve(response('workspaces'));
    if (url === '/api/me/organizations') return Promise.resolve(response('organizations'));
    return Promise.resolve(new Response('{}', { status: 404 }));
  });
});
afterEach(() => {
  cleanup();
  if (originalLocks === undefined) Reflect.deleteProperty(navigator, 'locks');
  else Object.defineProperty(navigator, 'locks', originalLocks);
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  router.guard = undefined;
  localStorage.clear();
});

const races = (['workspaces', 'organizations'] as const).flatMap((list) =>
  (
    ['ordinary-first', 'mismatch-first', 'older-newer-pending', 'older-newer-error'] as const
  ).flatMap((race) => (['same', 'other'] as const).map((user) => [list, race, user] as const)),
);

it.each(races)(
  'SR-299-2/3 observes %s in %s and follows its own %s proof',
  async (list, race, user) => {
    const reload = vi.fn();
    const replace = vi.fn();
    render(App, { reload, replace });
    const frame = await screen.findByTestId('refresh-private-frame');
    const input = screen.getByLabelText('Retained input');
    await fireEvent.input(input, { target: { value: 'A retained text' } });
    const authority = held<Me | undefined>();
    vi.mocked(api.me).mockReturnValue(authority.promise);
    const oldMismatch = held<Response>();
    const counterpart = held<Response>();
    const newer = held<Response>();
    let phase: 'older' | 'newer' | 'recovery' = 'older';
    vi.mocked(fetch).mockImplementation((input) => {
      const url = typeof input === 'string' ? input : input instanceof URL ? input.href : input.url;
      const resource =
        url === '/api/workspaces'
          ? 'workspaces'
          : url === '/api/me/organizations'
            ? 'organizations'
            : undefined;
      if (resource === undefined) return Promise.resolve(new Response('{}', { status: 404 }));
      if (phase === 'recovery') return Promise.resolve(response(resource));
      if (phase === 'newer') return newer.promise.then((value) => value.clone());
      if (resource === list) return oldMismatch.promise;
      if (race === 'mismatch-first') return counterpart.promise;
      return Promise.resolve(race === 'ordinary-first' ? unavailable() : response(resource));
    });
    await fireEvent.click(screen.getByRole('button', { name: 'First membership refresh' }));
    await waitFor(() => {
      expect(api.workspaces).toHaveBeenCalledTimes(2);
    });
    if (race === 'ordinary-first') {
      await screen.findByText('Caller handled the ordinary failure');
      expect(frame).toBeVisible();
    } else if (race !== 'mismatch-first') {
      phase = 'newer';
      await fireEvent.click(screen.getByRole('button', { name: 'Second membership refresh' }));
      await waitFor(() => {
        expect(api.workspaces).toHaveBeenCalledTimes(3);
      });
      if (race === 'older-newer-error') {
        newer.resolve(unavailable());
        await screen.findByText('Caller handled the ordinary failure');
      }
    }
    oldMismatch.resolve(response(list, 'other-viewer'));
    await waitFor(() => {
      expect(frame).not.toBeVisible();
      expect(api.me).toHaveBeenCalledTimes(2);
    });
    expect(frame.isConnected).toBe(true);
    expect(input).toHaveValue('A retained text');
    expect(frame.closest('div[style*="display: none"]')).toHaveProperty('inert', true);
    if (race === 'mismatch-first') {
      const counterpartResult =
        list === 'workspaces'
          ? vi.mocked(api.myOrganizations).mock.results[1]
          : vi.mocked(api.workspaces).mock.results[1];
      if (counterpartResult?.type !== 'return') throw new Error('missing counterpart read');
      counterpart.resolve(unavailable());
      await expect(counterpartResult.value).rejects.toMatchObject({ status: 409 });
      await new Promise<void>((resolve) => {
        setTimeout(resolve, 0);
      });
      expect(screen.queryByText('Caller handled the ordinary failure')).toBeNull();
    }
    phase = 'recovery';
    authority.resolve(user === 'same' ? identity : { ...identity, id: 'other-viewer' });
    if (user === 'same') {
      await waitFor(() => {
        expect(frame).toBeVisible();
      });
      expect(screen.getByLabelText('Retained input')).toBe(input);
      expect(input).toHaveValue('A retained text');
      expect(reload).not.toHaveBeenCalled();
    } else {
      await waitFor(() => {
        expect(reload).toHaveBeenCalledOnce();
      });
      expect(frame).not.toBeVisible();
      expect(accountSafety.reason).toBe('account');
    }
    expect(replace).not.toHaveBeenCalled();
    if (race === 'older-newer-pending') {
      newer.resolve(unavailable());
      await screen.findByText('Caller handled the ordinary failure');
      expect(api.me).toHaveBeenCalledTimes(2);
      if (user === 'same') expect(frame).toBeVisible();
      else expect(frame).not.toBeVisible();
    }
    expect(screen.queryByText('Foreign B space')).toBeNull();
    expect(screen.queryByText('Foreign B organization')).toBeNull();
  },
);
