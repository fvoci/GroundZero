import { render } from '@testing-library/svelte';
import { afterEach, describe, expect, it } from 'vitest';

import type { TaskView } from '$lib/api/client';
import { dayNumber, ganttWeeks, isoWeek, weekday } from '$lib/features/tasks/gantt';
import GanttChart from '$lib/features/tasks/GanttChart.svelte';
import { me } from '$lib/me.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

// 간트의 주 머리(D-90): 주 경계는 사용자의 한 주 시작(D-81), 월요일 시작이면 ISO 8601 주차(주차 연도는 그 주
// 목요일의 연도), 일요일 시작이면 주차 번호 없이 날짜 범위만. 날짜 번호로만 계산해 시간대가 끼지 않는다.
const iso = (ymd: string): string => {
  const w = isoWeek(dayNumber(ymd));
  return `${String(w.year)}-W${String(w.week).padStart(2, '0')}`;
};

describe('ISO weeks', () => {
  it('knows the weekday of a date number', () => {
    expect(weekday(dayNumber('1970-01-01'))).toBe(4);
    expect(weekday(dayNumber('2026-10-19'))).toBe(1);
    expect(weekday(dayNumber('1969-12-28'))).toBe(0);
  });

  it('numbers ordinary weeks and keeps Monday to Sunday together', () => {
    expect(iso('2026-10-19')).toBe('2026-W43');
    expect(iso('2026-10-25')).toBe('2026-W43');
    expect(iso('2026-10-26')).toBe('2026-W44');
  });

  it('puts year-end and year-start days in the week-year of their Thursday', () => {
    // 2026년은 1월 1일이 목요일이라 53주가 있다. 2027-01-01(금)은 2026년 53주.
    expect(iso('2026-01-01')).toBe('2026-W01');
    expect(iso('2026-12-31')).toBe('2026-W53');
    expect(iso('2027-01-01')).toBe('2026-W53');
    expect(iso('2027-01-03')).toBe('2026-W53');
    expect(iso('2027-01-04')).toBe('2027-W01');
    // 2024-12-30(월)은 2025년 1주, 2021-01-03(일)은 2020년 53주.
    expect(iso('2024-12-30')).toBe('2025-W01');
    expect(iso('2021-01-03')).toBe('2020-W53');
    expect(iso('2025-12-28')).toBe('2025-W52');
  });

  it('handles leap days', () => {
    expect(iso('2024-02-29')).toBe('2024-W09');
    expect(iso('2028-02-29')).toBe('2028-W09');
  });
});

describe('gantt week segments', () => {
  it('splits shown days at the week start and measures partial weeks at both edges (Monday start)', () => {
    // 2026-10-21(수)부터 12일: 수~일 5칸, 다음 주 7칸.
    const weeks = ganttWeeks('2026-10-21', 12, 'mon');
    expect(weeks.map((w) => [w.start, w.span, w.from, w.to])).toEqual([
      [0, 5, '2026-10-19', '2026-10-25'],
      [5, 7, '2026-10-26', '2026-11-01'],
    ]);
    expect(weeks.map((w) => w.iso)).toEqual([
      { year: 2026, week: 43 },
      { year: 2026, week: 44 },
    ]);
    expect(weeks.reduce((n, w) => n + w.span, 0)).toBe(12);
  });

  it('follows a Sunday week start without ISO numbers', () => {
    const weeks = ganttWeeks('2026-10-21', 5, 'sun');
    expect(weeks.map((w) => [w.start, w.span, w.from, w.to, w.iso])).toEqual([
      [0, 4, '2026-10-18', '2026-10-24', undefined],
      [4, 1, '2026-10-25', '2026-10-31', undefined],
    ]);
  });

  it('crosses month and year boundaries with the right week-year', () => {
    const weeks = ganttWeeks('2026-12-30', 10, 'mon');
    expect(weeks.map((w) => [w.from, w.to, w.iso?.year, w.iso?.week])).toEqual([
      ['2026-12-28', '2027-01-03', 2026, 53],
      ['2027-01-04', '2027-01-10', 2027, 1],
    ]);
    expect(weeks.map((w) => w.span)).toEqual([5, 5]);
  });

  it('returns nothing for an empty chart', () => {
    expect(ganttWeeks('', 0, 'mon')).toEqual([]);
  });
});

describe('gantt week header', () => {
  afterEach(() => {
    me.weekStart = '';
  });

  const task = (id: string, start: string, due: string): TaskView => ({
    id,
    item_id: 'i1',
    module_id: 'm1',
    workspace_id: 'ws-1',
    parent_id: null,
    reference: `KED-${id}`,
    title: id,
    description: '',
    status: 'todo',
    priority: 'none',
    start_date: start,
    due_date: due,
    estimate_minutes: null,
    tags: [],
    milestone: false,
    predecessors: [],
    series: null,
    occurrence_date: null,
    assignees: [],
    checklist_done: 0,
    checklist_total: 0,
    version: 1,
    move_version: 1,
    completed_at: null,
    access: 'write',
    updated_at: '2026-10-21T00:00:00Z',
  });
  const weekCells = (): {
    from: string;
    column: string;
    text: string;
    spoken: string;
    title: string;
  }[] =>
    [...document.querySelectorAll<HTMLElement>('[data-gantt-week]')].map((el) => ({
      from: el.dataset.ganttWeek ?? '',
      column: el.style.gridColumn,
      text: el.querySelector('[aria-hidden="true"]')?.textContent.trim() ?? '',
      spoken: el.querySelector('.sr-only')?.textContent.trim() ?? '',
      title: el.title,
    }));

  it('shows ISO weeks with their real dates on a Monday week start, aligned to the day columns', async () => {
    await setLocale('ko', { reload: false });
    me.weekStart = 'mon';
    // 2026-12-30(수) ~ 2027-01-12(화): 2026년 53주(보이는 5칸)·2027년 1주·2027년 2주(2칸).
    render(GanttChart, {
      props: { tasks: [task('a', '2026-12-30', '2027-01-12')], onopen: () => undefined },
    });
    const cells = weekCells();
    expect(cells.map((c) => [c.from, c.column])).toEqual([
      ['2026-12-28', '2 / span 5'],
      ['2027-01-04', '7 / span 7'],
      ['2027-01-11', '14 / span 2'],
    ]);
    // 기본 열 1.75rem: 5칸(8.75rem)은 짧은 표기, 7칸(12.25rem)은 전체 표기, 2칸(3.5rem)은 비움.
    expect(cells.map((c) => c.text)).toEqual(['53주 · 12/28~', '2027년 1주 · 1/4–1/10', '']);
    expect(cells[0]?.spoken).toContain('2026년 53주(ISO 주차, 월요일 시작)');
    expect(cells[2]?.spoken).toContain('2027년 2주');
    // 주 경계·월 경계 열: 1월 1일은 월 경계, 1월 4일·11일은 주 경계.
    const edges = [...document.querySelectorAll<HTMLElement>('[data-gantt-day]')]
      .filter((el) => el.className.includes('border-l-foreground'))
      .map((el) => el.dataset.ganttDay);
    expect(edges).toEqual(['2027-01-01', '2027-01-04', '2027-01-11']);
  });

  it('shows date ranges without week numbers on a Sunday week start', async () => {
    await setLocale('en', { reload: false });
    me.weekStart = 'sun';
    render(GanttChart, {
      props: { tasks: [task('a', '2026-10-18', '2026-10-31')], onopen: () => undefined },
    });
    const cells = weekCells();
    expect(cells.map((c) => [c.from, c.column, c.text])).toEqual([
      ['2026-10-18', '2 / span 7', '10/18–10/24'],
      ['2026-10-25', '9 / span 7', '10/25–10/31'],
    ]);
    expect(cells[0]?.spoken).toContain('weeks start on Sunday, no week number');
    expect(cells.every((c) => !/Week \d/.test(c.text))).toBe(true);
  });

  it('keeps the year and the rule in the hover title even when the label is short', async () => {
    await setLocale('ko', { reload: false });
    me.weekStart = 'sun';
    // 2026-12-30(수) ~ 2027-01-01(금): 연도를 넘는 주가 3칸뿐이라 표기는 연도 없는 "12/27~"지만 title은 연도와 규칙까지 담은 전체 설명이다.
    render(GanttChart, {
      props: { tasks: [task('a', '2026-12-30', '2027-01-01')], onopen: () => undefined },
    });
    const [cell] = weekCells();
    expect(cell?.text).toBe('12/27~');
    expect(cell?.title).toBe(cell?.spoken);
    expect(cell?.title).toMatch(/2026.*27.*2027|27.*2026.*2027/);
    expect(cell?.title).toContain('일요일 시작, 주차 번호 없음');
  });
});
