// 달력 배치 (D-31, D-50). 달 보기는 일요일부터 6주(42일). 날짜 칸마다 업무(마감·시작)와 일정을 놓는다.
import type { CalendarTaskView, EventView } from '$lib/api/client';
import { dayNumber, fromDayNumber } from '$lib/features/tasks/gantt';

import { clock, dayIn } from './time';

export type CalendarEntry =
  | { kind: 'task'; task: CalendarTaskView; label: string }
  | { kind: 'event'; event: EventView; label: string; sortKey: string };

/** `YYYY-MM`의 달 보기 첫날과 끝날(끝 포함). */
export function monthRange(month: string): { from: string; to: string; days: string[] } {
  const first = dayNumber(`${month}-01`);
  // 1970-01-01은 목요일이다. 일요일=0.
  const weekday = (((first + 4) % 7) + 7) % 7;
  const start = first - weekday;
  const days = Array.from({ length: 42 }, (_, i) => fromDayNumber(start + i));
  return { from: days[0] ?? '', to: days[41] ?? '', days };
}

export function shiftMonth(month: string, by: number): string {
  const [y = 1970, m = 1] = month.split('-').map(Number);
  const index = y * 12 + (m - 1) + by;
  return `${String(Math.floor(index / 12))}-${String((index % 12) + 1).padStart(2, '0')}`;
}

function eachDay(from: string, to: string, lo: string, hi: string): string[] {
  const a = Math.max(dayNumber(from), dayNumber(lo));
  const b = Math.min(dayNumber(to), dayNumber(hi));
  const out: string[] = [];
  for (let d = a; d <= b; d += 1) out.push(fromDayNumber(d));
  return out;
}

/**
 * 날짜별 항목. 업무는 마감일(없으면 시작일) 한 칸에만 놓아 여러 날 업무가 칸을 채우지 않게 한다.
 * 일정은 걸친 날마다 놓는다. 시각 있는 일정의 날짜는 보는 사람의 시간대로 정한다.
 */
export function entriesByDay(
  tasks: readonly CalendarTaskView[],
  events: readonly EventView[],
  from: string,
  to: string,
  viewerZone: string,
): Map<string, CalendarEntry[]> {
  const out = new Map<string, CalendarEntry[]>();
  const push = (day: string, entry: CalendarEntry): void => {
    const list = out.get(day);
    if (list === undefined) out.set(day, [entry]);
    else list.push(entry);
  };
  for (const event of events) {
    if (event.all_day && event.start_date !== null && event.end_date !== null) {
      for (const day of eachDay(event.start_date, event.end_date, from, to))
        push(day, { kind: 'event', event, label: `종일 ${event.title}`, sortKey: '' });
    } else if (event.starts_at !== null && event.ends_at !== null) {
      const startDay = dayIn(event.starts_at, viewerZone);
      const endDay = dayIn(event.ends_at, viewerZone);
      const time = clock(event.starts_at, viewerZone);
      for (const day of eachDay(startDay, endDay, from, to))
        push(day, {
          kind: 'event',
          event,
          label: day === startDay ? `${time} ${event.title}` : `(계속) ${event.title}`,
          sortKey: day === startDay ? time : '',
        });
    }
  }
  for (const task of tasks) {
    const day = task.due_date ?? task.start_date;
    if (day === null || day < from || day > to) continue;
    const what = task.milestone ? '◆' : task.due_date === null ? '시작' : '마감';
    push(day, { kind: 'task', task, label: `${what} ${task.reference ?? ''} ${task.title}` });
  }
  for (const list of out.values()) {
    list.sort((a, b) => {
      const ka = a.kind === 'event' ? `0${a.sortKey}` : '1';
      const kb = b.kind === 'event' ? `0${b.sortKey}` : '1';
      return ka.localeCompare(kb);
    });
  }
  return out;
}
