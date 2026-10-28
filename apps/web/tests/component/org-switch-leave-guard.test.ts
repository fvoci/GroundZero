import { timeData } from '$lib/features/time/data';
import { activityApi } from '$lib/features/activity/api';
import type { Me, Workspace } from '@fvoci/api-client';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { tick } from 'svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { api } from '$lib/api/client';
import { createLeaveGuard } from '$lib/features/common/leave-guard.svelte';
import { org } from '$lib/org.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';
import { router } from '$lib/router.svelte';
import { readLocal, scopedKey } from '$lib/storage';

import { TEST_ORGANIZATION } from './fixtures/organization';

import App from '../../src/app/App.svelte';

// 조직 전환(사이드바)은 다른 조직의 워크스페이스를 보고 있으면 홈으로 이동한다. 그 이동은 저장하지 않은 편집의 이탈 확인을
// 거치며, 취소하면 조직 선택(메모리·저장소)·워크스페이스·화면이 그대로여야 한다(CodeRabbit outside-diff 1e6de63c). 연속
// 전환과 늦게 온 이전 조직의 응답도 마지막 선택을 덮지 않는다.

const OTHER = { ...TEST_ORGANIZATION, id: 'org-2', name: '다른 조직' };

const me: Me = {
  id: 'user-a',
  display_name: '나',
  hour_cycle: '',
  locale: 'ko',
  time_zone: '',
  week_start: '',
  theme: 'system',
  last_organization_id: TEST_ORGANIZATION.id,
  limits: {
    max_title_chars: 200,
    max_document_bytes: 2_000_000,
    max_references_per_document: 500,
    max_document_depth: 10,
  },
};

function workspace(id: string, organizationId: string, name: string): Workspace {
  return {
    id,
    organization_id: organizationId,
    name,
    role: 'admin',
    is_owner: true,
    realtime_enabled: false,
    access: 'owner',
  };
}

// 워크스페이스 경로(`/w/:id`)는 UUID만 받는다.
const WS1 = '11111111-1111-4111-8111-111111111111';
const WS2 = '22222222-2222-4222-8222-222222222222';
const workspaces = [workspace(WS1, 'org-1', '공간 하나'), workspace(WS2, 'org-2', '공간 둘')];

/** 이 브라우저에 저장된 조직 선택(사용자별 키). */
const stored = (): string | null => readLocal(scopedKey('organization', 'user-a', undefined));

async function chooseOrganization(name: RegExp): Promise<void> {
  // 메뉴 단추는 click으로 열린다(task-move-menu 시험과 같은 방식).
  await fireEvent.click(await screen.findByTestId('organization-switcher'));
  await fireEvent.click(await screen.findByRole('menuitem', { name }));
}

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  window.localStorage.clear();
  vi.stubGlobal('matchMedia', (query: string) => ({
    matches: false,
    media: query,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
  }));
  Element.prototype.hasPointerCapture = (): boolean => false;
  Element.prototype.setPointerCapture = (): void => undefined;
  Element.prototype.releasePointerCapture = (): void => undefined;
  Element.prototype.scrollIntoView = (): void => undefined;
  // 따로 흉내 내지 않은 요청은 없다고 답한다(홈·워크스페이스 화면이 스스로 읽는 것).
  vi.spyOn(globalThis, 'fetch').mockImplementation(() =>
    Promise.resolve(
      new Response(JSON.stringify({ error: 'not_found', message: 'not found' }), {
        status: 404,
        headers: { 'Content-Type': 'application/json' },
      }),
    ),
  );
  vi.spyOn(api, 'me').mockResolvedValue(me);
  vi.spyOn(api, 'setDisplaySettings').mockResolvedValue(undefined);
  vi.spyOn(api, 'workspaces').mockResolvedValue(workspaces);
  vi.spyOn(api, 'myOrganizations').mockResolvedValue([TEST_ORGANIZATION, OTHER]);
  vi.spyOn(activityApi, 'unreadCount').mockResolvedValue(0);
  vi.spyOn(activityApi, 'notifications').mockResolvedValue({ notifications: [], next: null });
  vi.spyOn(timeData, 'currentTimer').mockResolvedValue(null);
  vi.spyOn(api, 'deletedWorkspaces').mockResolvedValue([]);
  vi.spyOn(api, 'wikiDocuments').mockResolvedValue([]);
  vi.spyOn(api, 'modules').mockResolvedValue([]);
  vi.spyOn(api, 'documentTemplates').mockResolvedValue([]);
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  router.guard = undefined;
  router.navigate('/');
  window.localStorage.clear();
});

describe('organization switch and the leave confirmation', () => {
  it('keeps the organization, the stored choice, the screen and the guard when leaving is cancelled, and applies both when confirmed', async () => {
    router.navigate(`/w/${WS1}`);
    render(App);
    await screen.findByTestId('organization-switcher');
    // 워크스페이스로 들어가면 그 조직을 기억한다(앱 셸의 효과). 그 뒤의 전환 취소가 이 값을 바꾸지 않아야 한다.
    await waitFor(() => {
      expect(org.currentId).toBe('org-1');
      expect(stored()).toBe('org-1');
    });

    // 저장하지 않은 편집이 있는 화면처럼 공용 이탈 확인을 건다.
    const guard = createLeaveGuard({ dirty: () => true });
    const detach = guard.attach();

    await chooseOrganization(/^다른 조직/);
    expect(guard.open).toBe(true);
    expect(org.currentId).toBe('org-1');
    expect(stored()).toBe('org-1');
    expect(router.path).toBe(`/w/${WS1}`);

    // 계속 편집: 아무것도 바뀌지 않는다.
    guard.keep();
    await tick();
    expect(guard.open).toBe(false);
    expect(org.currentId).toBe('org-1');
    expect(stored()).toBe('org-1');
    expect(router.path).toBe(`/w/${WS1}`);

    // 다시 고르고 이번에는 버리고 떠나기: 선택·저장소·화면이 함께 바뀐다.
    await chooseOrganization(/^다른 조직/);
    expect(guard.open).toBe(true);
    guard.discard();
    await tick();
    expect(router.path).toBe('/');
    await waitFor(() => {
      expect(org.currentId).toBe('org-2');
    });
    expect(stored()).toBe('org-2');
    detach();
  });

  it('applies consecutive switches in order and drops a late count from the earlier organization', async () => {
    const resolvers = new Map<string, (count: number) => void>();
    vi.spyOn(activityApi, 'unreadCount').mockImplementation(
      (organizationId: string) =>
        new Promise<number>((resolve) => {
          resolvers.set(organizationId, resolve);
        }),
    );
    router.navigate('/');
    render(App);
    await screen.findByTestId('organization-switcher');
    await waitFor(() => {
      expect(resolvers.has('org-1')).toBe(true);
    });
    resolvers.get('org-1')?.(1);
    await screen.findByRole('link', { name: /읽지 않은 알림 1개/ });

    // 홈에서는 이동이 없으므로 확인 없이 바로 바뀐다. 두 번 연속 전환: 마지막 선택이 남는다.
    await chooseOrganization(/^다른 조직/);
    expect(org.currentId).toBe('org-2');
    await chooseOrganization(/^시험 조직/);
    expect(org.currentId).toBe('org-1');
    expect(stored()).toBe('org-1');

    // 이전 조직(org-2)의 안 읽은 수가 늦게 와도 현재 조직의 수를 덮지 않는다.
    await waitFor(() => {
      expect(resolvers.has('org-2')).toBe(true);
    });
    resolvers.get('org-1')?.(3);
    await tick();
    resolvers.get('org-2')?.(5);
    await tick();
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(screen.queryByRole('link', { name: /읽지 않은 알림 5개/ })).toBeNull();
    expect(screen.getByRole('link', { name: /읽지 않은 알림 3개/ })).toBeInTheDocument();
  });
});

describe('the sidebar layout (D-121)', () => {
  it('puts the organization › workspace switcher at the very top, before the brand, search and personal menu', async () => {
    router.navigate(`/w/${WS1}`);
    render(App);
    const organization = await screen.findByTestId('organization-switcher');
    const workspace = await screen.findByTestId('workspace-switcher');
    const before = (a: Element, b: Element): boolean =>
      (a.compareDocumentPosition(b) & Node.DOCUMENT_POSITION_FOLLOWING) !== 0;
    const sidebar = organization.closest('[data-sidebar="sidebar"]');
    if (sidebar === null) throw new Error('sidebar');
    const brand = Array.from(sidebar.querySelectorAll('span')).find(
      (s) => s.textContent === 'fvoci',
    );
    expect(before(organization, workspace)).toBe(true);
    expect(before(workspace, screen.getByTestId('search-open'))).toBe(true);
    expect(before(workspace, screen.getByRole('link', { name: '홈' }))).toBe(true);
    if (brand !== undefined) expect(before(workspace, brand)).toBe(true);
    // 전환기보다 앞에 오는 상호작용 요소가 없다(모바일 서랍의 닫기 단추만 예외).
    const first = sidebar.querySelector('button, a');
    expect(first === organization || first?.getAttribute('aria-label') === '메뉴 닫기').toBe(true);
  });
});
