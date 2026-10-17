// 업무 일괄 등록용 CSV 읽기(D-39). 열: 제목, 설명, 상태, 우선순위, 시작일, 기한, 예상(분), 담당자(이름, ;로 여럿), 태그(;로 여럿).
// 첫 줄이 머리글(제목/title)이면 건너뛴다. 상태·우선순위는 한국어나 영어 값을 받는다.
import type { AssigneeView, TaskPriority, TaskStatus } from '$lib/api/client';
import { csvDate, csvRecords, isHeader } from '$lib/csv';
import { m } from '$lib/paraglide/messages.js';

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
      fail(m.tasks_csv_no_title());
      return;
    }
    if (title.length > MAX_TITLE) {
      fail(m.tasks_csv_title_too_long({ max: MAX_TITLE }));
      return;
    }
    const st = STATUS[status.toLowerCase()];
    if (st === undefined) {
      fail(m.tasks_csv_unknown_status({ value: status }));
      return;
    }
    const pr = PRIORITY[priority.toLowerCase()];
    if (pr === undefined) {
      fail(m.tasks_csv_unknown_priority({ value: priority }));
      return;
    }
    const startDate = csvDate(start);
    const dueDate = csvDate(due);
    if (startDate === undefined || dueDate === undefined) {
      fail(m.tasks_csv_bad_date());
      return;
    }
    if (startDate !== null && dueDate !== null && startDate > dueDate) {
      fail(m.tasks_csv_start_after_due());
      return;
    }
    let minutes: number | null = null;
    if (estimate !== '') {
      if (!/^\d+$/.test(estimate)) {
        fail(m.tasks_csv_bad_estimate({ value: estimate }));
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
            ? m.tasks_csv_unknown_assignee({ name })
            : m.tasks_csv_ambiguous_assignee({ name }),
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
