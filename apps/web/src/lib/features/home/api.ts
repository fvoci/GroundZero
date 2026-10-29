import type { components, MyTask, MyTaskCounts, StatusCategory } from '@fvoci/api-client';
import { expectOk } from '$lib/shared/api';
import {
  bool,
  int,
  optStr,
  parseList,
  rec,
  statusCategory,
  str,
  type Json,
} from '$lib/shared/api/parse';

type Schemas = components['schemas'];
export type HomeProgress = Schemas['HomeProgress'];
export type HomeTaskView = Omit<Required<Schemas['HomeTask']>, 'status_category'> & {
  status_category: StatusCategory;
};
export type HomeView = Omit<
  Schemas['Home'],
  'up_next' | 'recent_tasks' | 'recent_documents' | 'my_tasks' | 'pins' | 'favorites'
> & {
  my_tasks: MyTasksView;
  up_next: HomeTaskView[];
  recent_tasks: HomeTaskView[];
  recent_documents: Required<Schemas['HomeDocument']>[];
  pins: MarkedTargetView[];
  favorites: MarkedTargetView[];
};
export type MarkedTargetView = Required<Schemas['MarkedTarget']>;

export type MyTaskBucket = 'overdue' | 'today' | 'upcoming' | 'none' | 'done';

export interface MyTaskView extends Omit<MyTask, 'bucket' | 'status_category'> {
  bucket: MyTaskBucket;
  status_category: StatusCategory;
}

export interface MyTasksView {
  counts: MyTaskCounts;
  workspaces: { workspace_id: string; workspace_name: string; tasks: MyTaskView[] }[];
}

function myTaskBucket(value: string): MyTaskBucket {
  if (
    value === 'overdue' ||
    value === 'today' ||
    value === 'upcoming' ||
    value === 'none' ||
    value === 'done'
  )
    return value;
  throw new TypeError(`unknown bucket ${value}`);
}

function parseMyTask(value: Json): MyTaskView {
  const o = rec(value, 'my task');
  return {
    id: str(o, 'id'),
    item_id: str(o, 'item_id'),
    reference: optStr(o, 'reference'),
    title: str(o, 'title'),
    status: str(o, 'status'),
    status_category: statusCategory(str(o, 'status_category')),
    priority: str(o, 'priority'),
    start_date: optStr(o, 'start_date'),
    due_date: optStr(o, 'due_date'),
    bucket: myTaskBucket(str(o, 'bucket')),
    updated_at: str(o, 'updated_at'),
  };
}

export function parseMyTasks(value: Json): MyTasksView {
  const o = rec(value, 'my tasks');
  const c = rec(o.counts, 'my task counts');
  return {
    counts: {
      overdue: int(c, 'overdue'),
      today: int(c, 'today'),
      upcoming: int(c, 'upcoming'),
      undated: int(c, 'undated'),
      done: int(c, 'done'),
    },
    workspaces: parseList(o.workspaces, (v) => {
      const w = rec(v, 'workspace tasks');
      return {
        workspace_id: str(w, 'workspace_id'),
        workspace_name: str(w, 'workspace_name'),
        tasks: parseList(w.tasks, parseMyTask),
      };
    }),
  };
}

function moduleKind(value: string): Schemas['ModuleKind'] {
  switch (value) {
    case 'project':
    case 'research':
    case 'study':
    case 'course':
    case 'hobby':
      return value;
    default:
      throw new TypeError(`unknown module kind ${value}`);
  }
}

function markTarget(value: string): Schemas['MarkTarget'] {
  if (value === 'item' || value === 'document' || value === 'task') return value;
  throw new TypeError(`unknown mark target ${value}`);
}

function studyUnitKind(value: string): Schemas['StudyUnitKind'] {
  switch (value) {
    case 'pages':
    case 'problems':
    case 'minutes':
    case 'items':
      return value;
    default:
      throw new TypeError(`unknown study unit kind ${value}`);
  }
}

function parseProgress(value: Json): HomeProgress {
  const p = rec(value, 'home progress');
  const kind = str(p, 'kind');
  switch (kind) {
    case 'tasks':
    case 'course':
      return { kind, done: int(p, 'done'), total: int(p, 'total') };
    case 'study':
      return {
        kind,
        amounts: parseList(p.amounts, (value) => {
          const amount = rec(value, 'study amount');
          return {
            unit: studyUnitKind(str(amount, 'unit')),
            done: int(amount, 'done'),
            target: int(amount, 'target'),
          };
        }),
      };
    default:
      throw new TypeError(`unknown home progress kind ${kind}`);
  }
}

function parseHomeTask(value: Json): HomeTaskView {
  const o = rec(value, 'home task');
  const bucket = optStr(o, 'bucket');
  if (bucket !== null && bucket !== 'overdue' && bucket !== 'today' && bucket !== 'upcoming')
    throw new TypeError(`unknown bucket ${bucket}`);
  return {
    id: str(o, 'id'),
    item_id: str(o, 'item_id'),
    workspace_id: str(o, 'workspace_id'),
    reference: optStr(o, 'reference'),
    title: str(o, 'title'),
    status: str(o, 'status'),
    due_date: optStr(o, 'due_date'),
    status_category: statusCategory(str(o, 'status_category')),
    bucket,
  };
}

function parseMarked(value: Json): MarkedTargetView {
  const o = rec(value, 'marked target');
  return {
    kind: markTarget(str(o, 'kind')),
    id: str(o, 'id'),
    label: str(o, 'label'),
    workspace_id: optStr(o, 'workspace_id'),
    item_id: optStr(o, 'item_id'),
  };
}

function parseUpNextCounts(value: unknown): HomeView['up_next_counts'] {
  const c = rec(value, 'up next counts');
  return { overdue: int(c, 'overdue'), today: int(c, 'today'), upcoming: int(c, 'upcoming') };
}

function parseHome(value: Json): HomeView {
  const o = rec(value, 'home');
  return {
    my_tasks: parseMyTasks(o.my_tasks),
    today: str(o, 'today'),
    up_next: parseList(o.up_next, parseHomeTask),
    up_next_counts: parseUpNextCounts(o.up_next_counts),
    recent_tasks: parseList(o.recent_tasks, parseHomeTask),
    recent_documents: parseList(o.recent_documents, (v) => {
      const d = rec(v, 'home document');
      return {
        id: str(d, 'id'),
        title: str(d, 'title'),
        reference: optStr(d, 'reference'),
        workspace_id: optStr(d, 'workspace_id'),
        item_id: optStr(d, 'item_id'),
        updated_at: str(d, 'updated_at'),
      };
    }),
    active_items: parseList(o.active_items, (v) => {
      const i = rec(v, 'home item');
      return {
        id: str(i, 'id'),
        workspace_id: str(i, 'workspace_id'),
        module_kind: moduleKind(str(i, 'module_kind')),
        reference: optStr(i, 'reference'),
        title: str(i, 'title'),
        progress: parseProgress(i.progress),
        can_write: bool(i, 'can_write'),
      };
    }),
    pins: parseList(o.pins, parseMarked),
    favorites: parseList(o.favorites, parseMarked),
  };
}

/** 현재 조직의 집계 읽기. 달력·알림·활동은 각 기능 API에서 읽는다. */
export const homeApi = {
  home: (organizationId: string, tz: string) =>
    expectOk(
      'GET',
      `/api/me/home?organization_id=${encodeURIComponent(organizationId)}&tz=${encodeURIComponent(tz)}`,
      parseHome,
    ),
  myTasks: (organizationId: string, today: string) =>
    expectOk(
      'GET',
      `/api/me/tasks?organization_id=${encodeURIComponent(organizationId)}&today=${encodeURIComponent(today)}`,
      parseMyTasks,
    ),
};
