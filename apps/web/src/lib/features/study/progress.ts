// 스터디 계획 대비 진행(D-36): 완료·진행 중·남은 단위, 분량 종류별 한 분량, 오늘 예정·완료, 밀린 단위.
// 밀린 단위 재배치는 사용자가 고른 시작일과 하루 단위 수로만 날짜를 정한다(X-07: 자동 추정 없음).
import type { StudyUnitKind, StudyUnitView } from '$lib/api/client';
import { m } from '$lib/paraglide/messages.js';

/** 분량 종류 이름. 언어가 바뀔 수 있어 부를 때마다 만든다(D-81). */
export function unitLabel(unit: StudyUnitKind): string {
  switch (unit) {
    case 'pages':
      return m.study_unit_pages();
    case 'problems':
      return m.study_unit_problems();
    case 'minutes':
      return m.study_unit_minutes();
    case 'items':
      return m.study_unit_items();
  }
}

/** 분량(예: "12쪽", "12 pages"). */
export function unitAmount(unit: StudyUnitKind, count: number): string {
  switch (unit) {
    case 'pages':
      return m.study_amount_pages({ count });
    case 'problems':
      return m.study_amount_problems({ count });
    case 'minutes':
      return m.study_amount_minutes({ count });
    case 'items':
      return m.study_amount_items({ count });
  }
}

/** 내 계획일(없으면 공통 계획일). */
export const plannedOn = (u: StudyUnitView): string | null => u.mine.planned_on ?? u.planned_on;

export interface StudyProgress {
  total: number;
  done: number;
  inProgress: number;
  remaining: number;
  percent: number;
  /** 분량 종류별 목표와 한 분량(완료 단위는 목표만큼 한 것으로 본다). */
  amounts: { unit: StudyUnitKind; target: number; done: number }[];
  todayPlanned: number;
  todayDone: number;
  overdue: StudyUnitView[];
}

export function studyProgress(units: readonly StudyUnitView[], today: string): StudyProgress {
  const done = units.filter((u) => u.mine.status === 'done');
  const amounts = new Map<StudyUnitKind, { target: number; done: number }>();
  for (const u of units) {
    const a = amounts.get(u.unit) ?? { target: 0, done: 0 };
    a.target += u.amount;
    a.done += u.mine.status === 'done' ? u.amount : Math.min(u.mine.done_amount, u.amount);
    amounts.set(u.unit, a);
  }
  return {
    total: units.length,
    done: done.length,
    inProgress: units.filter((u) => u.mine.status === 'in_progress').length,
    remaining: units.length - done.length,
    percent: units.length === 0 ? 0 : Math.round((done.length / units.length) * 100),
    amounts: [...amounts.entries()].map(([unit, a]) => ({ unit, ...a })),
    todayPlanned: units.filter((u) => plannedOn(u) === today).length,
    todayDone: done.filter((u) => u.mine.completed_on === today).length,
    overdue: units.filter((u) => {
      const p = plannedOn(u);
      return u.mine.status !== 'done' && p !== null && p < today;
    }),
  };
}

function addDays(day: string, n: number): string {
  const d = new Date(`${day}T00:00:00Z`);
  d.setUTCDate(d.getUTCDate() + n);
  return d.toISOString().slice(0, 10);
}

/** 밀린 단위를 번호순으로 시작일부터 하루 perDay개씩 배치한다. */
export function replan(
  overdue: readonly StudyUnitView[],
  start: string,
  perDay: number,
): { unit_id: string; planned_on: string }[] {
  const per = Math.max(1, Math.floor(perDay));
  return [...overdue]
    .sort((a, b) => a.number - b.number)
    .map((u, i) => ({ unit_id: u.id, planned_on: addDays(start, Math.floor(i / per)) }));
}
