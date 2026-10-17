import { describe, expect, it } from 'vitest';

import type { StudyUnitView } from '$lib/api/client';
import { replan, studyProgress } from '$lib/features/study/progress';

const unit = (
  n: number,
  mine: Partial<StudyUnitView['mine']> = {},
  base: Partial<StudyUnitView> = {},
): StudyUnitView => ({
  id: `u${String(n)}`,
  number: n,
  title: `단위 ${String(n)}`,
  chapter: '',
  unit: 'pages',
  amount: 10,
  planned_on: null,
  mine: {
    status: 'not_started',
    done_amount: 0,
    planned_on: null,
    completed_on: null,
    note: '',
    ...mine,
  },
  ...base,
});

describe('study progress', () => {
  it('counts units, amounts by kind, today and overdue', () => {
    const p = studyProgress(
      [
        unit(1, { status: 'done', completed_on: '2026-10-17' }, { planned_on: '2026-10-17' }),
        unit(2, { status: 'in_progress', done_amount: 4 }, { planned_on: '2026-10-10' }),
        unit(
          3,
          { planned_on: '2026-10-20' },
          { planned_on: '2026-10-01', unit: 'problems', amount: 30 },
        ),
        unit(4, { done_amount: 50 }, { amount: 20 }),
      ],
      '2026-10-17',
    );
    expect([p.done, p.inProgress, p.remaining, p.percent]).toEqual([1, 1, 3, 25]);
    expect(p.amounts).toEqual([
      { unit: 'pages', target: 40, done: 34 },
      { unit: 'problems', target: 30, done: 0 },
    ]);
    expect([p.todayPlanned, p.todayDone]).toEqual([1, 1]);
    // 3번은 내 날짜(20일)가 공통 계획일을 덮어 밀리지 않았다.
    expect(p.overdue.map((u) => u.number)).toEqual([2]);
  });

  it('replans overdue units from the chosen day, n per day, by number', () => {
    expect(replan([unit(5), unit(2), unit(3)], '2026-10-30', 2)).toEqual([
      { unit_id: 'u2', planned_on: '2026-10-30' },
      { unit_id: 'u3', planned_on: '2026-10-30' },
      { unit_id: 'u5', planned_on: '2026-10-31' },
    ]);
    expect(replan([unit(1)], '2026-12-31', 0)[0]?.planned_on).toBe('2026-12-31');
  });
});
