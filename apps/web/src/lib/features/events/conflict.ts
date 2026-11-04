import { type EventInput } from '$lib/features/events/model';
import type { RecurrenceRule } from '$lib/features/tasks/model';
import type { ConflictField } from '$lib/features/common/field-conflict';
import { m } from '$lib/paraglide/messages.js';
import { KIND_LABEL, describeRule } from './labels';
// D-94의 공통 비교에 전달하는 일정 단위. 날짜·시각·시간대·종일 여부는 한 묶음이다.

/** 합치는 칸. `when`은 종일 여부·날짜·시각·시간대 묶음. */
export const EVENT_UNITS = ['kind', 'title', 'description', 'when'] as const;
export type EventUnit = (typeof EVENT_UNITS)[number];

const WHITE_SPACE = /\p{White_Space}/u;

/**
 * 서버가 저장할 때처럼 앞뒤 공백을 지운다. 서버(Rust `str::trim`)는 유니코드 White_Space만 지우는데 JS `trim()`은
 * U+FEFF도 지우고 U+0085는 두므로 쓰지 않는다(서버가 다르게 저장할 칸을 같다고 보거나 그 반대가 되지 않게).
 */
export function trimLikeServer(s: string): string {
  let start = 0;
  let end = s.length;
  while (start < end && WHITE_SPACE.test(s.charAt(start))) start += 1;
  while (end > start && WHITE_SPACE.test(s.charAt(end - 1))) end -= 1;
  return s.slice(start, end);
}

/** 같은 시각인지. 서버와 입력의 표기(`Z`·`+09:00`·소수 초)가 달라도 값으로 비교한다. */
function sameInstant(a: string | null, b: string | null): boolean {
  if (a === null || b === null) return a === b;
  const x = Date.parse(a);
  const y = Date.parse(b);
  if (Number.isNaN(x) || Number.isNaN(y)) return a === b;
  // Date.parse truncates below milliseconds; the server preserves RFC3339 nanoseconds.
  const fraction = (value: string): string =>
    (/\.(\d+)(?:Z|[+-]\d{2}:\d{2})$/i.exec(value)?.[1] ?? '').replace(/0+$/, '');
  return x === y && fraction(a) === fraction(b);
}

/**
 * 그 칸의 값이 같은지. 제목·설명은 서버가 앞뒤 공백을 지워 저장하므로 지운 값으로 비교한다(공백만 덧붙인 칸을
 * 내가 고친 칸으로 보아 서버 값을 되돌리거나 겹쳤다고 묻지 않게).
 */
export function sameUnit(unit: EventUnit, a: EventInput, b: EventInput): boolean {
  switch (unit) {
    case 'kind':
      return a.kind === b.kind;
    case 'title':
      return trimLikeServer(a.title) === trimLikeServer(b.title);
    case 'description':
      return trimLikeServer(a.description) === trimLikeServer(b.description);
    case 'when':
      return (
        a.all_day === b.all_day &&
        a.start_date === b.start_date &&
        a.end_date === b.end_date &&
        sameInstant(a.starts_at, b.starts_at) &&
        sameInstant(a.ends_at, b.ends_at) &&
        a.timezone === b.timezone
      );
  }
}

/** 그 칸만 `from`의 값으로 바꾼 입력. */
function take(unit: EventUnit, into: EventInput, from: EventInput): EventInput {
  switch (unit) {
    case 'kind':
      return { ...into, kind: from.kind };
    case 'title':
      return { ...into, title: from.title };
    case 'description':
      return { ...into, description: from.description };
    case 'when':
      return {
        ...into,
        all_day: from.all_day,
        start_date: from.start_date,
        end_date: from.end_date,
        starts_at: from.starts_at,
        ends_at: from.ends_at,
        timezone: from.timezone,
      };
  }
}

/** 입력 칸만 옮긴다(서버 일정의 id·버전·권한 같은 칸이 보내는 본문에 섞이지 않게). */
export function inputOf(e: EventInput): EventInput {
  return {
    kind: e.kind,
    title: e.title,
    description: e.description,
    all_day: e.all_day,
    start_date: e.start_date,
    end_date: e.end_date,
    starts_at: e.starts_at,
    ends_at: e.ends_at,
    timezone: e.timezone,
  };
}

export function eventConflictFields(): ConflictField<EventInput>[] {
  return EVENT_UNITS.map((key) => ({
    key,
    label:
      key === 'when'
        ? m.events_clash_when()
        : key === 'kind'
          ? m.events_kind()
          : key === 'title'
            ? m.events_title()
            : m.events_description(),
    same: (a, b) => sameUnit(key, a, b),
    text: (value) => {
      if (key === 'kind') return KIND_LABEL[value.kind];
      if (key !== 'when') return value[key];
      return `${value.all_day ? m.events_all_day() : ''}\n${value.start_date ?? value.starts_at ?? ''} → ${value.end_date ?? value.ends_at ?? ''}\n${value.timezone}`.trim();
    },
    copy: (target, source) => {
      Object.assign(target, take(key, target, source));
    },
  }));
}

export interface RuleValue {
  rule: RecurrenceRule | null;
}
export function sameRule(a: RecurrenceRule | null, b: RecurrenceRule | null): boolean {
  if (a === null || b === null) return a === b;
  return (
    a.freq === b.freq &&
    a.interval === b.interval &&
    a.until === b.until &&
    a.count === b.count &&
    [...new Set(a.weekdays)].sort().join(',') === [...new Set(b.weekdays)].sort().join(',')
  );
}
export function ruleConflictFields(): ConflictField<RuleValue>[] {
  return [
    {
      key: 'rule',
      label: m.events_recurrence(),
      same: (a, b) => sameRule(a.rule, b.rule),
      text: (value) => (value.rule === null ? m.events_freq_none() : describeRule(value.rule)),
      copy: (target, source) => {
        target.rule = source.rule;
      },
    },
  ];
}
