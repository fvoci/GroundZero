import { timeData } from '$lib/features/time/data';
import type { Me } from '@fvoci/api-client';
import { cleanup, render, screen, waitFor } from '@testing-library/svelte';
import { tick } from 'svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { api, NetworkError } from '$lib/api/client';
import { setLocale } from '$lib/paraglide/runtime.js';
import { org } from '$lib/org.svelte';
import { router } from '$lib/router.svelte';
import { invitationChanges } from '$lib/features/home/invitations.svelte';

import { TEST_ORGANIZATION } from './fixtures/organization';

import App from '../../src/app/App.svelte';

// 읽지 않은 알림 수를 다시 읽다 실패하면 0이 되어 알림이 없는 것처럼 보였다(UI 감사). 앞의 수를 그대로 두고, 다음 창 초점
// 때 바로 다시 묻는다.
const me: Me = {
  id: 'user-a',
  display_name: '나',
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

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  vi.stubGlobal('matchMedia', (query: string) => ({
    matches: false,
    media: query,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
  }));
  vi.spyOn(globalThis, 'fetch').mockImplementation(() =>
    Promise.resolve(
      new Response(JSON.stringify({ error: 'not_found', message: 'not found' }), {
        status: 404,
        headers: { 'Content-Type': 'application/json' },
      }),
    ),
  );
  vi.spyOn(api, 'me').mockResolvedValue(me);
  vi.spyOn(api, 'workspaces').mockResolvedValue([]);
  vi.spyOn(api, 'myOrganizations').mockResolvedValue([TEST_ORGANIZATION]);
  vi.spyOn(api, 'unreadCount').mockResolvedValue(0);
  vi.spyOn(timeData, 'currentTimer').mockResolvedValue(null);
  vi.spyOn(api, 'deletedWorkspaces').mockResolvedValue([]);
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  router.navigate('/');
});

describe('unread notification count', () => {
  it('refreshes the server count immediately after an invitation changes', async () => {
    vi.spyOn(Date, 'now').mockReturnValue(1_000_000);
    const count = vi.spyOn(api, 'unreadCount').mockResolvedValueOnce(2).mockResolvedValue(1);
    render(App);
    await screen.findByRole('link', { name: /읽지 않은 알림 2개/ });
    invitationChanges.changed();
    await screen.findByRole('link', { name: /읽지 않은 알림 1개/ });
    expect(count).toHaveBeenCalledTimes(2);
  });
  it('keeps the last count when reading it again fails, and retries on the next focus', async () => {
    let now = 1_000_000;
    vi.spyOn(Date, 'now').mockImplementation(() => now);
    // 안 읽은 수는 서버가 현재 조직 범위로 센다(D-118).
    const unreadCount = vi
      .spyOn(api, 'unreadCount')
      .mockResolvedValueOnce(2)
      .mockRejectedValueOnce(new NetworkError('offline'))
      .mockResolvedValue(1);
    render(App);
    const link = await screen.findByRole('link', { name: /읽지 않은 알림 2개/ });

    now += 60_000;
    window.dispatchEvent(new Event('focus'));
    await waitFor(() => {
      expect(unreadCount).toHaveBeenCalledTimes(2);
    });
    expect(link).toHaveAccessibleName(/읽지 않은 알림 2개/);

    // 실패 뒤에는 기다리지 않고 다시 묻는다.
    window.dispatchEvent(new Event('focus'));
    await waitFor(() => {
      expect(screen.getByRole('link', { name: /읽지 않은 알림 1개/ })).toBeInTheDocument();
    });
  });
});

// 조직 선택이 풀린 뒤(조직에서 나가 목록이 비는 등) 이전 조직의 수가 뒤늦게 와도 배지에 들어가면 안 된다(CodeRabbit #141).
describe('unread count after the organization selection is cleared', () => {
  it('drops a late count from the previous organization', async () => {
    vi.spyOn(Date, 'now').mockReturnValue(1_000_000);
    let release: ((count: number) => void) | undefined;
    vi.spyOn(api, 'unreadCount').mockImplementation(
      () =>
        new Promise<number>((resolve) => {
          release = resolve;
        }),
    );
    render(App);
    await waitFor(() => {
      expect(release).toBeDefined();
    });
    org.load('user-a', []);
    await tick();
    release?.(5);
    await tick();
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(screen.queryByRole('link', { name: /읽지 않은 알림 5개/ })).toBeNull();
  });
});
