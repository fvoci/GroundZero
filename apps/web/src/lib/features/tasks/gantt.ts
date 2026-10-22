// 간트 배치 (D-31). 날짜만 있는 업무(D-50)를 UTC 날짜 번호로 바꿔 열을 정한다(시간대에 따라 하루가 밀리지 않게).
import type { LinkKind, TaskLinkView } from '$lib/api/client';
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

/** 업무가 가진 선행 관계. */
export interface GanttLinkSource {
  id: string;
  /** 서버 응답이 아닌 업무(그리는 중인 미리보기 등)에는 없을 수 있다(없으면 관계 없음). */
  predecessors?: readonly TaskLinkView[];
}

/**
 * 간트 연결선 하나. 종류에 따라 선행 업무의 시작(SS·SF) 또는 끝(FS·FF)에서 나와 후속 업무의 시작(FS·SS) 또는
 * 끝(FF·SF)으로 들어간다(D-91). 화살촉은 늘 후속 업무 쪽이다. 좌표는 막대와 같은 rem 단위다.
 */
export interface GanttLink {
  /** `선행 id>후속 id`. 같은 관계는 한 번만 그린다. */
  key: string;
  from: string;
  to: string;
  kind: LinkKind;
  lag_days: number;
  /** SVG 경로(직각). 마지막 구간은 후속 업무의 들어가는 끝(시작이면 왼쪽에서, 끝이면 오른쪽에서)으로 들어간다. */
  d: string;
  /** 화살촉(후속 업무의 들어가는 끝). */
  arrow: string;
}

/** 업무의 선행 관계 목록(종류·지연일). */
export function linksOf(task: GanttLinkSource): TaskLinkView[] {
  return [...(task.predecessors ?? [])];
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
    task: GanttLinkSource & { milestone: boolean };
    from: number;
    to: number;
    /** 끝이 표시 기간 밖이라 잘려 그린 막대(그 끝에서는 선을 잇지 않는다). */
    clippedEnd?: boolean;
  }[],
  geometry: GanttLinkGeometry,
  /** 이 간트가 받은 모든 업무(그리지 못한 끝을 세는 데만 쓴다). */
  all: readonly GanttLinkSource[] = [],
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
  // 막대의 시작(왼쪽)·끝(오른쪽) 가장자리. 마일스톤은 ◆의 왼쪽·오른쪽 꼭짓점.
  const startX = (b: { from: number; milestone: boolean }): number =>
    b.milestone ? b.from * col + col / 2 - DIAMOND_HALF : b.from * col + BAR_INSET;
  const endX = (b: { to: number; milestone: boolean }): number =>
    b.milestone ? b.to * col + col / 2 + DIAMOND_HALF : (b.to + 1) * col - BAR_INSET;
  const consider = (succ: GanttLinkSource): void => {
    for (const { task_id: predId, kind, lag_days } of linksOf(succ)) {
      if (typeof predId !== 'string' || predId === succ.id) continue;
      const key = `${predId}>${succ.id}`;
      if (seen.has(key)) continue;
      seen.add(key);
      const pred = at.get(predId);
      const next = at.get(succ.id);
      // 선행에서 나가는 끝: FS·FF는 끝(오른쪽으로 나감), SS·SF는 시작(왼쪽으로 나감).
      // 후속에 들어가는 끝: FS·SS는 시작(왼쪽에서 오른쪽으로 들어감), FF·SF는 끝(오른쪽에서 왼쪽으로 들어감).
      const fromEnd = kind === 'FS' || kind === 'FF';
      const toStart = kind === 'FS' || kind === 'SS';
      // 잘린 막대의 끝은 진짜 끝을 모르므로 그 끝에서 잇지 않는다.
      if (
        pred === undefined ||
        next === undefined ||
        (fromEnd && pred.clippedEnd) ||
        (!toStart && next.clippedEnd)
      ) {
        hidden += 1;
        continue;
      }
      const x1 = fromEnd ? endX(pred) : startX(pred);
      const x2 = toStart ? startX(next) : endX(next);
      // 끄는 중의 미리 보기로 끝이 표 밖(첫 열 앞·마지막 열 뒤)으로 나가면 그 끝은 보이지 않는다. 가장자리에 붙여 잇지
      // 않고 다른 보이지 않는 끝과 같이 센다(놓아 저장하면 다시 읽은 기간에서 다시 그린다).
      if (x1 < 0 || x1 > width || x2 < 0 || x2 > width) {
        hidden += 1;
        continue;
      }
      const y1 = (pred.index + 0.5) * row;
      const y2 = (next.index + 0.5) * row;
      // 나가는 쪽(+1 오른쪽, -1 왼쪽)과 들어가며 나아가는 쪽(+1 왼쪽에서 오른쪽으로, -1 오른쪽에서 왼쪽으로).
      const out = fromEnd ? 1 : -1;
      const dir = toStart ? 1 : -1;
      const clamp = (x: number): number => Math.min(Math.max(x, 0), width);
      let d: string;
      if (out !== dir) {
        // SS는 두 시작보다 왼쪽, FF는 두 끝보다 오른쪽에서 세로로 돌아 들어간다(막대를 가로지르지 않음).
        const xm = clamp(out < 0 ? Math.min(x1, x2) - STUB : Math.max(x1, x2) + STUB);
        d = `M${num(x1)} ${num(y1)}H${num(xm)}V${num(y2)}H${num(x2)}`;
      } else if ((x2 - x1) * dir >= MIN_GAP) {
        // 나아가는 쪽으로 틈이 있음(FS는 후속이 뒤, SF는 후속 끝이 선행 시작보다 앞): 조금 나가(틈이 좁으면 틈의
        // 가운데에서) 꺾어 내려(올라)가고 들어간다.
        const gap = (x2 - x1) * dir;
        const xm = gap >= 2 * STUB ? x1 + STUB * dir : (x1 + x2) / 2;
        d = `M${num(x1)} ${num(y1)}H${num(xm)}V${num(y2)}H${num(x2)}`;
      } else {
        // 틈이 없음(같은 날·겹침·역전): 선행 줄의 아래(후속이 위면 위) 경계선을 따라 되돌아가서 들어가는 끝 앞에서
        // 들어간다. 막대는 줄 안쪽(위아래 여백)에 그려지므로 경계선 위의 선은 막대를 가로지르지 않는다. 표의 양 끝을
        // 넘지 않는다.
        const yb = (next.index > pred.index ? pred.index + 1 : pred.index) * row;
        const xa = clamp(x1 + STUB * dir);
        const xb = clamp(x2 - STUB * dir);
        d = `M${num(x1)} ${num(y1)}H${num(xa)}V${num(yb)}H${num(xb)}V${num(y2)}H${num(x2)}`;
      }
      const base = clamp(x2 - ARROW * dir);
      const arrow = `M${num(x2)} ${num(y2)}L${num(base)} ${num(y2 - ARROW * 0.7)}L${num(base)} ${num(y2 + ARROW * 0.7)}Z`;
      links.push({ key, from: predId, to: succ.id, kind, lag_days, d, arrow });
    }
  };
  for (const r of rows) consider(r.task);
  // 그리지 않은 업무(날짜 없음·기간 밖)가 끝인 관계도 센다. 이미 본 관계는 다시 세지 않는다.
  for (const t of all) if (!at.has(t.id)) consider(t);
  return { links, hidden };
}

/** 날짜 번호의 요일(일요일=0). 1970-01-01(번호 0)은 목요일이다. */
export function weekday(day: number): number {
  return (((day + 4) % 7) + 7) % 7;
}

/**
 * ISO 8601 주차(월요일 시작). 주차 연도는 그 주의 목요일이 속한 달력 연도라서 달력 연도와 다를 수 있다
 * (예: 2024-12-30은 2025년 1주, 2027-01-01은 2026년 53주). 날짜 번호로만 계산해 시간대가 끼지 않는다.
 */
export function isoWeek(day: number): { year: number; week: number } {
  const monday = day - ((weekday(day) + 6) % 7);
  const thursday = monday + 3;
  const year = Number(fromDayNumber(thursday).slice(0, 4));
  const jan1 = dayNumber(`${String(year).padStart(4, '0')}-01-01`);
  return { year, week: Math.floor((thursday - jan1) / 7) + 1 };
}

/** 간트의 주 머리 한 칸: 보이는 첫 열·열 수와 그 주 전체(보이는 칸이 주 일부여도 주의 실제 첫날·끝날). */
export interface GanttWeek {
  /** 보이는 첫 열(0부터). */
  start: number;
  /** 보이는 열 수(표가 주 중간에서 시작·끝나면 7보다 작다). */
  span: number;
  /** 그 주의 실제 첫날·끝날(`YYYY-MM-DD`). */
  from: string;
  to: string;
  /** 월요일 시작일 때만 ISO 주차. 일요일 시작이면 ISO 주차가 정의되지 않아 없다. */
  iso: { year: number; week: number } | undefined;
}

/**
 * 보이는 날짜 열을 주로 나눈다. 주 경계는 사용자의 한 주 시작(D-81)을 따른다. 월요일 시작이면 ISO 주차를 붙이고,
 * 일요일 시작이면 주차 번호 없이 날짜 범위만 둔다(D-90).
 */
export function ganttWeeks(first: string, days: number, weekStart: 'mon' | 'sun'): GanttWeek[] {
  if (days <= 0 || first === '') return [];
  const firstDay = dayNumber(first);
  const startDow = weekStart === 'mon' ? 1 : 0;
  const out: GanttWeek[] = [];
  let i = 0;
  while (i < days) {
    const day = firstDay + i;
    const weekFirst = day - ((weekday(day) - startDow + 7) % 7);
    const weekLast = weekFirst + 6;
    const span = Math.min(weekLast - day + 1, days - i);
    out.push({
      start: i,
      span,
      from: fromDayNumber(weekFirst),
      to: fromDayNumber(weekLast),
      iso: weekStart === 'mon' ? isoWeek(weekFirst) : undefined,
    });
    i += span;
  }
  return out;
}
