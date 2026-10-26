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
import { displaySettings, formatClock } from '$lib/display';
import { m } from '$lib/paraglide/messages.js';
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
 * 날짜별 항목. 업무는 시작·마감이 모두 있으면 기간에, 하나만 있으면 그 날짜에 놓는다(D-121).
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
        push(day, { ...base, label: m.events_entry_all_day({ title: event.title }), sortKey: '' });
    } else if (occ.starts_at !== null && occ.ends_at !== null) {
      const startDay = dayIn(occ.starts_at, viewerZone);
      // 끝 시각은 포함하지 않는다(자정에 끝나면 다음 날에 걸치지 않음).
      const endMs = Math.max(Date.parse(occ.starts_at), Date.parse(occ.ends_at) - 1);
      const endDay = dayIn(new Date(endMs).toISOString(), viewerZone);
      const time = clock(occ.starts_at, viewerZone);
      const shown = formatClock(occ.starts_at, { ...displaySettings(), timeZone: viewerZone });
      for (const day of eachDay(startDay, endDay, from, to))
        push(day, {
          ...base,
          label:
            day === startDay
              ? m.events_entry_timed({ time: shown, title: event.title })
              : m.events_entry_continued({ title: event.title }),
          sortKey: day === startDay ? time : '',
        });
    } else if (occ.skipped && occ.occurrence >= from && occ.occurrence <= to) {
      // 일정 시간대에 없는 현지 시각이라 만들지 않은 반복 회차(D-81). 날짜는 일정 시간대 기준.
      push(occ.occurrence, {
        ...base,
        label: m.events_entry_skipped({ title: event.title }),
        sortKey: '',
      });
    }
  }
  for (const p of projected) {
    const day = p.due_date ?? p.start_date;
    if (day === null || day < from || day > to) continue;
    push(day, {
      kind: 'projected',
      projected: p,
      label: p.milestone
        ? m.events_entry_projected_milestone({ title: p.title })
        : m.events_entry_projected({ title: p.title }),
    });
  }
  for (const task of tasks) {
    const day = task.due_date ?? task.start_date;
    if (day === null) continue;
    const words = { reference: task.reference ?? '', title: task.title };
    const days =
      task.start_date !== null && task.due_date !== null
        ? eachDay(task.start_date, task.due_date, from, to)
        : eachDay(day, day, from, to);
    for (const date of days) {
      const label = task.milestone
        ? m.events_entry_milestone(words)
        : date === task.due_date
          ? m.events_entry_task_due(words)
          : date === task.start_date
            ? m.events_entry_task_start(words)
            : m.events_entry_task_continued(words);
      push(date, { kind: 'task', task, label });
    }
  }
  for (const d of dates) {
    if (d.date < from || d.date > to) continue;
    push(d.date, {
      kind: 'date',
      date: d,
      label: d.personal
        ? m.events_entry_date_personal({ label: dateLabel(d.kind), title: d.title })
        : m.events_entry_date({ label: dateLabel(d.kind), title: d.title }),
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

/** 업무·일정 밖 날짜(D-80)의 머리말(표시 언어, D-81). */
export function dateLabel(kind: CalendarDateKind): string {
  switch (kind) {
    case 'episode':
      return m.events_date_episode();
    case 'study_unit':
      return m.events_date_study_unit();
    case 'submission':
      return m.events_date_submission();
    case 'revision_due':
      return m.events_date_revision_due();
    case 'deliverable':
      return m.events_date_deliverable();
  }
}

/** 토·일인지(YYYY-MM-DD, 날짜만 보므로 시간대와 상관없다). 주 시작 요일 설정과 무관하게 칸에 음영을 줄 때 쓴다. */
export function isWeekend(day: string): boolean {
  const d = new Date(`${day}T00:00:00Z`).getUTCDay();
  return d === 0 || d === 6;
}
