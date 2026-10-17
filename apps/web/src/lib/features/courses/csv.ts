// 회차 일괄 등록용 CSV 읽기(D-39). 열: 제목, 챕터, 길이, 계획일. 길이는 "1:02:03"·"12:30"(분:초)·"45"(분).
// 첫 줄이 머리글(제목/title)이면 건너뛴다. 따옴표로 감싼 칸과 칸 안의 쉼표·따옴표("")를 지원한다.

export interface EpisodeRow {
  title: string;
  chapter: string;
  duration_seconds: number;
  planned_on: string | null;
}

export interface CsvResult {
  rows: EpisodeRow[];
  /** 줄 번호(1부터)와 이유. */
  errors: { line: number; reason: string }[];
}

function splitLine(line: string): string[] {
  const cells: string[] = [];
  let cell = '';
  let quoted = false;
  for (let i = 0; i < line.length; i += 1) {
    const c = line[i];
    if (quoted) {
      if (c === '"' && line[i + 1] === '"') {
        cell += '"';
        i += 1;
      } else if (c === '"') {
        quoted = false;
      } else {
        cell += c ?? '';
      }
    } else if (c === '"') {
      quoted = true;
    } else if (c === ',' || c === '\t') {
      cells.push(cell.trim());
      cell = '';
    } else {
      cell += c ?? '';
    }
  }
  cells.push(cell.trim());
  return cells;
}

/** "1:02:03" → 3723, "12:30" → 750(분:초), "45" → 2700(분). 빈칸은 0. 알 수 없으면 null. */
export function parseDuration(raw: string): number | null {
  const value = raw.trim();
  if (value === '') return 0;
  if (!/^\d+(:\d{1,2}){0,2}$/.test(value)) return null;
  const parts = value.split(':').map(Number);
  if (parts.length === 1) return (parts[0] ?? 0) * 60;
  if (parts.length === 2) return (parts[0] ?? 0) * 60 + (parts[1] ?? 0);
  return (parts[0] ?? 0) * 3600 + (parts[1] ?? 0) * 60 + (parts[2] ?? 0);
}

export function parseEpisodesCsv(text: string): CsvResult {
  const rows: EpisodeRow[] = [];
  const errors: { line: number; reason: string }[] = [];
  const lines = text.split(/\r?\n/);
  lines.forEach((line, index) => {
    if (line.trim() === '') return;
    const [title = '', chapter = '', duration = '', planned = ''] = splitLine(line);
    if (index === 0 && ['제목', 'title'].includes(title.toLowerCase())) return;
    const lineNo = index + 1;
    if (title === '') {
      errors.push({ line: lineNo, reason: '제목이 없습니다' });
      return;
    }
    const seconds = parseDuration(duration);
    if (seconds === null) {
      errors.push({ line: lineNo, reason: `길이를 알 수 없습니다: ${duration}` });
      return;
    }
    if (planned !== '' && !/^\d{4}-\d{2}-\d{2}$/.test(planned)) {
      errors.push({ line: lineNo, reason: `날짜는 YYYY-MM-DD로 적어 주세요: ${planned}` });
      return;
    }
    rows.push({
      title,
      chapter,
      duration_seconds: seconds,
      planned_on: planned === '' ? null : planned,
    });
  });
  return { rows, errors };
}
