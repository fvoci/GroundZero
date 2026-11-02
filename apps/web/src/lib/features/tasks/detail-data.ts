import { api } from '$lib/api/client';
import { workApi, WorkRuleConflict } from './api';
import { composeTasks, loadBoard, loadRelations } from './board-data';
import {
  TaskRuleConflict,
  type TaskResource,
  type TaskConflictCurrent,
  type TaskDetailView,
  type TaskInput,
  type SuccessorShift,
} from './model';

type SaveResult =
  | { kind: 'updated'; task: TaskDetailView }
  | { kind: 'conflict'; current: TaskConflictCurrent }
  | { kind: 'shift_conflict'; current: TaskConflictCurrent };
type LinkResult =
  | { kind: 'updated'; task: TaskDetailView }
  | { kind: 'link_conflict'; current: TaskDetailView }
  | { kind: 'shift_conflict'; current: TaskDetailView };

async function readDetailScope(itemId: string) {
  const [others, relations, item] = await Promise.all([
    workApi.tasks(itemId),
    loadRelations({ item: itemId }),
    api.item(itemId),
  ]);
  return { others, relations, item };
}

/** 상세의 관계는 항목 범위에서 조합해 상위·선행 업무도 현재 목록에 있는지 확인한다. */
function composeDetail(
  resource: TaskResource,
  { others, relations, item }: Awaited<ReturnType<typeof readDetailScope>>,
): TaskDetailView {
  const resources = [resource, ...others.filter((row) => row.id !== resource.id)];
  const task = composeTasks(resources, relations, item.archived ? 'read' : item.access)[0];
  if (task === undefined) throw new TypeError('missing task');
  return { ...task, checklist: relations.checklist.filter((row) => row.task_id === resource.id) };
}

async function detailOf(resource: TaskResource): Promise<TaskDetailView> {
  return composeDetail(resource, await readDetailScope(resource.item_id));
}

export async function loadDetail(id: string, itemId?: string): Promise<TaskDetailView> {
  const task = workApi.task(id);
  if (itemId === undefined) return detailOf(await task);
  // 부모 항목은 힌트다. 단건 실패가 우선이며, 먼저 실패한 범위 읽기도 즉시 처리해 둔다.
  const scope = readDetailScope(itemId).then(
    (value) => ({ kind: 'loaded' as const, value }),
    (error: unknown) => ({ kind: 'failed' as const, error }),
  );
  const resource = await task;
  // 다른 항목의 업무를 가리킨 URL도 기존처럼 실제 업무의 범위로 조합한다.
  if (resource.item_id !== itemId) return detailOf(resource);
  const result = await scope;
  if (result.kind === 'failed') throw result.error;
  return composeDetail(resource, result.value);
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
    input: Partial<TaskInput>,
    shifts: SuccessorShift[] = [],
  ): Promise<SaveResult> => {
    const result = await workApi.updateTask(user, id, version, input, shifts);
    if (result.kind === 'updated') return { kind: result.kind, task: await detailOf(result.task) };
    return result;
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
