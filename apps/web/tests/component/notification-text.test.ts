import { type NotificationView } from '$lib/features/activity/api';
import { describe, expect, it } from 'vitest';

import { formatDay } from '$lib/display';
import { notificationText } from '$lib/features/references/notification-text';
import { m } from '$lib/paraglide/messages.js';

const base: NotificationView = {
  id: 'n1',
  kind: 'mention',
  organization_id: null,
  invitation_id: null,
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
  it('renders invitations without a document target in both languages', () => {
    const invite: NotificationView = {
      ...base,
      kind: 'invite',
      document_reference: null,
      document_title: null,
      due_date: null,
      actor_name: '민수',
      subject: '연구실',
    };
    expect(notificationText(invite)).toBe(
      m.references_notification_invite({ who: '민수', target: '연구실' }),
    );
    expect(notificationText({ ...invite, actor_name: null, subject: null }).trim()).toBe(
      m
        .references_notification_invite({ who: m.references_notification_someone(), target: '' })
        .trim(),
    );
    expect(notificationText({ ...base, kind: 'invite' })).toBe(
      m.references_notification_invite({
        who: m.references_notification_someone(),
        target: 'TSK-3 보고서',
      }),
    );
    expect(m.references_notification_invite({ who: 'Alex', target: 'Lab' }, { locale: 'en' })).toBe(
      'Alex invited you. Lab',
    );
    expect(
      m.references_notification_invite({ who: '민수', target: '연구실' }, { locale: 'ko' }),
    ).toBe('민수님이 초대했습니다. 연구실');
  });

  it('says what kind of notification it is', () => {
    const due = notificationText({ ...base, kind: 'due' });
    expect(due).toContain('TSK-3 보고서');
    // 날짜는 표시 언어의 서식으로(감사 W27, D-81).
    expect(due).toContain(formatDay('2026-10-20'));
    expect(due).not.toBe(notificationText({ ...base, kind: 'mention' }));
    expect(notificationText({ ...base, kind: 'comment', actor_name: '민수' })).toContain('민수');
    expect(notificationText({ ...base, kind: 'reply' })).not.toBe(
      notificationText({ ...base, kind: 'comment' }),
    );
  });
});
