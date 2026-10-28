import { api } from '$lib/api/client';
import type { Access } from '$lib/shared/api/parse';
import { workApi } from './api';
import {
  parseRule,
  type TaskResource,
  type TaskView,
  type TaskAssignee,
  type TaskDependency,
  type TaskSeriesResource,
  type ChecklistItem,
} from './model';

export interface TaskRelations {
  assignees: TaskAssignee[];
  dependencies: TaskDependency[];
  series: TaskSeriesResource[];
  checklist: ChecklistItem[];
}

/** 읽기의 시점이 달라도 모르는 업무·계열 id를 화면에 얹지 않는다. */
export function composeTasks(
  tasks: TaskResource[],
  relations: TaskRelations,
  access: Access,
): TaskView[] {
  const ids = new Set(tasks.map((task) => task.id));
  const series = new Map(relations.series.map((row) => [row.id, row]));
  return tasks.map((task) => {
    const checklist = relations.checklist.filter((row) => row.task_id === task.id);
    const recurrence = task.series_id == null ? undefined : series.get(task.series_id);
    return {
      ...task,
      parent_id: task.parent_id != null && ids.has(task.parent_id) ? task.parent_id : null,
      reference: task.reference ?? null,
      start_date: task.start_date ?? null,
      due_date: task.due_date ?? null,
      estimate_minutes: task.estimate_minutes ?? null,
      occurrence_date: task.occurrence_date ?? null,
      completed_at: task.completed_at ?? null,
      assignees: relations.assignees
        .filter((row) => row.task_id === task.id)
        .map(({ user_id, display_name }) => ({ user_id, display_name })),
      predecessors: relations.dependencies
        .filter((row) => row.successor_id === task.id && ids.has(row.predecessor_id))
        .map((row) => ({ task_id: row.predecessor_id, kind: row.kind, lag_days: row.lag_days })),
      series: recurrence === undefined ? null : { ...recurrence, rule: parseRule(recurrence.rule) },
      checklist_total: checklist.length,
      checklist_done: checklist.filter((row) => row.checked).length,
      access,
    };
  });
}

export async function loadRelations(
  scope: { item: string } | { task: string },
): Promise<TaskRelations> {
  const [assignees, dependencies, series, checklist] = await Promise.all([
    workApi.assignees(scope),
    workApi.dependencies(scope),
    workApi.series(scope),
    workApi.checklist(scope),
  ]);
  return { assignees, dependencies, series, checklist };
}

export async function loadBoard(itemId: string, access?: Access): Promise<TaskView[]> {
  const [tasks, relations, grade] = await Promise.all([
    workApi.tasks(itemId),
    loadRelations({ item: itemId }),
    access === undefined
      ? api.item(itemId).then((item) => (item.archived ? ('read' as const) : item.access))
      : Promise.resolve(access),
  ]);
  return composeTasks(tasks, relations, grade);
}
