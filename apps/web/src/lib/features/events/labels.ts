// 일정 표시 글자. 시각은 보는 사람의 시간대로 보이고, 일정의 시간대가 다르면 그 기준 시각도 함께 적는다(D-50).
// 글자는 부를 때 지금 언어로 만든다(언어는 실행 중에 바뀔 수 있다, D-81).
import type { EventKind, EventView, RecurrenceRule } from '$lib/api/client';
import { weekdays } from '$lib/display';
import { m } from '$lib/paraglide/messages.js';

import { clock, dayIn, wallTime } from './time';

export const KIND_LABEL: Record<EventKind, string> = {
  get meeting() {
    return m.events_kind_meeting();
  },
  get presentation() {
    return m.events_kind_presentation();
  },
  get event() {
    return m.events_kind_event();
  },
};

export function describeWhen(event: EventView, viewerZone: string): string {
  if (event.all_day) {
    const start = event.start_date ?? '';
    const end = event.end_date ?? start;
    return start === end
      ? m.events_when_all_day({ date: start })
      : m.events_when_all_day_range({ start, end });
  }
  if (event.starts_at === null || event.ends_at === null) return '';
  const startDay = dayIn(event.starts_at, viewerZone);
  const endDay = dayIn(event.ends_at, viewerZone);
  const span =
    startDay === endDay
      ? `${startDay} ${clock(event.starts_at, viewerZone)}–${clock(event.ends_at, viewerZone)}`
      : `${startDay} ${clock(event.starts_at, viewerZone)} ~ ${endDay} ${clock(event.ends_at, viewerZone)}`;
  if (event.timezone === viewerZone) return span;
  return m.events_when_in_zone({
    span,
    zone: event.timezone,
    time: wallTime(event.starts_at, event.timezone).replace('T', ' '),
  });
}

/** 반복 규칙의 요일 번호(1=월 … 7=일, ISO). */
export const ISO_WEEKDAYS = [1, 2, 3, 4, 5, 6, 7];

/** ISO 요일 번호(1=월 … 7=일)의 짧은 이름. 표시 언어를 따른다. */
export function weekdayLabel(iso: number): string {
  return weekdays().find((w) => w.index === iso % 7)?.short ?? '';
}

function every(rule: RecurrenceRule): string {
  const count = rule.interval;
  if (count === 1) {
    switch (rule.freq) {
      case 'daily':
        return m.events_rule_daily();
      case 'weekly':
        return m.events_rule_weekly();
      case 'monthly':
        return m.events_rule_monthly();
      case 'yearly':
        return m.events_rule_yearly();
    }
  }
  switch (rule.freq) {
    case 'daily':
      return m.events_rule_every_days({ count });
    case 'weekly':
      return m.events_rule_every_weeks({ count });
    case 'monthly':
      return m.events_rule_every_months({ count });
    case 'yearly':
      return m.events_rule_every_years({ count });
  }
}

/** 반복 규칙을 글로(예: "2주마다 월·목, 10회"). */
export function describeRule(rule: RecurrenceRule): string {
  let text = every(rule);
  if (rule.freq === 'weekly' && rule.weekdays.length > 0)
    text = m.events_rule_on_days({
      rule: text,
      days: rule.weekdays.map((d) => weekdayLabel(d)).join('·'),
    });
  if (rule.count !== null) return m.events_rule_count({ rule: text, count: rule.count });
  if (rule.until !== null) return m.events_rule_until({ rule: text, until: rule.until });
  return text;
}
