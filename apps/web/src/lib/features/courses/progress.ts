// 강의 진도 계산(D-37). 회차 목록과 내 진도에서 화면에 보일 숫자를 만든다. 날짜는 'YYYY-MM-DD' 글자로 비교한다.
import type { EpisodeView } from '$lib/api/client';

export interface CourseProgress {
  total: number;
  done: number;
  inProgress: number;
  remaining: number;
  /** 회차 수 기준 진행률(0~100). */
  percentByCount: number;
  totalSeconds: number;
  doneSeconds: number;
  /** 영상 길이 기준 진행률(0~100). 길이를 모르면 회차 수 기준과 같다. */
  percentByDuration: number;
  remainingSeconds: number;
  todayPlanned: number;
  todayDone: number;
  /** 계획일이 지났는데 완료하지 않은 회차(지연). */
  overdue: EpisodeView[];
  /** 목표일까지 남은 날(오늘 포함). 목표일이 없거나 지났으면 null. */
  daysLeft: number | null;
  /** 목표일까지 하루에 필요한 회차 수·영상 길이(초). */
  perDayEpisodes: number | null;
  perDaySeconds: number | null;
}

/** 이 회차의 계획일: 내 날짜가 공통 계획일보다 앞선다. */
export function plannedOn(e: EpisodeView): string | null {
  return e.mine.planned_on ?? e.planned_on;
}

function daysBetween(from: string, to: string): number {
  return Math.round((Date.parse(`${to}T00:00:00Z`) - Date.parse(`${from}T00:00:00Z`)) / 86_400_000);
}

export function courseProgress(
  episodes: readonly EpisodeView[],
  today: string,
  targetOn: string | null,
): CourseProgress {
  const total = episodes.length;
  const done = episodes.filter((e) => e.mine.status === 'done');
  const inProgress = episodes.filter((e) => e.mine.status === 'in_progress').length;
  const totalSeconds = episodes.reduce((s, e) => s + e.duration_seconds, 0);
  const doneSeconds = done.reduce((s, e) => s + e.duration_seconds, 0);
  const percentByCount = total === 0 ? 0 : Math.round((done.length / total) * 100);
  const percentByDuration =
    totalSeconds === 0 ? percentByCount : Math.round((doneSeconds / totalSeconds) * 100);
  const remaining = total - done.length;
  const remainingSeconds = totalSeconds - doneSeconds;
  const todayPlanned = episodes.filter((e) => plannedOn(e) === today).length;
  const todayDone = done.filter((e) => e.mine.completed_on === today).length;
  const overdue = episodes.filter((e) => {
    const planned = plannedOn(e);
    return e.mine.status !== 'done' && planned !== null && planned < today;
  });
  let daysLeft: number | null = null;
  if (targetOn !== null && targetOn >= today) daysLeft = daysBetween(today, targetOn) + 1;
  return {
    total,
    done: done.length,
    inProgress,
    remaining,
    percentByCount,
    totalSeconds,
    doneSeconds,
    percentByDuration,
    remainingSeconds,
    todayPlanned,
    todayDone,
    overdue,
    daysLeft,
    perDayEpisodes: daysLeft === null ? null : Math.ceil(remaining / daysLeft),
    perDaySeconds: daysLeft === null ? null : Math.ceil(remainingSeconds / daysLeft),
  };
}

/** 날짜까지 남은 날(지났으면 음수). 만료·마감 안내에 쓴다. */
export function daysUntil(today: string, date: string): number {
  return daysBetween(today, date);
}
