// 회차 일괄 등록용 CSV 읽기(D-39). 열: 제목, 챕터, 길이, 계획일. 길이는 "1:02:03"·"12:30"(분:초)·"45"(분).
// 첫 기록이 머리글(제목/title)이면 건너뛴다. 따옴표로 감싼 칸과 칸 안의 쉼표·따옴표("")·줄바꿈을 지원한다.

import { csvDate, csvRecords, isHeader } from '$lib/csv';
import { m } from '$lib/paraglide/messages.js';

/** 서버와 같은 한도: 제목 300자, 길이 100시간. */
const MAX_TITLE = 300;
const MAX_SECONDS = 360_000;

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
  csvRecords(text).forEach((record, index) => {
    if (isHeader(record, index)) return;
    const [title = '', chapter = '', duration = '', planned = ''] = record.cells;
    const lineNo = record.line;
    if (title === '') {
      errors.push({ line: lineNo, reason: m.courses_csv_title_missing() });
      return;
    }
    if (title.length > MAX_TITLE) {
      errors.push({
        line: lineNo,
        reason: m.courses_csv_title_too_long({ max: String(MAX_TITLE) }),
      });
      return;
    }
    const seconds = parseDuration(duration);
    if (seconds === null) {
      errors.push({ line: lineNo, reason: m.courses_csv_duration_invalid({ value: duration }) });
      return;
    }
    if (seconds > MAX_SECONDS) {
      errors.push({ line: lineNo, reason: m.courses_csv_duration_too_long({ value: duration }) });
      return;
    }
    const plannedOn = csvDate(planned);
    if (plannedOn === undefined) {
      errors.push({ line: lineNo, reason: m.courses_csv_date_invalid({ value: planned }) });
      return;
    }
    rows.push({
      title,
      chapter,
      duration_seconds: seconds,
      planned_on: plannedOn,
    });
  });
  return { rows, errors };
}
