import { fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { NotificationView } from '$lib/api/client';
import { api, ApiError } from '$lib/api/client';
import NotificationsScreen from '$lib/features/references/NotificationsScreen.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';

afterEach(() => {
  vi.restoreAllMocks();
});

const unread: NotificationView = {
  id: 'n1',
  kind: 'mention',
  document_id: 'd1',
  task_id: null,
  document_title: '보고서',
  document_reference: null,
  workspace_id: 'w1',
  item_id: null,
  comment_id: null,
  excerpt: null,
  due_date: null,
  subject: null,
  actor_name: '민수',
  created_at: '2026-10-19T00:00:00Z',
  read: false,
};

// 감사 W25: "모두 읽음"이 앱 오류(세션 만료 401, DB 오류 500 등)로 실패하면 거부가 버려져 화면에 아무 말이 없었다
// (D-82). 실패를 단추 곁에 알리고, 목록은 그대로 두며, 바뀐 것이 없으니 사이드바 개수도 다시 세지 않는다.
describe('notifications mark all read', () => {
  it('says so beside the button when marking fails and keeps the list', async () => {
    await setLocale('ko', { reload: false });
    vi.spyOn(api, 'notifications').mockResolvedValue([unread]);
    vi.spyOn(api, 'markNotificationsRead').mockRejectedValue(
      new ApiError(401, 'unauthorized', 'Unauthorized'),
    );
    const onchanged = vi.fn();
    render(NotificationsScreen, { props: { workspaces: [], onchanged } });

    await fireEvent.click(await screen.findByRole('button', { name: '모두 읽음' }));

    const alert = await screen.findByRole('alert');
    expect(alert.textContent).toContain(m.references_notifications_read_failed());
    expect(alert.textContent).toContain(m.common_error_unauthorized());
    expect(screen.getByTestId('notifications').textContent).toContain('보고서');
    expect(onchanged).not.toHaveBeenCalled();
  });
});
