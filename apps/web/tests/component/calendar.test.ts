import { describe, expect, it } from 'vitest';

import type { CalendarTaskView, EventView } from '$lib/api/client';
import { entriesByDay, monthRange, shiftMonth } from '$lib/features/events/calendar';
import { dayIn, instantOf, wallTime } from '$lib/features/events/time';

const event = (over: Partial<EventView>): EventView => ({
  id: 'e',
  item_id: 'i',
  workspace_id: 'w',
  kind: 'meeting',
  title: '회의',
  description: '',
  all_day: false,
  start_date: null,
  end_date: null,
  starts_at: null,
  ends_at: null,
  timezone: 'Asia/Seoul',
  version: 1,
  access: 'write',
  ...over,
});

describe('time zones (D-50)', () => {
  it('reads wall times in the event zone and shows them in the viewer zone', () => {
    expect(instantOf('2026-11-01T00:30', 'Asia/Seoul')).toBe('2026-10-31T15:30:00.000Z');
    expect(dayIn('2026-10-31T15:30:00Z', 'Asia/Seoul')).toBe('2026-11-01');
    expect(dayIn('2026-10-31T15:30:00Z', 'UTC')).toBe('2026-10-31');
    expect(wallTime('2026-10-31T15:30:00Z', 'America/New_York')).toBe('2026-10-31T11:30');
  });

  it('handles daylight saving transitions', () => {
    // 뉴욕 2026-11-01 01:30은 두 번 있다(첫 번째를 쓴다). 2026-03-08 02:30은 없다(뒤로 민다).
    expect(wallTime(instantOf('2026-11-01T09:00', 'America/New_York'), 'America/New_York')).toBe(
      '2026-11-01T09:00',
    );
    expect(wallTime(instantOf('2026-03-08T12:00', 'America/New_York'), 'America/New_York')).toBe(
      '2026-03-08T12:00',
    );
  });
});

describe('month calendar', () => {
  it('spans six weeks from the Sunday before the first', () => {
    const range = monthRange('2026-11');
    expect(range.from).toBe('2026-11-01');
    expect(range.days).toHaveLength(42);
    expect(monthRange('2026-10').from).toBe('2026-09-27');
    expect(shiftMonth('2026-12', 1)).toBe('2027-01');
    expect(shiftMonth('2026-01', -1)).toBe('2025-12');
  });

  it('places events on every day they cover and tasks on their due day', () => {
    const task: CalendarTaskView = {
      id: 't',
      item_id: 'i',
      reference: 'DEV-2',
      title: '보고서',
      status: 'todo',
      milestone: false,
      start_date: '2026-10-28',
      due_date: '2026-11-03',
    };
    const map = entriesByDay(
      [task],
      [
        event({
          id: 'a',
          all_day: true,
          start_date: '2026-11-02',
          end_date: '2026-11-03',
          title: '학회',
        }),
        event({
          id: 'b',
          starts_at: '2026-10-31T15:30:00Z',
          ends_at: '2026-10-31T16:00:00Z',
          title: '점검',
        }),
      ],
      '2026-11-01',
      '2026-11-30',
      'Asia/Seoul',
    );
    expect(map.get('2026-11-01')?.map((e) => e.label)).toEqual(['00:30 점검']);
    expect(map.get('2026-11-02')?.map((e) => e.label)).toEqual(['종일 학회']);
    expect(map.get('2026-11-03')?.map((e) => e.label)).toEqual(['종일 학회', '마감 DEV-2 보고서']);
    expect(map.has('2026-10-28')).toBe(false);
  });
});
