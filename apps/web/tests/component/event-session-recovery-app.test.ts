import { cleanup, render, screen, waitFor, within } from '@testing-library/svelte';
import { tick } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { Me } from '@fvoci/api-client';
import App from '../../src/app/App.svelte';
import { api } from '$lib/api/client';
import { activityApi } from '$lib/features/activity/api';
import { accountSafety, auth } from '$lib/features/feedback/feedback.svelte';
import { readSessionGeneration } from '$lib/features/auth/session-signal';
import { eventsApi } from '$lib/features/events/api';
import type { EventRead, EventView } from '$lib/features/events/model';
import { taskScreenApi } from '$lib/features/tasks/screen-api';
import { savedViewsApi } from '$lib/features/events/saved-views-api';
import { itemGroupPath, router } from '$lib/router.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';
import { PROJECT_MODULE, TASK_ITEM, WORKSPACE } from '../../stories/fixtures/synthetic';
import { TEST_ORGANIZATION } from './fixtures/organization';

// 일정 대화상자의 첫 읽기가 가는 동안 다른 탭이 기동해 세션 알림을 남기면(세션 세대가 바뀜) 그 읽기는 버려진다. 업무 화면의
// 규칙(#298 SR-298-4·SR-298-8)처럼 버린 읽기를 오류로 끝내지 않고, 같은 사용자임을 /me로 확인한 뒤 한 번 다시 읽는다.
// 다른 사용자면 다시 읽지 않고 계정 전환 흐름으로 간다. 실제 브라우저의 두 탭 일정 충돌 시험이 이 경합으로 간헐 실패했다.

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
const event: EventView = {
  id: '00000000-0000-4000-8000-0000000000e1',
  item_id: item.id,
  workspace_id: WORKSPACE.id,
  kind: 'meeting',
  title: '두 탭 회의',
  description: '',
  all_day: true,
  start_date: '2026-11-02',
  end_date: '2026-11-02',
  starts_at: null,
  ends_at: null,
  timezone: 'Asia/Seoul',
  recurrence: null,
  excluded_dates: [],
  version: 1,
  access: 'write',
};
const read: EventRead = { ...event, viewer_id: identity.id, archived: false };
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
  router.navigate(itemGroupPath(WORKSPACE.id, item.id, 'events'));
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
  vi.spyOn(taskScreenApi, 'context').mockResolvedValue({
    viewer_id: identity.id,
    item,
    module: PROJECT_MODULE,
    documents: [],
  });
  vi.spyOn(api, 'item').mockResolvedValue(item);
  vi.spyOn(api, 'module').mockResolvedValue(PROJECT_MODULE);
  vi.spyOn(api, 'itemDocuments').mockResolvedValue([]);
  vi.spyOn(eventsApi, 'itemEvents').mockResolvedValue([event]);
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

it.each(['same', 'other'] as const)(
  'rereads an event first read discarded by another tab only after /me confirms the %s user',
  async (user) => {
    const first = held<EventRead>();
    const authority = held<Me>();
    const reads = vi
      .spyOn(eventsApi, 'event')
      .mockReturnValueOnce(first.promise)
      .mockResolvedValue(read);
    render(App, { reload: vi.fn(), replace: vi.fn() });
    const open = await screen.findByRole('button', { name: new RegExp(event.title) });
    vi.mocked(api.me).mockReturnValue(authority.promise);
    const before = vi.mocked(api.me).mock.calls.length;
    open.click();
    await waitFor(() => {
      expect(reads).toHaveBeenCalledOnce();
    });
    // 다른 탭이 기동하며 세션 알림을 남긴 뒤 첫 읽기가 돌아온다.
    signal();
    first.resolve(read);
    await waitFor(() => {
      expect(vi.mocked(api.me).mock.calls.length).toBeGreaterThan(before);
    });
    const dialog = screen.getByRole('dialog');
    expect(within(dialog).queryByRole('alert')).toBeNull();
    authority.resolve(user === 'same' ? identity : { ...identity, id: 'other-viewer' });
    if (user === 'other') {
      await waitFor(() => {
        expect(accountSafety.switching).toBe(true);
      });
      expect(reads).toHaveBeenCalledOnce();
      return;
    }
    await waitFor(() => {
      expect(within(dialog).getByLabelText(m.events_title())).toHaveValue(event.title);
    });
    await settled();
    expect(reads).toHaveBeenCalledTimes(2);
    expect(within(dialog).queryByText(m.events_event_load_failed())).toBeNull();
  },
);
