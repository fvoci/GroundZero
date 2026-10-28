import { api } from '$lib/api/client';
import { workApi, WorkRuleConflict } from './api';
import { composeTasks, loadBoard, loadRelations } from './board-data';
import {
  TaskRuleConflict,
  type TaskResource,
  type TaskDetailView,
  type TaskInput,
  type SuccessorShift,
} from './model';

type SaveResult =
  | { kind: 'updated'; task: TaskDetailView }
  | { kind: 'conflict'; current: TaskDetailView }
  | { kind: 'shift_conflict'; current: TaskDetailView };
type LinkResult =
  | { kind: 'updated'; task: TaskDetailView }
  | { kind: 'link_conflict'; current: TaskDetailView }
  | { kind: 'shift_conflict'; current: TaskDetailView };

/** 상세의 관계는 항목 범위에서 조합해 상위·선행 업무도 현재 목록에 있는지 확인한다. */
async function detailOf(resource: TaskResource): Promise<TaskDetailView> {
  const [others, relations, item] = await Promise.all([
    workApi.tasks(resource.item_id),
    loadRelations({ item: resource.item_id }),
    api.item(resource.item_id),
  ]);
  const resources = [resource, ...others.filter((row) => row.id !== resource.id)];
  const task = composeTasks(resources, relations, item.archived ? 'read' : item.access)[0];
  if (task === undefined) throw new TypeError('missing task');
  return { ...task, checklist: relations.checklist.filter((row) => row.task_id === resource.id) };
}

export async function loadDetail(id: string): Promise<TaskDetailView> {
  return detailOf(await workApi.task(id));
}

/** 화면 조합층. 명령은 하나만 보내고, 응답 뒤 필요한 관계를 새 자원 읽기로 조합한다. */
export const taskData = {
  ...workApi,
  tasks: loadBoard,
  task: loadDetail,
  createTask: async (...args: Parameters<typeof workApi.createTask>) => {
    return detailOf(await workApi.createTask(...args));
  },
  createQuickTask: async (...args: Parameters<typeof workApi.createQuickTask>) => {
    return detailOf(await workApi.createQuickTask(...args));
  },
  moveTask: async (...args: Parameters<typeof workApi.moveTask>) => {
    return detailOf(await workApi.moveTask(...args));
  },
  setTaskParent: async (...args: Parameters<typeof workApi.setTaskParent>) => {
    return detailOf(await workApi.setTaskParent(...args));
  },
  addChecklist: async (...args: Parameters<typeof workApi.addChecklist>) => {
    await workApi.addChecklist(...args);
    return loadDetail(args[1]);
  },
  removePredecessor: async (...args: Parameters<typeof workApi.removePredecessor>) => {
    await workApi.removePredecessor(...args);
    return loadDetail(args[1]);
  },
  updateTask: async (
    user: string,
    id: string,
    version: number,
    input: TaskInput,
    shifts: SuccessorShift[] = [],
  ): Promise<SaveResult> => {
    const result = await workApi.updateTask(user, id, version, input, shifts);
    if (result.kind === 'updated') return { kind: result.kind, task: await detailOf(result.task) };
    return { kind: result.kind, current: await detailOf(result.current) };
  },
  setPredecessor: async (
    ...args: Parameters<typeof workApi.setPredecessor>
  ): Promise<LinkResult> => {
    const result = await workApi.setPredecessor(...args);
    const current = await loadDetail(args[1]);
    if (result.kind === 'updated') return { kind: result.kind, task: current };
    return { kind: result.kind, current };
  },
  setTaskRecurrence: async (
    ...args: Parameters<typeof workApi.setTaskRecurrence>
  ): Promise<TaskDetailView> => {
    try {
      await workApi.setTaskRecurrence(...args);
    } catch (error: unknown) {
      if (error instanceof WorkRuleConflict) throw new TaskRuleConflict(await loadDetail(args[1]));
      throw error;
    }
    return loadDetail(args[1]);
  },
};
