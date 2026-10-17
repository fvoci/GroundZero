// CSV 읽기(쉼표 또는 탭). 따옴표로 감싼 칸과 칸 안의 쉼표·따옴표("")·줄바꿈을 지원한다. 회차·업무 일괄 등록이 함께 쓴다.

export interface CsvRecord {
  /** 이 기록이 시작하는 줄 번호(1부터). */
  line: number;
  cells: string[];
}

/** 빈 줄은 건너뛴다. 맨 앞의 BOM은 지운다. 칸 앞뒤 공백은 걷어 낸다(따옴표 안의 줄바꿈은 남긴다). */
export function csvRecords(text: string): CsvRecord[] {
  const src = text.startsWith('﻿') ? text.slice(1) : text;
  const records: CsvRecord[] = [];
  let cells: string[] = [];
  let cell = '';
  let quoted = false;
  let line = 1;
  let start = 1;
  const endRecord = (): void => {
    cells.push(cell.trim());
    if (cells.some((c) => c !== '')) records.push({ line: start, cells });
    cells = [];
    cell = '';
  };
  for (let i = 0; i < src.length; i += 1) {
    const c = src[i] ?? '';
    if (quoted) {
      if (c === '"' && src[i + 1] === '"') {
        cell += '"';
        i += 1;
      } else if (c === '"') {
        quoted = false;
      } else {
        if (c === '\n') line += 1;
        if (c !== '\r') cell += c;
      }
    } else if (c === '"') {
      quoted = true;
    } else if (c === ',' || c === '\t') {
      cells.push(cell.trim());
      cell = '';
    } else if (c === '\n') {
      endRecord();
      line += 1;
      start = line;
    } else if (c !== '\r') {
      cell += c;
    }
  }
  endRecord();
  return records;
}

/** 첫 기록이 머리글(첫 칸이 제목/title)인지. */
export function isHeader(record: CsvRecord, index: number): boolean {
  return index === 0 && ['제목', 'title'].includes((record.cells[0] ?? '').toLowerCase());
}

/** 날짜 칸: 비었으면 null, 'YYYY-MM-DD'면 그대로, 아니면 undefined(오류). */
export function csvDate(raw: string): string | null | undefined {
  if (raw === '') return null;
  return /^\d{4}-\d{2}-\d{2}$/.test(raw) ? raw : undefined;
}
