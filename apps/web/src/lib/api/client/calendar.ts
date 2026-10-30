import { parseEvent, type EventView } from '$lib/features/events/model';
import { expectOk } from '$lib/shared/api';
import {
  bool,
  optStr,
  parseList,
  rec,
  statusCategory,
  str,
  type Json,
} from '$lib/shared/api/parse';
import { type TaskStatus } from '$lib/features/tasks/model';
import type { StatusCategory } from '@fvoci/api-client';

export interface CalendarTaskView {
  id: string;
  item_id: string;
  reference: string | null;
  title: string;
  status: TaskStatus;
  status_category: StatusCategory;
  milestone: boolean;
  start_date: string | null;
  due_date: string | null;
}

export interface EventOccurrenceView {
  event_id: string;
  /** 일정 시간대 기준의 회차 날짜. */
  occurrence: string;
  start_date: string | null;
  end_date: string | null;
  starts_at: string | null;
  ends_at: string | null;
  /** 반복 회차의 현지 시각이 그 시간대에 없어 건너뜀(D-81). */
  skipped: boolean;
}

/** 아직 만들지 않은 반복 업무 회차(예정). */
export interface ProjectedTaskView {
  series_id: string;
  root_task_id: string;
  item_id: string;
  title: string;
  milestone: boolean;
  occurrence: string;
  start_date: string | null;
  due_date: string | null;
}

export const CALENDAR_DATE_KINDS = [
  'episode',
  'study_unit',
  'submission',
  'revision_due',
  'deliverable',
] as const;

export type CalendarDateKind = (typeof CALENDAR_DATE_KINDS)[number];

/** 업무·일정 밖의 날짜(D-80): 강의·스터디 계획일, 투고·산출물 날짜. */
export interface CalendarDateView {
  kind: CalendarDateKind;
  id: string;
  item_id: string;
  title: string;
  date: string;
  /** 내 계획일(본인만 봄). */
  personal: boolean;
  done: boolean;
}

export interface CalendarView {
  tasks: CalendarTaskView[];
  projected_tasks: ProjectedTaskView[];
  events: EventView[];
  occurrences: EventOccurrenceView[];
  dates: CalendarDateView[];
  items: { id: string; reference: string | null; title: string }[];
}

function parseCalendar(value: Json): CalendarView {
  const o = rec(value, 'calendar');
  return {
    tasks: parseList(o.tasks, (v): CalendarTaskView => {
      const t = rec(v, 'calendar task');
      return {
        id: str(t, 'id'),
        item_id: str(t, 'item_id'),
        reference: optStr(t, 'reference'),
        title: str(t, 'title'),
        status: str(t, 'status'),
        status_category: statusCategory(str(t, 'status_category')),
        milestone: bool(t, 'milestone'),
        start_date: optStr(t, 'start_date'),
        due_date: optStr(t, 'due_date'),
      };
    }),
    projected_tasks: parseList(o.projected_tasks, (v): ProjectedTaskView => {
      const p = rec(v, 'projected task');
      return {
        series_id: str(p, 'series_id'),
        root_task_id: str(p, 'root_task_id'),
        item_id: str(p, 'item_id'),
        title: str(p, 'title'),
        milestone: bool(p, 'milestone'),
        occurrence: str(p, 'occurrence'),
        start_date: optStr(p, 'start_date'),
        due_date: optStr(p, 'due_date'),
      };
    }),
    events: parseList(o.events, parseEvent),
    dates: parseList(o.dates, (v): CalendarDateView => {
      const d = rec(v, 'calendar date');
      const kind = CALENDAR_DATE_KINDS.find((k) => k === str(d, 'kind'));
      if (kind === undefined) throw new TypeError('unknown calendar date kind');
      return {
        kind,
        id: str(d, 'id'),
        item_id: str(d, 'item_id'),
        title: str(d, 'title'),
        date: str(d, 'date'),
        personal: bool(d, 'personal'),
        done: bool(d, 'done'),
      };
    }),
    occurrences: parseList(o.occurrences, (v): EventOccurrenceView => {
      const e = rec(v, 'occurrence');
      return {
        event_id: str(e, 'event_id'),
        occurrence: str(e, 'occurrence'),
        start_date: optStr(e, 'start_date'),
        end_date: optStr(e, 'end_date'),
        starts_at: optStr(e, 'starts_at'),
        ends_at: optStr(e, 'ends_at'),
        skipped: e.skipped === true,
      };
    }),
    items: parseList(o.items, (v) => {
      const i = rec(v, 'calendar item');
      return { id: str(i, 'id'), reference: optStr(i, 'reference'), title: str(i, 'title') };
    }),
  };
}

export const calendarApi = {
  /** 달력(D-80). `scope`로 모듈·항목 범위로 좁힌다(같은 데이터, 사본 없음). */
  calendar: (
    workspaceId: string,
    from: string,
    to: string,
    tz: string,
    scope: { moduleId?: string | undefined; itemId?: string | undefined } = {},
  ) => {
    const params = new URLSearchParams({ from, to, tz });
    if (scope.moduleId !== undefined) params.set('module_id', scope.moduleId);
    if (scope.itemId !== undefined) params.set('item_id', scope.itemId);
    return expectOk(
      'GET',
      `/api/workspaces/${encodeURIComponent(workspaceId)}/calendar?${params.toString()}`,
      parseCalendar,
    );
  },
};
