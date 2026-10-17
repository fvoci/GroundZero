// 달력 배치 (D-31, D-50, D-81). 달 보기는 사용자의 한 주 시작(월·일)부터 6주(42일). 날짜 칸마다 업무(마감·시작)와 일정을 놓는다.
import type {
  CalendarDateKind,
  CalendarDateView,
  CalendarTaskView,
  EventOccurrenceView,
  EventView,
  ProjectedTaskView,
} from '$lib/api/client';
import type { WeekStart } from '$lib/display';
import { dayNumber, fromDayNumber } from '$lib/features/tasks/gantt';

import { clock, dayIn } from './time';

export type CalendarEntry =
  | { kind: 'task'; task: CalendarTaskView; label: string }
  | { kind: 'projected'; projected: ProjectedTaskView; label: string }
  | { kind: 'date'; date: CalendarDateView; label: string }
  | {
      kind: 'event';
      event: EventView;
      /** 일정 시간대 기준 회차 날짜(회차 빼기에 쓴다). */
      occurrence: string;
      label: string;
      sortKey: string;
    };

/** `YYYY-MM`의 달 보기 첫날과 끝날(끝 포함). 첫 줄은 `weekStart`(월·일)로 시작한다. */
export function monthRange(
  month: string,
  weekStart: WeekStart = 'sun',
): { from: string; to: string; days: string[] } {
  const first = dayNumber(`${month}-01`);
  // 1970-01-01은 목요일이다. 일요일=0.
  const weekday = (((first + 4) % 7) + 7) % 7;
  const lead = weekStart === 'mon' ? (weekday + 6) % 7 : weekday;
  const start = first - lead;
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
  projected: readonly ProjectedTaskView[],
  events: readonly EventView[],
  occurrences: readonly EventOccurrenceView[],
  from: string,
  to: string,
  viewerZone: string,
  dates: readonly CalendarDateView[] = [],
): Map<string, CalendarEntry[]> {
  const out = new Map<string, CalendarEntry[]>();
  const push = (day: string, entry: CalendarEntry): void => {
    const list = out.get(day);
    if (list === undefined) out.set(day, [entry]);
    else list.push(entry);
  };
  const byId = new Map(events.map((e) => [e.id, e]));
  for (const occ of occurrences) {
    const event = byId.get(occ.event_id);
    if (event === undefined) continue;
    const base = { kind: 'event' as const, event, occurrence: occ.occurrence };
    if (occ.start_date !== null && occ.end_date !== null) {
      for (const day of eachDay(occ.start_date, occ.end_date, from, to))
        push(day, { ...base, label: `종일 ${event.title}`, sortKey: '' });
    } else if (occ.starts_at !== null && occ.ends_at !== null) {
      const startDay = dayIn(occ.starts_at, viewerZone);
      // 끝 시각은 포함하지 않는다(자정에 끝나면 다음 날에 걸치지 않음).
      const endMs = Math.max(Date.parse(occ.starts_at), Date.parse(occ.ends_at) - 1);
      const endDay = dayIn(new Date(endMs).toISOString(), viewerZone);
      const time = clock(occ.starts_at, viewerZone);
      for (const day of eachDay(startDay, endDay, from, to))
        push(day, {
          ...base,
          label: day === startDay ? `${time} ${event.title}` : `(계속) ${event.title}`,
          sortKey: day === startDay ? time : '',
        });
    }
  }
  for (const p of projected) {
    const day = p.due_date ?? p.start_date;
    if (day === null || day < from || day > to) continue;
    push(day, {
      kind: 'projected',
      projected: p,
      label: `예정 ${p.milestone ? '◆ ' : ''}${p.title}`,
    });
  }
  for (const task of tasks) {
    const day = task.due_date ?? task.start_date;
    if (day === null || day < from || day > to) continue;
    const what = task.milestone ? '◆' : task.due_date === null ? '시작' : '마감';
    push(day, { kind: 'task', task, label: `${what} ${task.reference ?? ''} ${task.title}` });
  }
  for (const d of dates) {
    if (d.date < from || d.date > to) continue;
    push(d.date, {
      kind: 'date',
      date: d,
      label: `${DATE_LABEL[d.kind]}${d.personal ? '(내 계획)' : ''} ${d.title}`,
    });
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

/** 그 날짜가 든 주의 첫날(`weekStart` 월·일, D-81). */
export function startOfWeek(day: string, weekStart: WeekStart): string {
  const n = dayNumber(day);
  // 1970-01-01은 목요일이다. 일요일=0.
  const weekday = (((n + 4) % 7) + 7) % 7;
  const lead = weekStart === 'mon' ? (weekday + 6) % 7 : weekday;
  return fromDayNumber(n - lead);
}

/** 업무·일정 밖 날짜(D-80)의 머리말. */
export const DATE_LABEL: Record<CalendarDateKind, string> = {
  episode: '수강',
  study_unit: '공부',
  submission: '제출 예정',
  revision_due: '수정 마감',
  deliverable: '산출물 마감',
};
