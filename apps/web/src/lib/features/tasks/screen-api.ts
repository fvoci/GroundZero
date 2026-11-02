import type { components, Module, TaskStatusDefinition } from '@fvoci/api-client';
import { untrack } from 'svelte';
import { ApiError, fail, request } from '$lib/shared/api';
import {
  access,
  bool,
  parseList,
  rec,
  str,
  parseTaskStatusDefinition,
  type Json,
  type Access,
} from '$lib/shared/api/parse';
import { parseItem, parseModule, type ItemView } from '$lib/api/client/modules';
import { parseSummary, type SummaryView } from '$lib/api/client/documents';
import { me } from '$lib/me.svelte';
import { accountSafety, switchAccount } from '$lib/features/feedback/feedback.svelte';
import { DiscardedTaskRead, readTaskSessionGeneration } from './context-read';
import { composeTasks, type TaskRelations } from './board-data';
import {
  parseAssignee,
  parseChecklist,
  parseDependency,
  parseSeries,
  parseTaskResource,
  type TaskResource,
} from './model';

type Schemas = components['schemas'];
export type TaskContext = Omit<Schemas['TaskContextRead'], 'item' | 'module' | 'documents'> & {
  item: ItemView;
  module: Module;
  documents: SummaryView[];
};
export type Section<T> = { state: 'ready'; value: T } | { state: 'failed'; error: 'unavailable' };
interface Bundle extends TaskRelations {
  viewer_id: Schemas['TaskBoardRead']['viewer_id'];
  tasks: TaskResource[];
  archived: boolean;
}
export interface BoardRead extends Bundle {
  item_access: Access;
  statuses: TaskStatusDefinition[];
}
export interface DetailRead extends Bundle {
  task: TaskResource;
  item: ItemView;
}
export interface DetailOptionsRead {
  viewer_id: string;
  item_id: string;
  statuses: Section<TaskStatusDefinition[]>;
  assignable: Section<ReturnType<typeof parseAssignee>[]>;
}

function bundle(value: Json): Bundle {
  const o = rec(value, 'task screen');
  return {
    viewer_id: str(o, 'viewer_id'),
    archived: bool(o, 'archived'),
    tasks: parseList(o.tasks, parseTaskResource),
    assignees: parseList(o.assignees, (value) => ({
      ...parseAssignee(value),
      task_id: str(rec(value, 'assignee'), 'task_id'),
    })),
    dependencies: parseList(o.dependencies, parseDependency),
    series: parseList(o.series, parseSeries),
    checklist: parseList(o.checklist, parseChecklist),
  };
}
function section<T>(value: Json, parse: (value: Json) => T): Section<T> {
  const o = rec(value, 'read section');
  if (str(o, 'state') === 'ready') return { state: 'ready', value: parse(o.value) };
  if (o.state !== 'failed' || o.error !== 'unavailable')
    throw new TypeError('invalid read section');
  return { state: 'failed', error: 'unavailable' };
}
interface ReadResponse {
  status: number;
  json: Json;
}
async function owned<T extends { viewer_id: string }>(
  user: string,
  read: () => Promise<ReadResponse>,
  parse: (value: Json) => T,
): Promise<T> {
  const generation = readTaskSessionGeneration();
  const safetyGeneration = untrack(() => accountSafety.generation);
  const isCurrent = (): boolean =>
    me.userId === user &&
    generation === readTaskSessionGeneration() &&
    safetyGeneration === accountSafety.generation;
  let response: ReadResponse;
  try {
    response = await read();
  } catch (cause: unknown) {
    if (!isCurrent()) throw new DiscardedTaskRead();
    throw cause;
  }
  if (!isCurrent()) throw new DiscardedTaskRead();
  // Reject stale failures before fail() can publish expired/account-changed state.
  if (response.status < 200 || response.status >= 300) fail(response.status, response.json);
  const result = parse(response.json);
  if (result.viewer_id !== user) {
    switchAccount();
    throw new ApiError(409, 'account_changed', 'account_changed');
  }
  return result;
}

export const taskScreenApi = {
  context: (id: string) =>
    owned(
      me.userId,
      () => request('GET', `/api/items/${encodeURIComponent(id)}/task-context`),
      (value): TaskContext => {
        const o = rec(value, 'task context');
        return {
          viewer_id: str(o, 'viewer_id'),
          item: parseItem(o.item),
          module: parseModule(o.module),
          documents: parseList(o.documents, parseSummary),
        };
      },
    ),
  board: (id: string) =>
    owned(
      me.userId,
      () => request('GET', `/api/items/${encodeURIComponent(id)}/task-board`),
      (value): BoardRead => {
        const o = rec(value, 'task board');
        return {
          ...bundle(value),
          item_access: access(str(o, 'item_access')),
          statuses: parseList(o.statuses, parseTaskStatusDefinition),
        };
      },
    ),
  detail: (id: string) =>
    owned(
      me.userId,
      () => request('GET', `/api/tasks/${encodeURIComponent(id)}/detail-read`),
      (value): DetailRead => {
        const o = rec(value, 'task detail');
        return {
          ...bundle(value),
          task: parseTaskResource(o.task),
          item: parseItem(o.item),
        };
      },
    ),
  options: (id: string) =>
    owned(
      me.userId,
      () => request('GET', `/api/items/${encodeURIComponent(id)}/task-detail-options`),
      (value): DetailOptionsRead => {
        const o = rec(value, 'task detail options');
        const item_id = str(o, 'item_id');
        if (item_id !== id) throw new TypeError('invalid task options scope');
        return {
          viewer_id: str(o, 'viewer_id'),
          item_id,
          statuses: section(o.statuses, (v) => parseList(v, parseTaskStatusDefinition)),
          assignable: section(o.assignable, (v) => parseList(v, parseAssignee)),
        };
      },
    ),
};

export function composeScreenDetail(read: DetailRead) {
  const resources = [read.task, ...read.tasks.filter((task) => task.id !== read.task.id)];
  const task = composeTasks(resources, read, read.archived ? 'read' : read.item.access)[0];
  if (task === undefined) throw new TypeError('missing task');
  return { ...task, checklist: read.checklist.filter((row) => row.task_id === read.task.id) };
}
