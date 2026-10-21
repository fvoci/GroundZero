// 알림 문장(종류별). 알림 화면과 홈이 같이 쓴다(홈이 모든 알림을 "언급"으로 보이던 것을 고침).
import type { NotificationView } from '$lib/api/client';
import { formatDay } from '$lib/display';
import { m } from '$lib/paraglide/messages.js';

export function notificationText(n: NotificationView): string {
  const who = n.actor_name ?? m.references_notification_someone();
  const target = `${n.document_reference ?? ''} ${n.document_title}`.trim();
  // 날짜는 표시 언어의 서식으로(D-81).
  const date = n.due_date === null ? '' : formatDay(n.due_date);
  switch (n.kind) {
    case 'mention':
      return m.references_notification_mention({ who, target });
    case 'comment':
      return m.references_notification_comment({ who, target });
    case 'reply':
      return m.references_notification_reply({ who, target });
    case 'due':
      return m.references_notification_due({ target, date });
    case 'expiry':
      return m.references_notification_expiry({ target, date });
    case 'review':
      return m.references_notification_review({
        target,
        subject: n.subject ?? m.references_notification_review_subject(),
        date,
      });
    case 'deadline':
      return m.references_notification_deadline({ target, subject: n.subject ?? '', date });
    default:
      return target;
  }
}
