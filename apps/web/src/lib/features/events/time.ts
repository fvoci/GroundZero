// 시간대 계산 (D-50). 저장은 UTC 시각, 표시는 보는 사람의 시간대, 입력한 벽시계 시각은 일정에 지정한 시간대로 읽는다.
// 별도 시간대 라이브러리 없이 브라우저의 Intl(IANA 시간대 데이터)을 쓴다.

export function localTimeZone(): string {
  return Intl.DateTimeFormat().resolvedOptions().timeZone;
}

interface Parts {
  year: number;
  month: number;
  day: number;
  hour: number;
  minute: number;
}

function partsIn(instant: Date, timeZone: string): Parts {
  const fmt = new Intl.DateTimeFormat('en-US', {
    timeZone,
    hourCycle: 'h23',
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  });
  const get = (type: Intl.DateTimeFormatPartTypes): number =>
    Number(fmt.formatToParts(instant).find((p) => p.type === type)?.value ?? '0');
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

/** 그 시간대의 벽시계 시각(`YYYY-MM-DDTHH:mm`)을 UTC 시각(ISO)으로. 여름시간으로 없는 시각은 뒤로 민다. */
export function instantOf(wall: string, timeZone: string): string {
  const [date = '', time = '00:00'] = wall.split('T');
  const [y = 1970, mo = 1, d = 1] = date.split('-').map(Number);
  const [h = 0, mi = 0] = time.split(':').map(Number);
  const target = Date.UTC(y, mo - 1, d, h, mi);
  let guess = target;
  // 두 번 고치면 여름시간 경계에서도 맞는다.
  for (let i = 0; i < 2; i += 1) {
    const p = partsIn(new Date(guess), timeZone);
    const seen = Date.UTC(p.year, p.month - 1, p.day, p.hour, p.minute);
    guess += target - seen;
  }
  return new Date(guess).toISOString();
}

/** 표시용 시각 `HH:mm`(보는 사람의 시간대). */
export function clock(iso: string, timeZone: string): string {
  return wallTime(iso, timeZone).slice(11);
}
