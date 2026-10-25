// 표시 설정과 날짜·시각 서식(D-81, D-50). 사용자가 저장한 값(계정)이 먼저이고, 아직 고르지 않은 칸은 브라우저
// 언어·시간대와 그 언어의 로케일 관례로 제안한다. 브라우저 설정이 바뀌어도 저장한 값을 덮지 않는다. 표시만 바꾸며
// 저장된 시각·날짜는 그대로다. 서식은 브라우저 Intl을 그대로 쓰는 작은 함수들이다(별도 날짜 라이브러리 없음).
import { me } from '$lib/me.svelte';

/** 브라우저가 아는 IANA 시간대인지. 저장된 값 자체는 바꾸지 않는다. */
export function isKnownZone(timeZone: string): boolean {
  try {
    new Intl.DateTimeFormat('en-US', { timeZone });
    return true;
  } catch {
    return false;
  }
}

/** 알 수 없는 시간대의 표시 대체. 일정은 UTC, 개인 설정은 브라우저 시간대를 넘긴다. */
export function resolveTimeZone(timeZone: string, fallback = 'UTC'): string {
  if (isKnownZone(timeZone)) return timeZone;
  return isKnownZone(fallback) ? fallback : 'UTC';
}

export function localTimeZone(): string {
  return resolveTimeZone(me.timeZone, Intl.DateTimeFormat().resolvedOptions().timeZone);
}

export type DisplayLocale = 'ko' | 'en';
export type WeekStart = 'mon' | 'sun';
export type HourCycle = 'h12' | 'h23';

export interface DisplaySettings {
  locale: DisplayLocale;
  timeZone: string;
  weekStart: WeekStart;
  hourCycle: HourCycle;
}

const LOCALE_TAG: Record<DisplayLocale, string> = { ko: 'ko-KR', en: 'en-US' };

export function asLocale(value: string): DisplayLocale | undefined {
  return value === 'ko' || value === 'en' ? value : undefined;
}
export function asWeekStart(value: string): WeekStart | undefined {
  return value === 'mon' || value === 'sun' ? value : undefined;
}
export function asHourCycle(value: string): HourCycle | undefined {
  return value === 'h12' || value === 'h23' ? value : undefined;
}

/** 브라우저 언어에서 고른 표시 언어(한국어가 아니면 영어). */
export function browserLocale(languages: readonly string[] = navigator.languages): DisplayLocale {
  return languages.some((l) => l.toLowerCase().startsWith('ko')) ? 'ko' : 'en';
}

/** 그 언어의 관례적인 주 시작. 지원하는 두 로케일(ko-KR·en-US)은 CLDR 관례상 모두 일요일이다. */
const WEEK_START: Record<DisplayLocale, WeekStart> = { ko: 'sun', en: 'sun' };
export function conventionalWeekStart(locale: DisplayLocale): WeekStart {
  return WEEK_START[locale];
}

/** 그 언어의 관례적인 시각 표시. */
export function conventionalHourCycle(locale: DisplayLocale): HourCycle {
  const cycle = new Intl.DateTimeFormat(LOCALE_TAG[locale], { hour: 'numeric' }).resolvedOptions()
    .hourCycle;
  return cycle === 'h11' || cycle === 'h12' ? 'h12' : 'h23';
}

/** 저장한 값이 없을 때의 제안(처음 접속). */
export function suggestedSettings(): DisplaySettings {
  const locale = browserLocale();
  return {
    locale,
    timeZone: resolveTimeZone(Intl.DateTimeFormat().resolvedOptions().timeZone),
    weekStart: conventionalWeekStart(locale),
    hourCycle: conventionalHourCycle(locale),
  };
}

/** 지금 쓰는 설정: 칸마다 저장한 값, 없으면 제안. 주 시작·시각 표시는 고른 언어의 관례로 채운다. */
export function displaySettings(): DisplaySettings {
  const locale = asLocale(me.locale) ?? browserLocale();
  return {
    locale,
    timeZone: localTimeZone(),
    weekStart: asWeekStart(me.weekStart) ?? conventionalWeekStart(locale),
    hourCycle: asHourCycle(me.hourCycle) ?? conventionalHourCycle(locale),
  };
}

export function localeTag(settings: DisplaySettings = displaySettings()): string {
  return LOCALE_TAG[settings.locale];
}

/** 요일 이름 7개를 한 주 시작 순서로(짧은 이름). `index`는 일요일=0. */
export function weekdays(
  settings: DisplaySettings = displaySettings(),
): { index: number; short: string; long: string }[] {
  const order = settings.weekStart === 'mon' ? [1, 2, 3, 4, 5, 6, 0] : [0, 1, 2, 3, 4, 5, 6];
  const short = new Intl.DateTimeFormat(localeTag(settings), { weekday: 'short', timeZone: 'UTC' });
  const long = new Intl.DateTimeFormat(localeTag(settings), { weekday: 'long', timeZone: 'UTC' });
  // 1970-01-04는 일요일이다.
  return order.map((index) => {
    const day = new Date(Date.UTC(1970, 0, 4 + index));
    return { index, short: short.format(day), long: long.format(day) };
  });
}

/** 날짜만 있는 값(`YYYY-MM-DD`)을 그 날짜 그대로 보인다. 시간대 때문에 하루가 밀리지 않게 UTC로 읽는다. */
export function formatDay(
  ymd: string,
  options: Intl.DateTimeFormatOptions = { dateStyle: 'medium' },
  settings: DisplaySettings = displaySettings(),
): string {
  const [y = 1970, m = 1, d = 1] = ymd.split('-').map(Number);
  return new Intl.DateTimeFormat(localeTag(settings), { ...options, timeZone: 'UTC' }).format(
    new Date(Date.UTC(y, m - 1, d)),
  );
}

/** 달 제목(`YYYY-MM`). */
export function formatMonth(month: string, settings: DisplaySettings = displaySettings()): string {
  return formatDay(`${month}-01`, { year: 'numeric', month: 'long' }, settings);
}

/** 실제 시점(UTC ISO)을 내 시간대·시각 표시로. 공동 일정의 원래 시간대처럼 다른 시간대로 보일 때는 `timeZone`을 준다. */
export function formatInstant(
  iso: string,
  options: Intl.DateTimeFormatOptions = { dateStyle: 'medium', timeStyle: 'short' },
  settings: DisplaySettings = displaySettings(),
  timeZone: string = settings.timeZone,
): string {
  return new Intl.DateTimeFormat(localeTag(settings), {
    ...options,
    timeZone: resolveTimeZone(timeZone),
    hourCycle: settings.hourCycle,
  }).format(new Date(iso));
}

/** 시각만(`오후 3:05`·`15:05`). */
export function formatClock(iso: string, settings: DisplaySettings = displaySettings()): string {
  return formatInstant(iso, { hour: 'numeric', minute: '2-digit' }, settings);
}
