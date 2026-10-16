// 일정 표시 글자. 시각은 보는 사람의 시간대로 보이고, 일정의 시간대가 다르면 그 기준 시각도 함께 적는다(D-50).
import type { EventKind, EventView } from '$lib/api/client';

import { clock, dayIn, wallTime } from './time';

export const KIND_LABEL: Record<EventKind, string> = {
  meeting: '회의',
  presentation: '발표',
  event: '행사',
};

export function describeWhen(event: EventView, viewerZone: string): string {
  if (event.all_day) {
    const start = event.start_date ?? '';
    const end = event.end_date ?? start;
    return start === end ? `${start} 종일` : `${start} ~ ${end} 종일`;
  }
  if (event.starts_at === null || event.ends_at === null) return '';
  const startDay = dayIn(event.starts_at, viewerZone);
  const endDay = dayIn(event.ends_at, viewerZone);
  const span =
    startDay === endDay
      ? `${startDay} ${clock(event.starts_at, viewerZone)}–${clock(event.ends_at, viewerZone)}`
      : `${startDay} ${clock(event.starts_at, viewerZone)} ~ ${endDay} ${clock(event.ends_at, viewerZone)}`;
  if (event.timezone === viewerZone) return span;
  return `${span} (${event.timezone} 기준 ${wallTime(event.starts_at, event.timezone).replace('T', ' ')})`;
}
