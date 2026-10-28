import type { TimeEntryView, TimeKind } from '$lib/features/time/model';
// 시간 기록 표시 (D-32).

import { m } from '$lib/paraglide/messages.js';

// 값은 읽을 때 지금 언어로 만든다(언어는 실행 중에 바뀔 수 있다).
export const TIME_KIND_LABEL: Record<TimeKind, string> = {
  get work() {
    return m.time_kind_work();
  },
  get study() {
    return m.time_kind_study();
  },
  get lecture() {
    return m.time_kind_lecture();
  },
  get hobby() {
    return m.time_kind_hobby();
  },
  get rest() {
    return m.time_kind_rest();
  },
  get other() {
    return m.time_kind_other();
  },
};

/** 타이머 표시 `H:MM:SS`. */
export function clockText(seconds: number): string {
  const s = Math.max(0, Math.floor(seconds));
  const h = Math.floor(s / 3600);
  const min = Math.floor((s % 3600) / 60);
  return `${String(h)}:${String(min).padStart(2, '0')}:${String(s % 60).padStart(2, '0')}`;
}

/** 합계 표시 `3시간 5분`(1분 미만은 `0분`). */
export function durationText(seconds: number): string {
  const minutes = Math.floor(Math.max(0, seconds) / 60);
  const h = Math.floor(minutes / 60);
  const rest = minutes % 60;
  if (h === 0) return m.time_duration_minutes({ minutes: rest });
  return rest === 0
    ? m.time_duration_hours({ hours: h })
    : m.time_duration_hours_minutes({ hours: h, minutes: rest });
}

/**
 * 지금 시각 기준의 길이(초). 서버가 응답한 시각의 `seconds`에, 진행 중이고 멈추지 않았으면 그 뒤로 흐른 시간을
 * 더한다(앱을 닫았다 열어도 서버 시각 기준이라 이어진다).
 */
export function liveSeconds(entry: TimeEntryView, fetchedAt: number, now: number): number {
  if (entry.ended_at !== null || entry.paused_at !== null) return entry.seconds;
  return entry.seconds + Math.max(0, Math.floor((now - fetchedAt) / 1000));
}
