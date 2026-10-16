// 간트 배치 (D-31). 날짜만 있는 업무(D-50)를 UTC 날짜 번호로 바꿔 열을 정한다(시간대에 따라 하루가 밀리지 않게).
import type { ListTask } from './board';
import { listOrder } from './board';

export interface GanttTask extends ListTask {
  start_date: string | null;
  due_date: string | null;
  milestone: boolean;
}

export interface GanttRow<T> {
  task: T;
  /** 첫 열을 0으로 하는 시작·끝 열(끝 포함). */
  from: number;
  to: number;
}

export interface GanttLayout<T> {
  /** 첫 열의 날짜(`YYYY-MM-DD`). */
  first: string;
  days: number;
  rows: GanttRow<T>[];
  undated: T[];
  /** 보이는 기간(최대 `MAX_GANTT_DAYS`일) 뒤에 있어 막대로 그리지 않은 업무. */
  later: T[];
}

/** 한 번에 그리는 최대 날짜 수(열이 끝없이 늘지 않게). */
export const MAX_GANTT_DAYS = 400;

const DAY_MS = 86_400_000;

export function dayNumber(date: string): number {
  const [y, m, d] = date.split('-').map(Number);
  return Math.floor(Date.UTC(y ?? 1970, (m ?? 1) - 1, d ?? 1) / DAY_MS);
}

export function fromDayNumber(day: number): string {
  return new Date(day * DAY_MS).toISOString().slice(0, 10);
}

/** 상위 업무 바로 뒤에 하위 업무가 오는 순서로, 날짜가 있는 업무를 막대로 놓는다. */
export function ganttLayout<T extends GanttTask>(tasks: readonly T[]): GanttLayout<T> {
  const undated: T[] = [];
  const spans: { task: T; begin: number; end: number }[] = [];
  for (const task of listOrder(tasks)) {
    const begin = task.start_date ?? task.due_date;
    const end = task.due_date ?? task.start_date;
    if (begin === null || end === null) undated.push(task);
    else spans.push({ task, begin: dayNumber(begin), end: dayNumber(end) });
  }
  if (spans.length === 0) return { first: '', days: 0, rows: [], undated, later: [] };
  const first = Math.min(...spans.map((s) => s.begin));
  const last = Math.min(Math.max(...spans.map((s) => s.end)), first + MAX_GANTT_DAYS - 1);
  const shown = spans.filter((s) => s.begin <= last);
  return {
    first: fromDayNumber(first),
    days: last - first + 1,
    rows: shown.map((s) => ({
      task: s.task,
      from: s.begin - first,
      to: Math.min(s.end, last) - first,
    })),
    undated,
    later: spans.filter((s) => s.begin > last).map((s) => s.task),
  };
}
