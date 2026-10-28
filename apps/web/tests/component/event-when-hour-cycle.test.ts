import { type EventView } from '$lib/features/events/model';
import { afterEach, describe, expect, it } from 'vitest';

import { formatDay } from '$lib/display';
import { describeWhen } from '$lib/features/events/labels';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';

// 실제 브라우저 확인: 일정 창·항목 일정 목록·충돌 안내의 시각(describeWhen)이 12시간 표시를 고른 사람에게도
// `14:30–14:55`로 보였다. 달력 칸(calendar.ts)은 `3:30 PM`이다. 시각 표시(D-81 (2))는 한 서식 함수(D-81 (5))를 따라야 한다.

afterEach(async () => {
  Object.assign(me, { locale: '', hourCycle: '' });
  await setLocale('ko', { reload: false });
});

async function use(locale: 'ko' | 'en', hourCycle: 'h12' | 'h23'): Promise<void> {
  Object.assign(me, { locale, hourCycle });
  await setLocale(locale, { reload: false });
}

// ICU 판에 따라 오전·오후 앞 공백이 U+202F일 수 있어 보통 공백으로 맞춰 비교한다.
const plain = (text: string): string => text.replace(/\s/gu, ' ');

const SEOUL = 'Asia/Seoul';
const NEW_YORK = 'America/New_York';

// 서울 2026-11-02 15:30–15:55, 뉴욕(11월 1일에 여름시간 끝, UTC-5) 같은 날 01:30.
const timed = (over: Partial<EventView> = {}): EventView => ({
  id: 'e1',
  item_id: 'i1',
  workspace_id: 'ws-1',
  kind: 'meeting',
  title: 'Standup',
  description: '',
  all_day: false,
  start_date: null,
  end_date: null,
  starts_at: '2026-11-02T06:30:00Z',
  ends_at: '2026-11-02T06:55:00Z',
  timezone: SEOUL,
  recurrence: null,
  excluded_dates: [],
  version: 1,
  access: 'write',
  ...over,
});

const allDay = (over: Partial<EventView> = {}): EventView =>
  timed({
    all_day: true,
    start_date: '2026-11-02',
    end_date: '2026-11-02',
    starts_at: null,
    ends_at: null,
    ...over,
  });

const day = formatDay;

describe('event summaries follow the hour-cycle setting (D-81)', () => {
  it('uses the 12-hour clock in English when chosen', async () => {
    await use('en', 'h12');
    expect(plain(describeWhen(timed(), SEOUL))).toBe(`${day('2026-11-02')} 3:30 PM–3:55 PM`);
    expect(plain(describeWhen(timed({ ends_at: '2026-11-03T06:55:00Z' }), SEOUL))).toBe(
      `${day('2026-11-02')} 3:30 PM ~ ${day('2026-11-03')} 3:55 PM`,
    );
    // 일정 시간대 기준 시각도 같은 시각 표시로.
    expect(plain(describeWhen(timed({ timezone: NEW_YORK }), SEOUL))).toBe(
      m.events_when_in_zone({
        span: `${day('2026-11-02')} 3:30 PM–3:55 PM`,
        zone: NEW_YORK,
        time: `${day('2026-11-02')} 1:30 AM`,
      }),
    );
  });

  it('uses the 24-hour clock in Korean when chosen', async () => {
    await use('ko', 'h23');
    expect(plain(describeWhen(timed(), SEOUL))).toBe(`${day('2026-11-02')} 15:30–15:55`);
    expect(plain(describeWhen(timed({ timezone: NEW_YORK }), SEOUL))).toBe(
      m.events_when_in_zone({
        span: `${day('2026-11-02')} 15:30–15:55`,
        zone: NEW_YORK,
        time: `${day('2026-11-02')} 01:30`,
      }),
    );
  });

  it('follows the setting rather than the language convention', async () => {
    await use('ko', 'h12');
    expect(plain(describeWhen(timed(), SEOUL))).toBe(`${day('2026-11-02')} 오후 3:30–오후 3:55`);
    await use('en', 'h23');
    expect(plain(describeWhen(timed({ timezone: NEW_YORK }), SEOUL))).toBe(
      m.events_when_in_zone({
        span: `${day('2026-11-02')} 15:30–15:55`,
        zone: NEW_YORK,
        time: `${day('2026-11-02')} 01:30`,
      }),
    );
  });

  // 브라우저가 모르는 일정 시간대는 날짜처럼 UTC로 보인다(화면이 깨지지 않게).
  it('shows an unknown event zone in UTC', async () => {
    await use('en', 'h12');
    expect(plain(describeWhen(timed({ timezone: 'Mars/Olympus' }), SEOUL))).toBe(
      m.events_when_in_zone({
        span: `${day('2026-11-02')} 3:30 PM–3:55 PM`,
        zone: 'Mars/Olympus',
        time: `${day('2026-11-02')} 6:30 AM`,
      }),
    );
  });

  it('leaves all-day events unchanged', async () => {
    for (const [locale, hourCycle] of [
      ['en', 'h12'],
      ['ko', 'h23'],
    ] as const) {
      await use(locale, hourCycle);
      expect(describeWhen(allDay(), SEOUL)).toBe(
        m.events_when_all_day({ date: day('2026-11-02') }),
      );
      expect(describeWhen(allDay({ end_date: '2026-11-04' }), SEOUL)).toBe(
        m.events_when_all_day_range({ start: day('2026-11-02'), end: day('2026-11-04') }),
      );
    }
  });
});
