// 일정 충돌 뒤 "내 입력으로 다시 저장"의 합치기(D-94). 입력을 연 때의 일정(`base`)과 비교해 내가 고친 칸만 내 값으로,
// 나머지는 서버 최신 값으로 보낸다. 같은 칸을 서버에서도 다른 값으로 바꿨으면 겹친 칸으로 돌려주고, 화면이 양쪽 값을
// 보여 덮어쓸지 묻는다. 날짜·시각·시간대·종일 여부는 한 묶음(`when`)이다(시작만 내 것, 끝은 서버 것처럼 섞이지 않게).
import type { EventInput } from '$lib/api/client';

/** 합치는 칸. `when`은 종일 여부·날짜·시각·시간대 묶음. */
export const EVENT_UNITS = ['kind', 'title', 'description', 'when'] as const;
export type EventUnit = (typeof EVENT_UNITS)[number];

export interface MergedInput {
  /** 보낼 입력: 내가 고친 칸은 내 값, 나머지는 서버 값. 겹친 칸도 내 값이다(덮어쓰기를 확인받은 뒤에만 보낸다). */
  input: EventInput;
  /** 양쪽이 서로 다른 값으로 고친 칸. */
  clashes: EventUnit[];
}

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
  return Number.isNaN(x) || Number.isNaN(y) ? a === b : x === y;
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
function inputOf(e: EventInput): EventInput {
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

/**
 * 서버 최신 일정(`server`)에 내 입력(`mine`)을 합친다. `base`는 입력을 연 때의 일정이다. 고치지 않은 칸은 서버 값,
 * 고친 칸은 서버가 그대로 두었거나 같은 값으로 바꿨으면 내 값, 서로 다른 값으로 바꿨으면 겹친 칸이다.
 */
export function mergeEventInput(
  base: EventInput,
  mine: EventInput,
  server: EventInput,
): MergedInput {
  let input = inputOf(server);
  const clashes: EventUnit[] = [];
  for (const unit of EVENT_UNITS) {
    if (sameUnit(unit, mine, base)) continue;
    input = take(unit, input, mine);
    if (!sameUnit(unit, server, base) && !sameUnit(unit, server, mine)) clashes.push(unit);
  }
  return { input, clashes };
}
