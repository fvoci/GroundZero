import { type EventView } from '$lib/features/events/model';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import type { CalendarTaskView, EventOccurrenceView } from '$lib/api/client';
import { entriesByDay, monthRange, shiftMonth } from '$lib/features/events/calendar';
import { dayIn, instantOf, instantsOf, offsetLabel, wallTime } from '$lib/features/events/time';
import { me } from '$lib/me.svelte';

// 이 파일의 문구·시각은 한국어·24시간 표시로 본다(D-81 표시 설정).
beforeEach(() => {
  Object.assign(me, { locale: 'ko', hourCycle: 'h23' });
});
afterEach(() => {
  Object.assign(me, { locale: '', hourCycle: '' });
});

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
  recurrence: null,
  excluded_dates: [],
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
    // 없는 시각은 뒤로, 두 번 있는 시각은 앞의 것.
    expect(wallTime(instantOf('2026-03-08T02:30', 'America/New_York'), 'America/New_York')).toBe(
      '2026-03-08T03:30',
    );
    expect(wallTime(instantOf('2026-09-06T00:00', 'America/Santiago'), 'America/Santiago')).toBe(
      '2026-09-06T01:00',
    );
    expect(instantOf('2026-11-01T01:30', 'America/New_York')).toBe('2026-11-01T05:30:00.000Z');
    expect(wallTime(instantOf('2026-10-17T09:00', 'Asia/Kathmandu'), 'Asia/Kathmandu')).toBe(
      '2026-10-17T09:00',
    );
    // 모르는 시간대: 입력은 거부하고 표시는 UTC로.
    expect(() => instantOf('2026-10-17T09:00', 'Factory/Nowhere')).toThrow(RangeError);
    expect(wallTime('2026-10-17T00:00:00Z', 'Factory/Nowhere')).toBe('2026-10-17T00:00');
    // 뉴욕 2026-11-01 01:30은 두 번 있다(첫 번째를 쓴다). 2026-03-08 02:30은 없다(뒤로 민다).
    expect(wallTime(instantOf('2026-11-01T09:00', 'America/New_York'), 'America/New_York')).toBe(
      '2026-11-01T09:00',
    );
    expect(wallTime(instantOf('2026-03-08T12:00', 'America/New_York'), 'America/New_York')).toBe(
      '2026-03-08T12:00',
    );
  });

  it('lists every instant of a wall time and labels its UTC offset (D-93)', () => {
    // 없는 시각은 0개, 보통은 1개, 두 번 있는 시각은 이른 것부터 2개.
    expect(instantsOf('2026-03-08T02:30', 'America/New_York')).toEqual([]);
    expect(instantsOf('2026-11-01T09:00', 'America/New_York')).toEqual([
      '2026-11-01T14:00:00.000Z',
    ]);
    expect(instantsOf('2026-11-01T01:30', 'America/New_York')).toEqual([
      '2026-11-01T05:30:00.000Z',
      '2026-11-01T06:30:00.000Z',
    ]);
    // 오스트레일리아 로드하우섬은 30분만 되돌린다(01:30~01:59가 두 번).
    expect(instantsOf('2026-04-05T01:45', 'Australia/Lord_Howe')).toHaveLength(2);
    expect(() => instantsOf('2026-10-17T09:00', 'Factory/Nowhere')).toThrow(RangeError);
    // 빼기는 U+2212, 더하기·0은 '+'.
    expect(offsetLabel('2026-11-01T05:30:00Z', 'America/New_York')).toBe('UTC−04:00');
    expect(offsetLabel('2026-11-01T06:30:00Z', 'America/New_York')).toBe('UTC−05:00');
    expect(offsetLabel('2026-11-01T06:30:00Z', 'Asia/Seoul')).toBe('UTC+09:00');
    expect(offsetLabel('2026-11-01T06:30:00Z', 'Asia/Kathmandu')).toBe('UTC+05:45');
    expect(offsetLabel('2026-11-01T06:30:00Z', 'UTC')).toBe('UTC+00:00');
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

  it('places events and tasks on every covered day within the requested range', () => {
    const task: CalendarTaskView = {
      id: 't',
      item_id: 'i',
      reference: 'DEV-2',
      title: '보고서',
      status: 'todo',
      status_category: 'todo',
      milestone: false,
      start_date: '2026-10-28',
      due_date: '2026-11-03',
    };
    const occ = (id: string, over: Partial<EventOccurrenceView>): EventOccurrenceView => ({
      event_id: id,
      occurrence: '2026-11-01',
      start_date: null,
      end_date: null,
      starts_at: null,
      ends_at: null,
      skipped: false,
      ...over,
    });
    const map = entriesByDay(
      [task],
      [
        {
          series_id: 's',
          root_task_id: 'r',
          item_id: 'i',
          title: '주간 보고',
          milestone: false,
          occurrence: '2026-11-04',
          start_date: null,
          due_date: '2026-11-04',
        },
      ],
      [
        event({ id: 'a', all_day: true, title: '학회' }),
        event({ id: 'b', title: '점검' }),
        event({ id: 'c', title: '야간 회의' }),
      ],
      [
        occ('a', { start_date: '2026-11-02', end_date: '2026-11-03' }),
        occ('b', { starts_at: '2026-10-31T15:30:00Z', ends_at: '2026-10-31T16:00:00Z' }),
        // 서울 11-05 23:00~24:00: 다음 날에 걸치지 않는다.
        occ('c', { starts_at: '2026-11-05T14:00:00Z', ends_at: '2026-11-05T15:00:00Z' }),
      ],
      '2026-11-01',
      '2026-11-30',
      'Asia/Seoul',
    );
    expect(map.get('2026-11-01')?.map((e) => e.label)).toEqual([
      '00:30 점검',
      '기간 중 DEV-2 보고서',
    ]);
    expect(map.get('2026-11-02')?.map((e) => e.label)).toEqual([
      '종일 학회',
      '기간 중 DEV-2 보고서',
    ]);
    expect(map.get('2026-11-03')?.map((e) => e.label)).toEqual(['종일 학회', '마감 DEV-2 보고서']);
    expect(map.has('2026-10-28')).toBe(false);
    expect(map.get('2026-11-04')?.map((e) => e.label)).toEqual(['예정 주간 보고']);
    expect(map.get('2026-11-05')?.map((e) => e.label)).toEqual(['23:00 야간 회의']);
    expect(map.has('2026-11-06')).toBe(false);
  });
});

describe('task calendar date boundaries (D-121)', () => {
  const task = (over: Partial<CalendarTaskView>): CalendarTaskView => ({
    id: 't',
    item_id: 'i',
    reference: 'DEV-2',
    title: '보고서',
    status: '검토',
    status_category: 'doing',
    milestone: false,
    start_date: null,
    due_date: null,
    ...over,
  });
  const entries = (tasks: CalendarTaskView[], from = '2026-11-01', to = '2026-11-30') =>
    entriesByDay(tasks, [], [], [], from, to, 'Asia/Seoul');

  it.each(['start_date', 'due_date'] as const)(
    'clips a single %s to the inclusive requested range',
    (field) => {
      const map = entries(
        ['2026-10-31', '2026-11-01', '2026-11-30', '2026-12-01'].map((day) =>
          task({ id: day, [field]: day }),
        ),
      );
      expect([...map.keys()]).toEqual(['2026-11-01', '2026-11-30']);
    },
  );

  it('labels a period by its start, intermediate dates and due date', () => {
    const map = entries([task({ start_date: '2026-11-10', due_date: '2026-11-12' })]);
    expect([...map.values()].map((list) => list.map((entry) => entry.label))).toEqual([
      ['시작 DEV-2 보고서'],
      ['기간 중 DEV-2 보고서'],
      ['마감 DEV-2 보고서'],
    ]);
  });

  it('preserves a same-day deadline and milestone labels', () => {
    const map = entries([
      task({ start_date: '2026-11-10', due_date: '2026-11-10' }),
      task({ id: 'milestone', milestone: true, due_date: '2026-11-11' }),
    ]);
    expect(map.get('2026-11-10')?.map((entry) => entry.label)).toEqual(['마감 DEV-2 보고서']);
    expect(map.get('2026-11-11')?.map((entry) => entry.label)).toEqual(['◆ DEV-2 보고서']);
  });
});

describe('calendar date sources (D-80)', () => {
  it('places plan and record dates on their day and marks personal plans', () => {
    const map = entriesByDay([], [], [], [], '2026-11-01', '2026-11-30', 'Asia/Seoul', [
      {
        kind: 'episode',
        id: 'ep',
        item_id: 'c',
        title: '2. 행렬',
        date: '2026-11-10',
        personal: true,
        done: false,
      },
      {
        kind: 'revision_due',
        id: 'r',
        item_id: 'p',
        title: '학회 투고',
        date: '2026-11-12',
        personal: false,
        done: false,
      },
      {
        kind: 'deliverable',
        id: 'd',
        item_id: 'p',
        title: '밖',
        date: '2026-12-01',
        personal: false,
        done: true,
      },
    ]);
    expect(map.get('2026-11-10')?.map((e) => e.label)).toEqual(['수강(내 계획) 2. 행렬']);
    expect(map.get('2026-11-12')?.map((e) => e.label)).toEqual(['수정 마감 학회 투고']);
    expect(map.has('2026-12-01')).toBe(false);
  });
});

describe('skipped repeating occurrences (D-81)', () => {
  it('shows an occurrence whose local time does not exist as skipped on its date', () => {
    const map = entriesByDay(
      [],
      [],
      [
        event({
          id: 'e',
          title: '새벽 점검',
          starts_at: '2026-03-01T07:30:00Z',
          ends_at: '2026-03-01T08:00:00Z',
          timezone: 'America/New_York',
        }),
      ],
      [
        {
          event_id: 'e',
          occurrence: '2026-03-08',
          start_date: null,
          end_date: null,
          starts_at: null,
          ends_at: null,
          skipped: true,
        },
      ],
      '2026-03-01',
      '2026-03-31',
      'America/New_York',
    );
    expect(map.get('2026-03-08')?.map((e) => e.label)).toEqual(['건너뜀(그 시각 없음) 새벽 점검']);
  });

  it('flags wall times that do not exist in the zone', async () => {
    const { isMissingLocalTime } = await import('$lib/features/events/time');
    expect(isMissingLocalTime('2026-03-08T02:30', 'America/New_York')).toBe(true);
    expect(isMissingLocalTime('2026-11-01T01:30', 'America/New_York')).toBe(false);
    expect(isMissingLocalTime('2026-03-08T02:30', 'Asia/Seoul')).toBe(false);
  });
});
