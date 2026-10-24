import type { Me } from '@fvoci/api-client';
import { cleanup, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { NotificationView } from '$lib/api/client';
import { api, NetworkError } from '$lib/api/client';
import { setLocale } from '$lib/paraglide/runtime.js';
import { router } from '$lib/router.svelte';

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
};

function unread(id: string): NotificationView {
  return {
    id,
    kind: 'mention',
    document_id: null,
    task_id: null,
    document_title: '문서',
    document_reference: null,
    workspace_id: null,
    item_id: null,
    comment_id: null,
    excerpt: null,
    due_date: null,
    subject: null,
    actor_name: null,
    created_at: '2026-10-21T00:00:00Z',
    read: false,
  };
}

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
  vi.spyOn(api, 'currentTimer').mockResolvedValue(null);
  vi.spyOn(api, 'deletedWorkspaces').mockResolvedValue([]);
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  router.navigate('/');
});

describe('unread notification count', () => {
  it('keeps the last count when reading it again fails, and retries on the next focus', async () => {
    let now = 1_000_000;
    vi.spyOn(Date, 'now').mockImplementation(() => now);
    const notifications = vi
      .spyOn(api, 'notifications')
      .mockResolvedValueOnce([unread('n1'), unread('n2')])
      .mockRejectedValueOnce(new NetworkError('offline'))
      .mockResolvedValue([unread('n1')]);
    render(App);
    const link = await screen.findByRole('link', { name: /읽지 않은 알림 2개/ });

    now += 60_000;
    window.dispatchEvent(new Event('focus'));
    await waitFor(() => {
      expect(notifications).toHaveBeenCalledTimes(2);
    });
    expect(link).toHaveAccessibleName(/읽지 않은 알림 2개/);

    // 실패 뒤에는 기다리지 않고 다시 묻는다.
    window.dispatchEvent(new Event('focus'));
    await waitFor(() => {
      expect(screen.getByRole('link', { name: /읽지 않은 알림 1개/ })).toBeInTheDocument();
    });
  });
});
