// 시간 기록 표시 (D-32).
import type { TimeEntryView, TimeKind } from '$lib/api/client';

export const TIME_KIND_LABEL: Record<TimeKind, string> = {
  work: '작업',
  study: '순공부',
  lecture: '강의 시청',
  hobby: '취미',
  rest: '휴식',
  other: '기타',
};

/** 타이머 표시 `H:MM:SS`. */
export function clockText(seconds: number): string {
  const s = Math.max(0, Math.floor(seconds));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  return `${String(h)}:${String(m).padStart(2, '0')}:${String(s % 60).padStart(2, '0')}`;
}

/** 합계 표시 `3시간 5분`(1분 미만은 `0분`). */
export function durationText(seconds: number): string {
  const minutes = Math.floor(Math.max(0, seconds) / 60);
  const h = Math.floor(minutes / 60);
  const m = minutes % 60;
  if (h === 0) return `${String(m)}분`;
  return m === 0 ? `${String(h)}시간` : `${String(h)}시간 ${String(m)}분`;
}

/**
 * 지금 시각 기준의 길이(초). 서버가 응답한 시각의 `seconds`에, 진행 중이고 멈추지 않았으면 그 뒤로 흐른 시간을
 * 더한다(앱을 닫았다 열어도 서버 시각 기준이라 이어진다).
 */
export function liveSeconds(entry: TimeEntryView, fetchedAt: number, now: number): number {
  if (entry.ended_at !== null || entry.paused_at !== null) return entry.seconds;
  return entry.seconds + Math.max(0, Math.floor((now - fetchedAt) / 1000));
}
