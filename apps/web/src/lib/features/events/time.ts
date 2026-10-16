// 시간대 계산 (D-50). 저장은 UTC 시각, 표시는 보는 사람의 시간대, 입력한 벽시계 시각은 일정에 지정한 시간대로 읽는다.
// 별도 시간대 라이브러리 없이 브라우저의 Intl(IANA 시간대 데이터)을 쓴다.

export function localTimeZone(): string {
  return Intl.DateTimeFormat().resolvedOptions().timeZone;
}

/** 브라우저가 아는 시간대 이름인지(입력 확인용). */
export function isKnownZone(timeZone: string): boolean {
  try {
    new Intl.DateTimeFormat('en-US', { timeZone });
    return true;
  } catch {
    return false;
  }
}

interface Parts {
  year: number;
  month: number;
  day: number;
  hour: number;
  minute: number;
}

function partsIn(instant: Date, timeZone: string): Parts {
  // 브라우저가 모르는 시간대 이름(서버에는 있을 수 있음)이면 화면이 깨지지 않게 UTC로 보인다.
  const zone = isKnownZone(timeZone) ? timeZone : 'UTC';
  const fmt = new Intl.DateTimeFormat('en-US', {
    timeZone: zone,
    hourCycle: 'h23',
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  });
  const parts = fmt.formatToParts(instant);
  const get = (type: Intl.DateTimeFormatPartTypes): number =>
    Number(parts.find((p) => p.type === type)?.value ?? '0');
  return {
    year: get('year'),
    month: get('month'),
    day: get('day'),
    hour: get('hour'),
    minute: get('minute'),
  };
}

const pad = (n: number): string => String(n).padStart(2, '0');

/** UTC 시각을 그 시간대의 벽시계 시각(`YYYY-MM-DDTHH:mm`)으로. */
export function wallTime(iso: string, timeZone: string): string {
  const p = partsIn(new Date(iso), timeZone);
  return `${String(p.year)}-${pad(p.month)}-${pad(p.day)}T${pad(p.hour)}:${pad(p.minute)}`;
}

/** UTC 시각이 그 시간대에서 속한 날짜(`YYYY-MM-DD`). */
export function dayIn(iso: string, timeZone: string): string {
  return wallTime(iso, timeZone).slice(0, 10);
}

/** 그 시각의 UTC와의 차이(밀리초). */
function offsetAt(utcMs: number, timeZone: string): number {
  const p = partsIn(new Date(utcMs), timeZone);
  return (
    Date.UTC(p.year, p.month - 1, p.day, p.hour, p.minute) - Math.floor(utcMs / 60_000) * 60_000
  );
}

/**
 * 그 시간대의 벽시계 시각(`YYYY-MM-DDTHH:mm`)을 UTC 시각(ISO)으로. 두 번 있는 시각(여름시간 끝)은 앞의 것,
 * 없는 시각(여름시간 시작)은 그 틈만큼 뒤로 민다. 모르는 시간대면 RangeError.
 */
export function instantOf(wall: string, timeZone: string): string {
  if (!isKnownZone(timeZone)) throw new RangeError(`unknown time zone ${timeZone}`);
  const [date = '', time = '00:00'] = wall.split('T');
  const [y = 1970, mo = 1, d = 1] = date.split('-').map(Number);
  const [h = 0, mi = 0] = time.split(':').map(Number);
  const target = Date.UTC(y, mo - 1, d, h, mi);
  // 앞뒤 하루의 차이 두 가지가 후보다. 그 시각으로 돌아오는 것 중 이른 것을 고른다.
  const before = target - offsetAt(target - 86_400_000, timeZone);
  const after = target - offsetAt(target + 86_400_000, timeZone);
  const candidates = [before, after].sort((a, b) => a - b);
  for (const c of candidates) {
    if (wallTime(new Date(c).toISOString(), timeZone) === wall) return new Date(c).toISOString();
  }
  // 없는 시각: 틈 앞의 차이를 쓰면 틈만큼 뒤의 벽시계 시각이 된다.
  return new Date(Math.max(before, after)).toISOString();
}

/** 표시용 시각 `HH:mm`(보는 사람의 시간대). */
export function clock(iso: string, timeZone: string): string {
  return wallTime(iso, timeZone).slice(11);
}
