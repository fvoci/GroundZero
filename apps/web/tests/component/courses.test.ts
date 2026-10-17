import { describe, expect, it } from 'vitest';

import type { EpisodeView } from '$lib/api/client';
import { parseDuration, parseEpisodesCsv } from '$lib/features/courses/csv';
import { courseProgress } from '$lib/features/courses/progress';

const ep = (
  n: number,
  over: Partial<EpisodeView['mine']> = {},
  base: Partial<EpisodeView> = {},
): EpisodeView => ({
  id: String(n),
  number: n,
  title: `회차 ${String(n)}`,
  chapter: '',
  duration_seconds: 600,
  url: '',
  materials: '',
  planned_on: null,
  mine: {
    status: 'not_started',
    planned_on: null,
    completed_on: null,
    watch_count: 0,
    last_position_seconds: null,
    note: '',
    ...over,
  },
  ...base,
});

describe('course progress', () => {
  it('counts completions once and measures by count and by length', () => {
    const p = courseProgress(
      [
        ep(
          1,
          { status: 'done', watch_count: 3, completed_on: '2026-10-17' },
          { duration_seconds: 1800 },
        ),
        ep(2, { status: 'in_progress' }, { planned_on: '2026-10-16' }),
        ep(3, {}, { planned_on: '2026-10-17' }),
        ep(4, { planned_on: '2026-10-20' }, { planned_on: '2026-10-10' }),
      ],
      '2026-10-17',
      '2026-10-19',
    );
    expect(p.done).toBe(1);
    expect(p.percentByCount).toBe(25);
    expect(p.percentByDuration).toBe(50);
    expect(p.todayPlanned).toBe(1);
    expect(p.todayDone).toBe(1);
    // 2번은 공통 계획일이 지났고, 4번은 내 날짜(20일)가 공통 계획일을 덮어 밀리지 않았다.
    expect(p.overdue.map((e) => e.number)).toEqual([2]);
    expect(p.daysLeft).toBe(3);
    expect(p.perDayEpisodes).toBe(1);
  });

  it('counts today only for episodes still done', () => {
    const p = courseProgress(
      [ep(1, { status: 'in_progress', completed_on: '2026-10-17' })],
      '2026-10-17',
      null,
    );
    expect(p.todayDone).toBe(0);
  });

  it('has no pace without a future target', () => {
    expect(courseProgress([ep(1)], '2026-10-17', '2026-10-01').perDayEpisodes).toBeNull();
    expect(courseProgress([], '2026-10-17', null).percentByCount).toBe(0);
  });
});

describe('episode CSV', () => {
  it('reads titles, chapters, durations and dates with quoting', () => {
    const r = parseEpisodesCsv(
      '제목,챕터,길이,계획일\n벡터,1장,12:30,2026-10-20\n"행렬, 기초",1장,1:02:03,\n행렬식,,45,\n',
    );
    expect(r.errors).toEqual([]);
    expect(r.rows.map((x) => [x.title, x.duration_seconds, x.planned_on])).toEqual([
      ['벡터', 750, '2026-10-20'],
      ['행렬, 기초', 3723, null],
      ['행렬식', 2700, null],
    ]);
  });

  it('reports bad lines by number', () => {
    const r = parseEpisodesCsv('벡터,,abc,\n,1장,,\n좋음,,,2026/10/20');
    expect(r.errors.map((e) => e.line)).toEqual([1, 2, 3]);
    expect(parseDuration('')).toBe(0);
  });

  it('keeps quoted line breaks, skips a header after a BOM or blank lines, checks limits', () => {
    const long = '가'.repeat(301);
    const r = parseEpisodesCsv(
      `\uFEFF\n제목,챕터,길이,계획일\n"여러\n줄",1장,10,\n${long},,1,\n긴 회차,,6001,`,
    );
    expect(r.rows.map((x) => x.title)).toEqual(['여러\n줄']);
    expect(r.errors).toEqual([
      { line: 5, reason: '제목은 300자까지입니다' },
      { line: 6, reason: '길이는 100시간까지입니다: 6001' },
    ]);
  });
});
