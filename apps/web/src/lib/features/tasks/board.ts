// 칸반 계산 (D-31). 열은 상태, 열 안의 순서는 서버 목록 순서(위치)를 따른다.
import type { TaskStatus } from '$lib/api/client';
import { TASK_STATUSES } from '$lib/api/client';

export interface BoardTask {
  id: string;
  status: TaskStatus;
}

export interface BoardMove {
  status: TaskStatus;
  beforeId: string | null;
}

export type BoardAction = 'up' | 'down' | 'left' | 'right';

export function lanes<T extends BoardTask>(tasks: readonly T[]): Record<TaskStatus, T[]> {
  const out: Record<TaskStatus, T[]> = { todo: [], in_progress: [], done: [] };
  for (const t of tasks) out[t.status].push(t);
  return out;
}

/** 키보드·버튼 이동을 (열, 바로 뒤 업무)로 바꾼다. 할 수 없으면 undefined. */
export function planBoardMove(
  tasks: readonly BoardTask[],
  id: string,
  action: BoardAction,
): BoardMove | undefined {
  const task = tasks.find((t) => t.id === id);
  if (task === undefined) return undefined;
  const lane = lanes(tasks)[task.status];
  const index = lane.findIndex((t) => t.id === id);
  const column = TASK_STATUSES.indexOf(task.status);
  switch (action) {
    case 'up': {
      const previous = lane[index - 1];
      return previous === undefined ? undefined : { status: task.status, beforeId: previous.id };
    }
    case 'down':
      if (index >= lane.length - 1) return undefined;
      return { status: task.status, beforeId: lane[index + 2]?.id ?? null };
    case 'left': {
      const status = TASK_STATUSES[column - 1];
      return status === undefined ? undefined : { status, beforeId: null };
    }
    case 'right': {
      const status = TASK_STATUSES[column + 1];
      return status === undefined ? undefined : { status, beforeId: null };
    }
  }
}

export interface ListTask extends BoardTask {
  parent_id: string | null;
}

/** 목록 보기 순서: 상위 업무마다 바로 뒤에 그 하위 업무. 상위를 볼 수 없는 하위 업무는 맨 끝. */
export function listOrder<T extends ListTask>(tasks: readonly T[]): T[] {
  const ids = new Set(tasks.map((t) => t.id));
  const out: T[] = [];
  for (const top of tasks) {
    if (top.parent_id !== null) continue;
    out.push(top, ...tasks.filter((t) => t.parent_id === top.id));
  }
  out.push(...tasks.filter((t) => t.parent_id !== null && !ids.has(t.parent_id)));
  return out;
}
