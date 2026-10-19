import { describe, expect, it } from 'vitest';

import type { NotificationView } from '$lib/api/client';
import { notificationText } from '$lib/features/references/notification-text';

const base: NotificationView = {
  id: 'n1',
  kind: 'mention',
  document_id: null,
  task_id: 't1',
  document_title: '보고서',
  document_reference: 'TSK-3',
  workspace_id: 'w1',
  item_id: 'i1',
  comment_id: null,
  excerpt: null,
  due_date: '2026-10-20',
  subject: null,
  actor_name: null,
  created_at: '2026-10-19T00:00:00Z',
  read: false,
};

// 홈은 모든 알림을 "나를 언급했습니다"로 보였다(감사 반영). 알림 화면과 같은 종류별 문장을 쓴다.
describe('notificationText', () => {
  it('says what kind of notification it is', () => {
    const due = notificationText({ ...base, kind: 'due' });
    expect(due).toContain('TSK-3 보고서');
    expect(due).toContain('2026-10-20');
    expect(due).not.toBe(notificationText({ ...base, kind: 'mention' }));
    expect(notificationText({ ...base, kind: 'comment', actor_name: '민수' })).toContain('민수');
    expect(notificationText({ ...base, kind: 'reply' })).not.toBe(
      notificationText({ ...base, kind: 'comment' }),
    );
  });
});
