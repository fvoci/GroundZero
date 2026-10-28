import type { StatusCategory } from '@fvoci/api-client';
import { ApiError } from '$lib/shared/api';
import { access, bool, int, optInt, optStr, parseList, rec, statusCategory, str, strings, type Access, type Json } from '$lib/shared/api/parse';
// --- 업무 -------------------------------------------------------------

// 상태 이름은 항목 정의의 문자열, 완료 판단은 status_category만 사용한다.
export type TaskStatus = string;
export type TaskPriority = 'none' | 'low' | 'medium' | 'high' | 'urgent';
export const TASK_PRIORITIES: readonly TaskPriority[] = [
  'none',
  'low',
  'medium',
  'high',
  'urgent',
] as const;

export interface AssigneeView {
  user_id: string;
  display_name: string;
}

export const FREQUENCIES = ['daily', 'weekly', 'monthly', 'yearly'] as const;
export type Frequency = (typeof FREQUENCIES)[number];

/** 반복 규칙. 기준일이 첫 회차, `count`는 첫 회차 포함, 없는 날(31일·2월 29일)은 건너뜀. */
export interface RecurrenceRule {
  freq: Frequency;
  interval: number;
  /** ISO 요일(월=1 … 일=7), 매주에만. */
  weekdays: number[];
  until: string | null;
  count: number | null;
}

export function parseRule(value: Json): RecurrenceRule | null {
  if (value === null || value === undefined) return null;
  const o = rec(value, 'recurrence');
  const freq = FREQUENCIES.find((f) => f === str(o, 'freq'));
  if (freq === undefined) throw new TypeError(`unknown frequency ${str(o, 'freq')}`);
  return {
    freq,
    interval: int(o, 'interval'),
    weekdays: Array.isArray(o.weekdays) ? o.weekdays.map((d) => Number(d)) : [],
    until: optStr(o, 'until'),
    count: optInt(o, 'count'),
  };
}

// 반복 충돌 최신본은 편집 중 입력과 분리한다.
export class TaskRuleConflict extends ApiError {
  constructor(readonly current: TaskDetailView) {
    super(409, 'rule_conflict', 'rule_conflict');
  }
}

export interface TaskSeriesView {
  id: string;
  root_task_id: string;
  rule: RecurrenceRule | null;
  timezone: string;
}

export interface TaskView {
  id: string;
  item_id: string;
  module_id: string;
  workspace_id: string;
  parent_id: string | null;
  reference: string | null;
  title: string;
  description: string;
  status: TaskStatus;
  status_category: StatusCategory;
  priority: TaskPriority;
  start_date: string | null;
  due_date: string | null;
  estimate_minutes: number | null;
  tags: string[];
  milestone: boolean;
  /** 선행 관계: 선행 업무, 종류, 지연일(D-91). 후속 업무는 다른 업무의 이 목록을 거꾸로 찾는다. */
  predecessors: TaskLinkView[];
  /** 반복 업무의 회차면 그 반복. */
  series: TaskSeriesView | null;
  occurrence_date: string | null;
  assignees: AssigneeView[];
  checklist_done: number;
  checklist_total: number;
  version: number;
  /** 이동 판(D-89): 열이나 열 안 위치가 바뀔 때마다 오른다. 조건부 이동의 기준. */
  move_version: number;
  completed_at: string | null;
  access: Access;
  updated_at: string;
}

export interface TaskChecklistItemView {
  id: string;
  text: string;
  checked: boolean;
}

export interface TaskDetailView extends TaskView {
  checklist: TaskChecklistItemView[];
}

export interface TaskInput {
  title: string;
  description: string;
  status: TaskStatus;
  priority: TaskPriority;
  start_date: string | null;
  due_date: string | null;
  estimate_minutes: number | null;
  tags: string[];
  assignee_ids: string[];
  milestone: boolean;
}

/**
 * 선행 관계 종류(D-91). FS 종료 후 시작, SS 시작 후 시작, FF 종료 후 종료, SF 시작 후 종료. 지연일은 0 이상의
 * 달력 날짜 수다.
 */
export type LinkKind = 'FS' | 'SS' | 'FF' | 'SF';
export const LINK_KINDS: readonly LinkKind[] = ['FS', 'SS', 'FF', 'SF'];
/** 지연일 상한(서버와 같다). */
export const MAX_LAG_DAYS = 3650;

/** 관계 편집을 시작한 때의 종류·지연일(관계 설정의 기준 조건). */
export interface LinkBase {
  kind: LinkKind;
  lag_days: number;
}

export interface TaskLinkView {
  task_id: string;
  kind: LinkKind;
  lag_days: number;
}

/** 재배치의 원인 관계. */
export interface ShiftCauseView {
  predecessor_id: string;
  predecessor_reference: string | null;
  predecessor_title: string;
  kind: LinkKind;
  lag_days: number;
}

/** 선행 업무나 관계를 바꿀 때 함께 옮길 후속 업무(현재 → 제안). 승인하면 그대로 `SuccessorShift`로 보낸다. */
export interface ShiftProposalView {
  task_id: string;
  reference: string | null;
  title: string;
  base_version: number;
  from_start_date: string | null;
  from_due_date: string | null;
  start_date: string | null;
  due_date: string | null;
  /** 이 업무를 옮기게 한 관계. */
  causes: ShiftCauseView[];
}

/** 날짜가 없어 옮기지 않는 업무(날짜를 지어내지 않는다). */
export interface UndatedTaskView {
  task_id: string;
  reference: string | null;
  title: string;
}

/** 계획을 함께 저장할 수 없는 까닭: 건수 한도를 넘거나, 제안 날짜가 저장 범위(1900~2200년) 밖이다. */
export type PlanBlockReason = 'too_many_shifts' | 'date_out_of_range';
const PLAN_BLOCK_REASONS: readonly PlanBlockReason[] = ['too_many_shifts', 'date_out_of_range'];

/** 저장할 수 없는 계획의 까닭과 근거(건수, 또는 범위 밖 날짜를 받은 업무와 그 날짜). */
export interface PlanBlockedView {
  reason: PlanBlockReason;
  shift_count: number;
  max_shifts: number;
  task_id: string | null;
  reference: string | null;
  date: string | null;
}

/** 재배치 계획: 옮길 후속 업무와 날짜가 없어 계산하지 않은 업무, 함께 저장할 수 없으면 그 까닭. */
export interface SchedulePlanView {
  shifts: ShiftProposalView[];
  undated: UndatedTaskView[];
  blocked: PlanBlockedView | null;
}

export interface SuccessorShift {
  task_id: string;
  base_version: number;
  start_date: string | null;
  due_date: string | null;
}

function parseShiftProposal(value: Json): ShiftProposalView {
  const o = rec(value, 'shift proposal');
  return {
    task_id: str(o, 'task_id'),
    reference: optStr(o, 'reference'),
    title: str(o, 'title'),
    base_version: int(o, 'base_version'),
    from_start_date: optStr(o, 'from_start_date'),
    from_due_date: optStr(o, 'from_due_date'),
    start_date: optStr(o, 'start_date'),
    due_date: optStr(o, 'due_date'),
    causes: parseList(o.causes, parseShiftCause),
  };
}

export function linkKind(value: string): LinkKind {
  const found = LINK_KINDS.find((k) => k === value);
  if (found === undefined) throw new TypeError(`unknown dependency kind ${value}`);
  return found;
}

function parseShiftCause(value: Json): ShiftCauseView {
  const o = rec(value, 'shift cause');
  return {
    predecessor_id: str(o, 'predecessor_id'),
    predecessor_reference: optStr(o, 'predecessor_reference'),
    predecessor_title: str(o, 'predecessor_title'),
    kind: linkKind(str(o, 'kind')),
    lag_days: int(o, 'lag_days'),
  };
}

function parseTaskLink(value: Json): TaskLinkView {
  const o = rec(value, 'task link');
  return {
    task_id: str(o, 'task_id'),
    kind: linkKind(str(o, 'kind')),
    lag_days: int(o, 'lag_days'),
  };
}

function parseUndated(value: Json): UndatedTaskView {
  const o = rec(value, 'undated task');
  return { task_id: str(o, 'task_id'), reference: optStr(o, 'reference'), title: str(o, 'title') };
}

function parsePlanBlocked(value: Json): PlanBlockedView {
  const o = rec(value, 'plan blocked');
  const reason = PLAN_BLOCK_REASONS.find((r) => r === str(o, 'reason'));
  if (reason === undefined) throw new TypeError(`unknown plan block reason ${str(o, 'reason')}`);
  return {
    reason,
    shift_count: int(o, 'shift_count'),
    max_shifts: int(o, 'max_shifts'),
    task_id: optStr(o, 'task_id'),
    reference: optStr(o, 'reference'),
    date: optStr(o, 'date'),
  };
}

export function parseSchedulePlan(value: Json): SchedulePlanView {
  const o = rec(value, 'schedule plan');
  return {
    shifts: parseList(o.shifts, parseShiftProposal),
    undated: parseList(o.undated, parseUndated),
    blocked: o.blocked === null || o.blocked === undefined ? null : parsePlanBlocked(o.blocked),
  };
}

export function taskPriority(value: string): TaskPriority {
  const found = TASK_PRIORITIES.find((p) => p === value);
  if (found === undefined) throw new TypeError(`unknown priority ${value}`);
  return found;
}

export function parseAssignee(value: Json): AssigneeView {
  const o = rec(value, 'assignee');
  return { user_id: str(o, 'user_id'), display_name: str(o, 'display_name') };
}

export function parseTask(value: Json): TaskView {
  const o = rec(value, 'task');
  return {
    id: str(o, 'id'),
    item_id: str(o, 'item_id'),
    module_id: str(o, 'module_id'),
    workspace_id: str(o, 'workspace_id'),
    parent_id: optStr(o, 'parent_id'),
    reference: optStr(o, 'reference'),
    title: str(o, 'title'),
    description: str(o, 'description'),
    status: str(o, 'status'),
    status_category: statusCategory(str(o, 'status_category')),
    priority: taskPriority(str(o, 'priority')),
    start_date: optStr(o, 'start_date'),
    due_date: optStr(o, 'due_date'),
    estimate_minutes: optInt(o, 'estimate_minutes'),
    tags: strings(o.tags),
    milestone: bool(o, 'milestone'),
    predecessors: parseList(o.predecessors, parseTaskLink),
    series:
      o.series === null || o.series === undefined
        ? null
        : ((): TaskSeriesView => {
            const s = rec(o.series, 'series');
            return {
              id: str(s, 'id'),
              root_task_id: str(s, 'root_task_id'),
              rule: parseRule(s.rule),
              timezone: str(s, 'timezone'),
            };
          })(),
    occurrence_date: optStr(o, 'occurrence_date'),
    assignees: parseList(o.assignees, parseAssignee),
    checklist_done: int(o, 'checklist_done'),
    checklist_total: int(o, 'checklist_total'),
    version: int(o, 'version'),
    move_version: int(o, 'move_version'),
    completed_at: optStr(o, 'completed_at'),
    access: access(str(o, 'access')),
    updated_at: str(o, 'updated_at'),
  };
}

export function parseTaskDetail(value: Json): TaskDetailView {
  const o = rec(value, 'task detail');
  return {
    ...parseTask(o),
    checklist: parseList(o.checklist, (v): TaskChecklistItemView => {
      const c = rec(v, 'checklist item');
      return { id: str(c, 'id'), text: str(c, 'text'), checked: bool(c, 'checked') };
    }),
  };
}
