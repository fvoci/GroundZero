// 업무 일괄 등록용 CSV 읽기(D-39). 열: 제목, 설명, 상태, 우선순위, 시작일, 기한, 예상(분), 담당자(이름, ;로 여럿), 태그(;로 여럿).
// 첫 줄이 머리글(제목/title)이면 건너뛴다. 상태·우선순위는 한국어나 영어 값을 받는다.
import type { AssigneeView, TaskPriority, TaskStatus } from '$lib/api/client';
import { csvDate, csvRecords, isHeader } from '$lib/csv';

/** 업무 제목 한도(서버 기본 FVOCI_MAX_TITLE_CHARS). */
const MAX_TITLE = 200;

export interface TaskRow {
  title: string;
  description: string;
  status: TaskStatus;
  priority: TaskPriority;
  start_date: string | null;
  due_date: string | null;
  estimate_minutes: number | null;
  assignee_ids: string[];
  tags: string[];
}

export interface TaskCsvResult {
  rows: TaskRow[];
  errors: { line: number; reason: string }[];
}

const STATUS: Record<string, TaskStatus> = {
  '': 'todo',
  '할 일': 'todo',
  todo: 'todo',
  '진행 중': 'in_progress',
  in_progress: 'in_progress',
  완료: 'done',
  done: 'done',
};

const PRIORITY: Record<string, TaskPriority> = {
  '': 'none',
  없음: 'none',
  none: 'none',
  낮음: 'low',
  low: 'low',
  보통: 'medium',
  medium: 'medium',
  높음: 'high',
  high: 'high',
  긴급: 'urgent',
  urgent: 'urgent',
};

const list = (raw: string): string[] =>
  raw
    .split(';')
    .map((s) => s.trim())
    .filter((s) => s !== '');

export function parseTasksCsv(text: string, assignable: readonly AssigneeView[]): TaskCsvResult {
  const rows: TaskRow[] = [];
  const errors: { line: number; reason: string }[] = [];
  csvRecords(text).forEach((record, index) => {
    if (isHeader(record, index)) return;
    const [
      title = '',
      description = '',
      status = '',
      priority = '',
      start = '',
      due = '',
      estimate = '',
      people = '',
      tags = '',
    ] = record.cells;
    const lineNo = record.line;
    const fail = (reason: string): void => {
      errors.push({ line: lineNo, reason });
    };
    if (title === '') {
      fail('제목이 없습니다');
      return;
    }
    if (title.length > MAX_TITLE) {
      fail(`제목은 ${String(MAX_TITLE)}자까지입니다`);
      return;
    }
    const st = STATUS[status.toLowerCase()];
    if (st === undefined) {
      fail(`상태를 알 수 없습니다: ${status}`);
      return;
    }
    const pr = PRIORITY[priority.toLowerCase()];
    if (pr === undefined) {
      fail(`우선순위를 알 수 없습니다: ${priority}`);
      return;
    }
    const startDate = csvDate(start);
    const dueDate = csvDate(due);
    if (startDate === undefined || dueDate === undefined) {
      fail('날짜는 YYYY-MM-DD로 적어 주세요');
      return;
    }
    if (startDate !== null && dueDate !== null && startDate > dueDate) {
      fail('시작일이 기한보다 늦습니다');
      return;
    }
    let minutes: number | null = null;
    if (estimate !== '') {
      if (!/^\d+$/.test(estimate)) {
        fail(`예상 시간(분)은 숫자로 적어 주세요: ${estimate}`);
        return;
      }
      minutes = Number(estimate);
    }
    const ids: string[] = [];
    for (const name of list(people)) {
      const found = assignable.filter((a) => a.display_name === name);
      if (found.length !== 1 || found[0] === undefined) {
        fail(
          found.length === 0
            ? `담당자를 찾을 수 없습니다: ${name}`
            : `같은 이름이 여럿입니다: ${name}`,
        );
        return;
      }
      ids.push(found[0].user_id);
    }
    rows.push({
      title,
      description,
      status: st,
      priority: pr,
      start_date: startDate,
      due_date: dueDate,
      estimate_minutes: minutes,
      assignee_ids: ids,
      tags: list(tags),
    });
  });
  return { rows, errors };
}
