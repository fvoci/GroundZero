import { afterEach, describe, expect, it } from 'vitest';

import type { DisplaySettings } from '$lib/display';
import {
  browserLocale,
  conventionalHourCycle,
  conventionalWeekStart,
  displaySettings,
  formatDay,
  formatInstant,
  weekdays,
} from '$lib/display';
import { monthRange, startOfWeek } from '$lib/features/events/calendar';
import { dayIn, instantOf, wallTime } from '$lib/features/events/time';
import { me } from '$lib/me.svelte';

const ko: DisplaySettings = {
  locale: 'ko',
  timeZone: 'Asia/Seoul',
  weekStart: 'mon',
  hourCycle: 'h23',
};
const en: DisplaySettings = {
  locale: 'en',
  timeZone: 'America/New_York',
  weekStart: 'sun',
  hourCycle: 'h12',
};

afterEach(() => {
  Object.assign(me, { timeZone: '', locale: '', weekStart: '', hourCycle: '' });
});

describe('display settings (D-81)', () => {
  it('suggests from the browser and keeps saved values first', () => {
    expect(browserLocale(['ko-KR', 'en-US'])).toBe('ko');
    expect(browserLocale(['en-GB'])).toBe('en');
    expect(conventionalWeekStart('ko')).toBe('sun');
    expect(conventionalHourCycle('en')).toBe('h12');
    Object.assign(me, { timeZone: 'Asia/Seoul', locale: 'en', weekStart: 'mon', hourCycle: 'h23' });
    expect(displaySettings()).toEqual({
      locale: 'en',
      timeZone: 'Asia/Seoul',
      weekStart: 'mon',
      hourCycle: 'h23',
    });
  });

  it('lays out months and weeks from the chosen week start', () => {
    // 2026-11-01은 일요일이다.
    expect(monthRange('2026-11', 'sun').from).toBe('2026-11-01');
    expect(monthRange('2026-11', 'mon').from).toBe('2026-10-26');
    expect(monthRange('2026-11', 'mon').days).toHaveLength(42);
    expect(startOfWeek('2026-10-21', 'sun')).toBe('2026-10-18');
    expect(startOfWeek('2026-10-21', 'mon')).toBe('2026-10-19');
    expect(startOfWeek('2026-10-18', 'mon')).toBe('2026-10-12');
    expect(weekdays(en).map((w) => w.index)).toEqual([0, 1, 2, 3, 4, 5, 6]);
    expect(weekdays(ko).map((w) => w.index)).toEqual([1, 2, 3, 4, 5, 6, 0]);
    expect(weekdays(en)[0]?.short).toBe('Sun');
    expect(weekdays(ko)[0]?.short).toBe('월');
  });

  it('puts the same instant on different days in Korea and the US', () => {
    const instant = '2026-10-20T15:30:00Z';
    expect(dayIn(instant, 'Asia/Seoul')).toBe('2026-10-21');
    expect(dayIn(instant, 'America/Los_Angeles')).toBe('2026-10-20');
  });

  it('keeps date-only values on their date in any zone', () => {
    expect(formatDay('2026-03-08', { dateStyle: 'medium' }, en)).toBe('Mar 8, 2026');
    expect(
      formatDay('2026-03-08', { dateStyle: 'medium' }, { ...en, timeZone: 'Pacific/Kiritimati' }),
    ).toBe('Mar 8, 2026');
  });

  it('formats instants with the 12/24 hour choice and across a DST change', () => {
    const afternoon = '2026-10-20T06:05:00Z';
    expect(formatInstant(afternoon, { hour: 'numeric', minute: '2-digit' }, ko)).toBe('15:05');
    expect(
      formatInstant(afternoon, { hour: 'numeric', minute: '2-digit' }, { ...ko, hourCycle: 'h12' }),
    ).toContain('3:05');
    // 2026-03-08 미국 동부 여름시간 시작(02:00 → 03:00). 그 앞뒤 시각은 한 시간 차이 없이 이어진다.
    expect(wallTime('2026-03-08T06:59:00Z', 'America/New_York')).toBe('2026-03-08T01:59');
    expect(wallTime('2026-03-08T07:00:00Z', 'America/New_York')).toBe('2026-03-08T03:00');
    // 가을의 겹친 시각(01:30)은 앞선 시각으로 읽는다.
    expect(instantOf('2026-11-01T01:30', 'America/New_York')).toBe('2026-11-01T05:30:00.000Z');
  });
});
