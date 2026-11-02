import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { tick } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { Me } from '@fvoci/api-client';
import App from '../../src/app/App.svelte';
import { api, ApiError } from '$lib/api/client';
import { activityApi } from '$lib/features/activity/api';
import { accountSafety, auth } from '$lib/features/feedback/feedback.svelte';
import { readSessionGeneration } from '$lib/features/auth/session-signal';
import type { TaskContext } from '$lib/features/tasks/screen-api';
import { savedViewsApi } from '$lib/features/events/saved-views-api';
import { itemGroupPath, router, workspacePath } from '$lib/router.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';
import { PROJECT_MODULE, TASK_ITEM, WORKSPACE } from '../../stories/fixtures/synthetic';
import { TEST_ORGANIZATION } from './fixtures/organization';

// 항목 화면(정보·프로필 폼이 있는 곳)을 앱 안에서 업무 묶음으로 열 때의 첫 읽기(task-context)가 가는 동안 다른 탭이
// 세션 알림을 남기면 그 읽기는 버려진다. 일정 대화상자(#305 E·F)·업무 화면(#298 SR-298-4·8, CR-4)처럼 같은 사용자임을
// /me로 확인한 뒤 한 번 다시 읽고, 다른 사용자면 다시 읽지 않고 계정 전환으로 간다. 확인이 실패해도 원래 요청이 아직
// 현재면 오류와 다시 시도를 보인다(빈 화면이나 다시 시도 없는 오류로 끝나지 않음).

function held<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((ok) => {
    resolve = ok;
  });
  return { promise, resolve };
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
const item = { ...TASK_ITEM, access: 'write' as const };
const context: TaskContext = {
  viewer_id: identity.id,
  item,
  module: PROJECT_MODULE,
  documents: [],
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
  can_edit: true,
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
  router.navigate(workspacePath(WORKSPACE.id));
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
  vi.spyOn(savedViewsApi, 'savedViews').mockResolvedValue([]);
  vi.spyOn(activityApi, 'unreadCount').mockResolvedValue(0);
  vi.spyOn(activityApi, 'mark').mockResolvedValue({ pin: false, favorite: false });
  vi.spyOn(api, 'item').mockResolvedValue(item);
  vi.spyOn(api, 'module').mockResolvedValue(PROJECT_MODULE);
  vi.spyOn(api, 'itemDocuments').mockResolvedValue([]);
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

const heading = () => document.getElementById('item-heading');
const CONTEXT_PATH = `/api/items/${item.id}/task-context`;
const contextResponse = () =>
  new Response(JSON.stringify(context), {
    status: 200,
    headers: { 'Content-Type': 'application/json' },
  });

/** 실제 task-context 클라이언트(세션 세대 울타리 포함)를 쓰도록 fetch 수준에서 첫 응답만 붙잡는다. */
function serveContext(first: Promise<Response>): { count: () => number } {
  let count = 0;
  vi.mocked(globalThis.fetch).mockImplementation((input: unknown) => {
    if (input === CONTEXT_PATH) {
      count += 1;
      return count === 1 ? first : Promise.resolve(contextResponse());
    }
    return Promise.resolve(new Response('{}', { status: 404 }));
  });
  return { count: () => count };
}

it.each(['same', 'other'] as const)(
  'rereads the item context discarded by another tab only after /me confirms the %s user',
  async (user) => {
    const first = held<Response>();
    const authority = held<Me>();
    const reads = serveContext(first.promise);
    render(App, { reload: vi.fn(), replace: vi.fn() });
    await waitFor(() => {
      expect(vi.mocked(api.workspaces)).toHaveBeenCalled();
    });
    await settled();
    vi.mocked(api.me).mockReturnValue(authority.promise);
    const before = vi.mocked(api.me).mock.calls.length;
    router.navigate(itemGroupPath(WORKSPACE.id, item.id, 'tasks'));
    await waitFor(() => {
      expect(reads.count()).toBe(1);
    });
    // 다른 탭이 기동하며 세션 알림을 남긴 뒤 첫 읽기가 돌아온다(그 읽기는 버려진다).
    signal();
    first.resolve(contextResponse());
    await waitFor(() => {
      expect(vi.mocked(api.me).mock.calls.length).toBeGreaterThan(before);
    });
    expect(screen.queryByText(m.modules_item_load_failed())).toBeNull();
    authority.resolve(user === 'same' ? identity : { ...identity, id: 'other-viewer' });
    if (user === 'other') {
      await waitFor(() => {
        expect(accountSafety.switching).toBe(true);
      });
      expect(reads.count()).toBe(1);
      return;
    }
    await waitFor(() => {
      expect(heading()).toHaveTextContent(item.title);
    });
    await settled();
    expect(reads.count()).toBe(2);
    expect(screen.queryByText(m.modules_item_load_failed())).toBeNull();
  },
);

it.each(['same', 'other'] as const)(
  'shows the item retry after a failed /me check is cleared by the %s user',
  async (user) => {
    const first = held<Response>();
    const reads = serveContext(first.promise);
    const reload = vi.fn();
    render(App, { reload, replace: vi.fn() });
    await waitFor(() => {
      expect(vi.mocked(api.workspaces)).toHaveBeenCalled();
    });
    await settled();
    vi.mocked(api.me).mockRejectedValue(
      new ApiError(503, 'unreachable', 'verification unavailable'),
    );
    router.navigate(itemGroupPath(WORKSPACE.id, item.id, 'tasks'));
    await waitFor(() => {
      expect(reads.count()).toBe(1);
    });
    signal();
    first.resolve(contextResponse());
    await waitFor(() => {
      expect(accountSafety.error).not.toBe('');
    });
    vi.mocked(api.me).mockResolvedValue(
      user === 'same' ? identity : { ...identity, id: 'other-viewer' },
    );
    const mask = screen.getByTestId('account-safe-state');
    await fireEvent.click(within(mask).getByRole('button', { name: m.common_retry() }));
    if (user === 'other') {
      await waitFor(() => {
        expect(reload).toHaveBeenCalledOnce();
      });
      expect(accountSafety.reason).toBe('account');
      await settled();
      expect(reads.count()).toBe(1);
      return;
    }
    await waitFor(() => {
      expect(accountSafety.switching).toBe(false);
    });
    await settled();
    expect(screen.getByText(m.modules_item_load_failed())).toBeInTheDocument();
    expect(reads.count()).toBe(1);
    await fireEvent.click(screen.getByRole('button', { name: m.common_retry() }));
    await waitFor(() => {
      expect(heading()).toHaveTextContent(item.title);
    });
    expect(reads.count()).toBe(2);
    expect(screen.queryByText(m.modules_item_load_failed())).toBeNull();
  },
);
