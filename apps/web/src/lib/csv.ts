// CSV 한 줄 나누기(쉼표 또는 탭). 따옴표로 감싼 칸과 칸 안의 쉼표·따옴표("")를 지원한다. 회차·업무 일괄 등록이 함께 쓴다.

export function splitCsvLine(line: string): string[] {
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

/** 날짜 칸: 비었으면 null, 'YYYY-MM-DD'면 그대로, 아니면 undefined(오류). */
export function csvDate(raw: string): string | null | undefined {
  if (raw === '') return null;
  return /^\d{4}-\d{2}-\d{2}$/.test(raw) ? raw : undefined;
}
