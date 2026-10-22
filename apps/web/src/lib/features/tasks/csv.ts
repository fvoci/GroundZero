// 업무 일괄 등록용 CSV 읽기(D-39). 열: 제목, 설명, 상태, 우선순위, 시작일, 기한, 예상(분), 담당자(이름, ;로 여럿), 태그(;로 여럿).
// 첫 줄이 머리글(제목/title)이면 건너뛴다. 상태·우선순위는 한국어나 영어 값을 받는다.
import type { AssigneeView, TaskPriority, TaskStatus } from '$lib/api/client';
import { csvDate, csvRecords, isHeader } from '$lib/csv';
import { m } from '$lib/paraglide/messages.js';

/** 업무 제목 한도(서버 기본 FVOCI_MAX_TITLE_CHARS). */
const MAX_TITLE = 200;
// 서버가 고치기에서 거부하는 값(crates/server/src/features/tasks.rs). 만들기는 제목만 보내 먼저 성공하므로, 여기서 놓치면
// 제목만 있는 업무가 남는다. 글자 수는 서버처럼 문자(코드 포인트)로 센다.
/** 설명 글자 수(MAX_DESCRIPTION_CHARS). */
const MAX_DESCRIPTION = 10_000;
/** 태그 수(MAX_TAGS, 같은 태그를 합치기 전)와 태그 하나의 글자 수(MAX_TAG_CHARS). */
const MAX_TAGS = 20;
const MAX_TAG_CHARS = 50;
/** 날짜의 해(DATE_YEARS). */
const MIN_YEAR = 1900;
const MAX_YEAR = 2200;
/** 예상(분)은 i32. */
const MAX_ESTIMATE = 2_147_483_647;

const chars = (s: string): number => Array.from(s).length;
/** 제어 문자(Rust `char::is_control`, 일반 범주 Cc). */
const CONTROL = /\p{Cc}/u;
/** 설명은 줄바꿈·탭만 받는다(validate_text). */
const CONTROL_IN_TEXT = /(?![\n\t])\p{Cc}/u;

/** 날짜 칸을 서버(parse_date)처럼 읽는다: 비었으면 null, 있는 날짜면 그 값, 아니면 오류 문구. */
function taskDate(raw: string): { value: string | null } | { reason: string } {
  const date = csvDate(raw);
  if (date === undefined) return { reason: m.tasks_csv_bad_date() };
  if (date === null) return { value: null };
  const year = Number(date.slice(0, 4));
  const month = Number(date.slice(5, 7));
  const day = Number(date.slice(8, 10));
  if (year < MIN_YEAR || year > MAX_YEAR) {
    return { reason: m.tasks_csv_date_range({ min: MIN_YEAR, max: MAX_YEAR }) };
  }
  // 2026-02-30처럼 없는 날은 Date가 다음 달로 넘기므로 되읽어 비교한다.
  const probe = new Date(Date.UTC(year, month - 1, day));
  const real =
    probe.getUTCFullYear() === year &&
    probe.getUTCMonth() === month - 1 &&
    probe.getUTCDate() === day;
  return real ? { value: date } : { reason: m.tasks_csv_bad_date() };
}

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
    if (chars(title) > MAX_TITLE) {
      fail(m.tasks_csv_title_too_long({ max: MAX_TITLE }));
      return;
    }
    if (chars(description) > MAX_DESCRIPTION) {
      fail(m.tasks_csv_description_too_long({ max: MAX_DESCRIPTION }));
      return;
    }
    if (CONTROL_IN_TEXT.test(description)) {
      fail(m.tasks_csv_description_control());
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
    const startDate = taskDate(start);
    if ('reason' in startDate) {
      fail(startDate.reason);
      return;
    }
    const dueDate = taskDate(due);
    if ('reason' in dueDate) {
      fail(dueDate.reason);
      return;
    }
    if (startDate.value !== null && dueDate.value !== null && startDate.value > dueDate.value) {
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
      if (minutes > MAX_ESTIMATE) {
        fail(m.tasks_csv_estimate_too_large({ value: estimate }));
        return;
      }
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
    const tagList = list(tags);
    if (tagList.length > MAX_TAGS) {
      fail(m.tasks_csv_too_many_tags({ max: MAX_TAGS }));
      return;
    }
    if (tagList.some((t) => chars(t) > MAX_TAG_CHARS)) {
      fail(m.tasks_csv_tag_too_long({ max: MAX_TAG_CHARS }));
      return;
    }
    if (tagList.some((t) => CONTROL.test(t))) {
      fail(m.tasks_csv_tag_control());
      return;
    }
    rows.push({
      title,
      description,
      status: st,
      priority: pr,
      start_date: startDate.value,
      due_date: dueDate.value,
      estimate_minutes: minutes,
      assignee_ids: ids,
      tags: tagList,
    });
  });
  return { rows, errors };
}
