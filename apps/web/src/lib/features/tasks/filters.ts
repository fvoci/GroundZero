// 업무 검색·필터·정렬과 저장한 보기의 내용(D-31). 서버는 보기 내용을 해석하지 않고 저장만 한다.
import type { TaskPriority, TaskStatus, TaskView } from '$lib/api/client';
import { TASK_STATUSES } from '$lib/api/client';

export const VIEWS = ['board', 'list', 'gantt', 'calendar'] as const;
export type TaskViewMode = (typeof VIEWS)[number];
export const SORTS = ['position', 'due', 'priority', 'title', 'updated'] as const;
export type TaskSort = (typeof SORTS)[number];
const PRIORITIES: readonly TaskPriority[] = ['none', 'low', 'medium', 'high', 'urgent'];

export interface TaskFilter {
  /** 제목·설명·번호에 들어 있는 글자. */
  text: string;
  status: TaskStatus | 'any';
  /** `any`·`me`·`none` 또는 사용자 id. */
  assignee: string;
  priority: TaskPriority | 'any';
  tag: string;
  /** 마감일 범위(YYYY-MM-DD, 끝 포함). 마감이 없으면 범위를 주었을 때 빠진다. */
  dueFrom: string;
  dueTo: string;
}

export interface ViewConfig {
  view: TaskViewMode;
  filter: TaskFilter;
  sort: TaskSort;
}

export const EMPTY_FILTER: TaskFilter = {
  text: '',
  status: 'any',
  assignee: 'any',
  priority: 'any',
  tag: '',
  dueFrom: '',
  dueTo: '',
};

const FILTER_KEYS: readonly (keyof TaskFilter)[] = [
  'text',
  'status',
  'assignee',
  'priority',
  'tag',
  'dueFrom',
  'dueTo',
];

export function isFiltered(f: TaskFilter): boolean {
  return FILTER_KEYS.some((k) => f[k] !== EMPTY_FILTER[k]);
}

export function applyFilter(tasks: readonly TaskView[], f: TaskFilter, me: string): TaskView[] {
  const text = f.text.trim().toLowerCase();
  const tag = f.tag.trim().toLowerCase();
  return tasks.filter((t) => {
    if (
      text !== '' &&
      !`${t.reference ?? ''} ${t.title} ${t.description}`.toLowerCase().includes(text)
    )
      return false;
    if (f.status !== 'any' && t.status !== f.status) return false;
    if (f.priority !== 'any' && t.priority !== f.priority) return false;
    if (tag !== '' && !t.tags.some((x) => x.toLowerCase() === tag)) return false;
    if (f.assignee === 'none' && t.assignees.length > 0) return false;
    if (f.assignee === 'me' && !t.assignees.some((a) => a.user_id === me)) return false;
    if (
      !['any', 'none', 'me'].includes(f.assignee) &&
      !t.assignees.some((a) => a.user_id === f.assignee)
    )
      return false;
    if (f.dueFrom !== '' && (t.due_date === null || t.due_date < f.dueFrom)) return false;
    if (f.dueTo !== '' && (t.due_date === null || t.due_date > f.dueTo)) return false;
    return true;
  });
}

const PRIORITY_RANK: Record<TaskPriority, number> = {
  urgent: 0,
  high: 1,
  medium: 2,
  low: 3,
  none: 4,
};

/** 정렬. `position`은 서버 순서를 그대로 둔다. 마감 없는 업무는 뒤로. */
export function sortTasks(tasks: readonly TaskView[], sort: TaskSort): TaskView[] {
  const out = [...tasks];
  switch (sort) {
    case 'position':
      return out;
    case 'due':
      return out.sort((a, b) => (a.due_date ?? '9999').localeCompare(b.due_date ?? '9999'));
    case 'priority':
      return out.sort((a, b) => PRIORITY_RANK[a.priority] - PRIORITY_RANK[b.priority]);
    case 'title':
      return out.sort((a, b) => a.title.localeCompare(b.title, 'ko'));
    case 'updated':
      return out.sort((a, b) => b.updated_at.localeCompare(a.updated_at));
  }
}

function pick<T extends string>(value: unknown, allowed: readonly T[], fallback: T): T {
  return allowed.find((v) => v === value) ?? fallback;
}

const text = (value: unknown): string => (typeof value === 'string' ? value : '');

/** 저장된 보기 내용을 읽는다. 모르는 값은 기본값으로. */
export function readConfig(raw: Record<string, unknown>): ViewConfig {
  const f = isRecord(raw.filter) ? raw.filter : {};
  return {
    view: pick(raw.view, VIEWS, 'board'),
    sort: pick(raw.sort, SORTS, 'position'),
    filter: {
      text: text(f.text),
      status: pick(f.status, [...TASK_STATUSES, 'any'] as const, 'any'),
      assignee: text(f.assignee) === '' ? 'any' : text(f.assignee),
      priority: pick(f.priority, [...PRIORITIES, 'any'] as const, 'any'),
      tag: text(f.tag),
      dueFrom: text(f.dueFrom),
      dueTo: text(f.dueTo),
    },
  };
}

export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/** 업무에 나오는 담당자(이름순, 겹치지 않게). */
export function assigneesOf(tasks: readonly TaskView[]): [string, string][] {
  const map = new Map<string, string>();
  for (const t of tasks) for (const a of t.assignees) map.set(a.user_id, a.display_name);
  return [...map.entries()].sort((a, b) => a[1].localeCompare(b[1], 'ko'));
}

/** 날짜별 업무: 마감일(없으면 시작일) 한 칸. */
export function tasksByDay(tasks: readonly TaskView[]): Map<string, TaskView[]> {
  const map = new Map<string, TaskView[]>();
  for (const t of tasks) {
    const day = t.due_date ?? t.start_date;
    if (day !== null) map.set(day, [...(map.get(day) ?? []), t]);
  }
  return map;
}
