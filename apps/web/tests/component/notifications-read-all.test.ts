import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
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

  // 리뷰: 요청 중에도 단추가 살아 있어 두 번 누르면 읽음 요청이 겹쳤다. 늦게 실패한 요청이 이미 갱신된 목록 위에
  // 오류를 띄우면, 읽지 않은 알림이 없어 단추가 사라진 뒤라 다시 시도해 지울 수도 없었다. 요청 중에는 단추를 막고
  // 다시 들어오지 않는다.
  it('sends one request while marking is in flight and disables the button', async () => {
    await setLocale('ko', { reload: false });
    const read: NotificationView = { ...unread, read: true };
    vi.spyOn(api, 'notifications').mockResolvedValueOnce([unread]).mockResolvedValue([read]);
    let finish: () => void = () => undefined;
    const mark = vi.spyOn(api, 'markNotificationsRead').mockImplementation(
      () =>
        new Promise<undefined>((resolve) => {
          finish = () => {
            resolve(undefined);
          };
        }),
    );
    const onchanged = vi.fn();
    render(NotificationsScreen, { props: { workspaces: [], onchanged } });

    const button = await screen.findByRole('button', { name: '모두 읽음' });
    await fireEvent.click(button);
    await fireEvent.click(button);

    expect(mark).toHaveBeenCalledTimes(1);
    expect(button).toBeDisabled();

    finish();
    await waitFor(() => {
      expect(screen.queryByRole('button', { name: '모두 읽음' })).toBeNull();
    });
    expect(onchanged).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('enables the button again after a failed attempt so it can be retried', async () => {
    await setLocale('ko', { reload: false });
    vi.spyOn(api, 'notifications').mockResolvedValue([unread]);
    const mark = vi
      .spyOn(api, 'markNotificationsRead')
      .mockRejectedValue(new ApiError(500, 'internal', '서버 오류'));
    render(NotificationsScreen, { props: { workspaces: [], onchanged: vi.fn() } });

    const button = await screen.findByRole('button', { name: '모두 읽음' });
    await fireEvent.click(button);
    await screen.findByRole('alert');
    expect(button).toBeEnabled();

    await fireEvent.click(button);
    await waitFor(() => {
      expect(mark).toHaveBeenCalledTimes(2);
    });
  });
});
