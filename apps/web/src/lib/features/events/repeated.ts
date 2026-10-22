// 이미 저장된 일정·시간 기록을 고칠 때 고친 시각 읽기(D-93). 손대지 않은 끝은 부르는 쪽이 저장된 시점을 그대로 보내고,
// 고친 끝만 여기서 읽는다. 없는 시각(봄)은 거절하고, 두 번 있는 시각(가을 여름시간 끝)은 앞의 것으로 정하지 않고(그건
// 새로 만들 때의 규칙, D-81) 사람이 고르게 한다. 고르는 항목에는 순서와 UTC 차이를 함께 보인다.
import { displaySettings, formatClock } from '$lib/display';
import { m } from '$lib/paraglide/messages.js';

import { instantsOf, isKnownZone, offsetLabel } from './time';

export interface RepeatedOption {
  /** 그 항목을 고르면 보낼 UTC 시각(ISO). */
  instant: string;
  /** `첫 번째 01:30 (UTC−04:00)`처럼 순서·시각·UTC 차이. */
  label: string;
}

export type EditedWall =
  { kind: 'instant'; instant: string } | { kind: 'missing' } | { kind: 'unchosen' };

const WALL = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}$/;

/**
 * 고친 벽시계 시각이 그 시간대에서 두 번 있으면 고를 두 항목(이른 것이 첫 번째), 아니면 빈 목록. 입력하는 중의 값(빈 값,
 * 다 쓰지 않은 시간대 이름)으로 화면을 그릴 때도 부르므로 예외를 내지 않는다.
 */
export function repeatedOptions(wall: string, timeZone: string): RepeatedOption[] {
  if (!WALL.test(wall) || !isKnownZone(timeZone)) return [];
  const found = instantsOf(wall, timeZone);
  if (found.length !== 2) return [];
  // 시각은 사용자의 12·24시간 표시(D-81)로, 그 일정의 시간대 기준으로 보인다.
  const settings = { ...displaySettings(), timeZone };
  return found.map((instant, index) => {
    const values = { time: formatClock(instant, settings), offset: offsetLabel(instant, timeZone) };
    return {
      instant,
      label:
        index === 0 ? m.events_repeated_time_first(values) : m.events_repeated_time_second(values),
    };
  });
}

/**
 * 고친 끝의 벽시계 시각을 읽는다: 없는 시각이면 `missing`, 두 번 있는 시각이면 고른 시각(고르지 않았거나 지금 후보에
 * 없으면 `unchosen`), 보통은 그 시각. 모르는 시간대면 RangeError.
 */
export function readEditedWall(
  wall: string,
  timeZone: string,
  chosen: string | undefined,
): EditedWall {
  const found = instantsOf(wall, timeZone);
  const [first] = found;
  if (first === undefined) return { kind: 'missing' };
  if (found.length === 1) return { kind: 'instant', instant: first };
  return chosen !== undefined && found.includes(chosen)
    ? { kind: 'instant', instant: chosen }
    : { kind: 'unchosen' };
}

/** 입력란의 `aria-describedby`: 지금 보이는 안내·오류의 id만 잇는다(없으면 속성을 두지 않는다). */
export function describedBy(...ids: (string | false)[]): string | undefined {
  const shown = ids.flatMap((id) => (id === false ? [] : [id]));
  return shown.length === 0 ? undefined : shown.join(' ');
}
