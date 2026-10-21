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
  /** 끝이 표시 기간 뒤라 마지막 열에서 잘라 그렸는지. */
  clippedEnd: boolean;
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
      clippedEnd: s.end > last,
    })),
    undated,
    later: spans.filter((s) => s.begin > last).map((s) => s.task),
  };
}

/** 간트에서 바꾸는 방법: 막대 옮기기(시작·마감 함께), 시작 끝, 마감 끝. */
export type GanttEdit = 'move' | 'start' | 'end';

/**
 * 막대를 `days`일 바꾼 날짜. 옮기기는 있는 날짜만 같이 옮긴다(없는 날짜는 그대로 없음). 끝을 끌면 시작·마감을 모두
 * 정한다(날짜 하나뿐인 업무도 기간이 된다). 시작이 마감보다 늦어지거나 마일스톤의 끝을 끌면 바꾸지 않는다(undefined).
 */
export function editDates(
  task: Pick<GanttTask, 'start_date' | 'due_date' | 'milestone'>,
  edit: GanttEdit,
  days: number,
): { start_date: string | null; due_date: string | null } | undefined {
  if (days === 0) return undefined;
  const begin = task.start_date ?? task.due_date;
  const end = task.due_date ?? task.start_date;
  if (begin === null || end === null) return undefined;
  const from = dayNumber(begin);
  const to = dayNumber(end);
  if (task.milestone) {
    return edit === 'move' ? { start_date: null, due_date: fromDayNumber(to + days) } : undefined;
  }
  if (edit === 'move') {
    return {
      start_date: task.start_date === null ? null : fromDayNumber(from + days),
      due_date: task.due_date === null ? null : fromDayNumber(to + days),
    };
  }
  const nextFrom = edit === 'start' ? from + days : from;
  const nextTo = edit === 'end' ? to + days : to;
  if (nextFrom > nextTo) return undefined;
  return { start_date: fromDayNumber(nextFrom), due_date: fromDayNumber(nextTo) };
}

/** 간트 연결선 하나: 선행 업무의 끝에서 후속 업무의 시작으로(끝나야 시작, D-31). 좌표는 막대와 같은 rem 단위다. */
export interface GanttLink {
  /** `선행 id>후속 id`. 같은 관계는 한 번만 그린다. */
  key: string;
  from: string;
  to: string;
  /** SVG 경로(직각). 마지막 구간은 늘 왼쪽에서 오른쪽으로 후속 업무의 시작에 들어간다. */
  d: string;
  /** 화살촉(후속 업무 시작 쪽). */
  arrow: string;
}

export interface GanttLinkGeometry {
  /** 날짜 열 너비(rem). */
  col: number;
  /** 줄 높이(rem). */
  row: number;
  /** 날짜 열 수(오른쪽 끝을 넘지 않게). */
  days: number;
}

/** 막대 안쪽 여백(막대가 열 경계에서 이만큼 들어가 그려진다, GanttChart와 같은 값). */
const BAR_INSET = 0.125;
/** 마일스톤(◆) 중심에서 꼭짓점까지(0.75rem 정사각형을 45° 돌린 것의 반 대각선보다 조금 크게). */
const DIAMOND_HALF = 0.55;
/** 막대 끝에서 꺾기 전 수평으로 나가는 길이, 화살촉 크기. */
const STUB = 0.4;
const ARROW = 0.3;
/** 이보다 좁으면(같은 날·겹침·역전) 꺾어 들어갈 틈이 없어 줄 경계로 돌아 들어간다. 바로 다음 날(막대 여백 두 개, 0.25)은 틈으로 본다. */
const MIN_GAP = 0.2;

const num = (n: number): string => String(Math.round(n * 1000) / 1000);

/**
 * 지금 그린 줄(`rows`, 끌고 있으면 미리 보기 자리가 들어간 것)만으로 연결선을 만든다. 끝 업무 중 하나라도 이 줄들에
 * 없으면(거르기·표시 기간 밖·날짜 없음·볼 수 없음) 그리지 않고 `hidden`으로 센다. 다른 업무나 가장자리에 잇지 않는다.
 * 날짜를 바꾸지 않는다: 시작·끝 열은 `ganttLayout`이 정한 그대로(끝 열 포함)다.
 */
export function ganttLinks(
  rows: readonly {
    task: { id: string; milestone: boolean; predecessor_ids: readonly string[] };
    from: number;
    to: number;
    /** 끝이 표시 기간 밖이라 잘려 그린 막대(그 끝에서는 선을 잇지 않는다). */
    clippedEnd?: boolean;
  }[],
  geometry: GanttLinkGeometry,
  /** 이 간트가 받은 모든 업무(그리지 못한 끝을 세는 데만 쓴다). */
  all: readonly { id: string; predecessor_ids: readonly string[] }[] = [],
): { links: GanttLink[]; hidden: number } {
  const { col, row, days } = geometry;
  const width = days * col;
  const at = new Map<
    string,
    { index: number; from: number; to: number; milestone: boolean; clippedEnd: boolean }
  >();
  rows.forEach((r, index) => {
    if (!at.has(r.task.id)) {
      at.set(r.task.id, {
        index,
        from: r.from,
        to: r.to,
        milestone: r.task.milestone,
        clippedEnd: r.clippedEnd === true,
      });
    }
  });
  const seen = new Set<string>();
  const links: GanttLink[] = [];
  let hidden = 0;
  const consider = (succId: string, predIds: readonly string[] | undefined): void => {
    for (const predId of predIds ?? []) {
      if (typeof predId !== 'string' || predId === succId) continue;
      const key = `${predId}>${succId}`;
      if (seen.has(key)) continue;
      seen.add(key);
      const pred = at.get(predId);
      const succ = at.get(succId);
      // 선행 막대의 끝이 잘려 있으면 진짜 끝을 모르므로 가장자리에서 잇지 않는다.
      if (pred === undefined || succ === undefined || pred.clippedEnd) {
        hidden += 1;
        continue;
      }
      const x1 = pred.milestone
        ? pred.to * col + col / 2 + DIAMOND_HALF
        : (pred.to + 1) * col - BAR_INSET;
      const x2 = succ.milestone
        ? succ.from * col + col / 2 - DIAMOND_HALF
        : succ.from * col + BAR_INSET;
      const y1 = (pred.index + 0.5) * row;
      const y2 = (succ.index + 0.5) * row;
      let d: string;
      if (x2 - x1 >= MIN_GAP) {
        // 후속이 뒤: 선행 끝에서 조금 나가(바로 다음 날처럼 틈이 좁으면 틈의 가운데에서) 꺾어 내려(올라)가고 후속
        // 시작으로 들어간다.
        const xm = x2 - x1 >= 2 * STUB ? x1 + STUB : (x1 + x2) / 2;
        d = `M${num(x1)} ${num(y1)}H${num(xm)}V${num(y2)}H${num(x2)}`;
      } else {
        // 겹치거나 후속이 앞에 있음: 선행 줄의 아래(후속이 위면 위) 경계선을 따라 되돌아가서 후속 시작 앞에서 들어간다.
        // 막대는 줄 안쪽(위아래 여백)에 그려지므로 경계선 위의 선은 막대를 가로지르지 않는다. 표의 양 끝을 넘지 않는다.
        const yb = (succ.index > pred.index ? pred.index + 1 : pred.index) * row;
        const xa = Math.min(x1 + STUB, width);
        const xb = Math.max(x2 - STUB, 0);
        d = `M${num(x1)} ${num(y1)}H${num(xa)}V${num(yb)}H${num(xb)}V${num(y2)}H${num(x2)}`;
      }
      const base = Math.max(x2 - ARROW, 0);
      const arrow = `M${num(x2)} ${num(y2)}L${num(base)} ${num(y2 - ARROW * 0.7)}L${num(base)} ${num(y2 + ARROW * 0.7)}Z`;
      links.push({ key, from: predId, to: succId, d, arrow });
    }
  };
  for (const r of rows) consider(r.task.id, r.task.predecessor_ids);
  // 그리지 않은 업무(날짜 없음·기간 밖)가 끝인 관계도 센다. 이미 본 관계는 다시 세지 않는다.
  for (const t of all) if (!at.has(t.id)) consider(t.id, t.predecessor_ids);
  return { links, hidden };
}
