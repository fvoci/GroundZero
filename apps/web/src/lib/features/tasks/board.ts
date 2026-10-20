// 칸반 계산 (D-31). 열은 상태, 열 안의 순서는 서버 목록 순서(위치)를 따른다.
import type { TaskStatus } from '$lib/api/client';
import { ApiError, NetworkError, TASK_STATUSES } from '$lib/api/client';
import { m } from '$lib/paraglide/messages.js';

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

/** 끌어 놓을 자리. `beforeId`가 null이면 열의 맨 끝. */
export interface DropSlot {
  status: TaskStatus;
  beforeId: string | null;
}

export interface CardBox {
  id: string;
  top: number;
  height: number;
}

/**
 * 열 안에서 포인터 높이 `y`가 들어갈 자리: 세로 가운데가 `y`보다 아래인 첫 카드 앞(끄는 카드는 뺀다).
 * 카드는 화면 위에서 아래 순서로 준다.
 */
export function dropBefore(cards: readonly CardBox[], draggedId: string, y: number): string | null {
  for (const c of cards) {
    if (c.id === draggedId) continue;
    if (y < c.top + c.height / 2) return c.id;
  }
  return null;
}

/**
 * 놓은 자리를 이동 요청(열, 바로 뒤 업무)으로 바꾼다. 제자리면 undefined.
 * `ordered`가 아니면(기본 순서가 아닌 정렬) 순서는 바꾸지 않고 다른 열의 끝으로만 옮긴다.
 */
export function planDrop(
  tasks: readonly BoardTask[],
  id: string,
  slot: DropSlot,
  ordered: boolean,
): BoardMove | undefined {
  const task = tasks.find((t) => t.id === id);
  if (task === undefined) return undefined;
  if (!ordered) {
    return slot.status === task.status ? undefined : { status: slot.status, beforeId: null };
  }
  if (slot.beforeId === id) return undefined;
  if (slot.status === task.status) {
    const lane = lanes(tasks)[task.status];
    const next = lane[lane.findIndex((t) => t.id === id) + 1];
    if ((next?.id ?? null) === slot.beforeId) return undefined;
  }
  return { status: slot.status, beforeId: slot.beforeId };
}

/**
 * 이동을 화면 배열에 먼저 반영한다(서버 응답 전). 서버의 열 안 순서와 같게 `beforeId` 바로 앞에, 없으면 그 열의
 * 마지막 업무 뒤에 둔다. 실패하면 호출한 쪽이 이전 배열로 되돌린다.
 */
export function applyMove<T extends BoardTask>(
  tasks: readonly T[],
  id: string,
  move: BoardMove,
): T[] {
  const task = tasks.find((t) => t.id === id);
  if (task === undefined) return [...tasks];
  const moved: T = { ...task, status: move.status };
  const rest = tasks.filter((t) => t.id !== id);
  let at = move.beforeId === null ? -1 : rest.findIndex((t) => t.id === move.beforeId);
  if (at === -1) {
    at = rest.length;
    for (let i = rest.length - 1; i >= 0; i -= 1) {
      if (rest[i]?.status === move.status) {
        at = i + 1;
        break;
      }
    }
  }
  return [...rest.slice(0, at), moved, ...rest.slice(at)];
}

/** 옮기기 실패 안내. 화면은 이미 되돌렸고 최신 목록을 다시 읽는다. */
export function moveErrorMessage(error: unknown): string {
  if (error instanceof NetworkError) return m.tasks_move_error_network();
  if (error instanceof ApiError) {
    switch (error.status) {
      case 400:
        return m.tasks_move_error_stale();
      case 403:
        return m.tasks_move_error_forbidden();
      case 404:
        return m.tasks_move_error_gone();
      case 409:
        return m.tasks_move_error_archived();
      default:
        // 서버 쪽 실패(5xx)는 client.ts가 만든 안전한 문구(요청 ID 포함)를 그대로 보인다.
        return error.status >= 500 ? error.message : m.tasks_move_error_other();
    }
  }
  return m.tasks_move_error_other();
}

/** 열의 예상 시간 합계(D-31 예상 시간, 분). 예상이 없는 카드는 더하지 않고 따로 센다. */
export interface LaneEstimate {
  minutes: number;
  /** 예상 시간이 없는 카드 수. */
  missing: number;
  count: number;
}

export function laneEstimate(tasks: readonly { estimate_minutes: number | null }[]): LaneEstimate {
  let minutes = 0;
  let missing = 0;
  for (const t of tasks) {
    if (t.estimate_minutes === null) missing += 1;
    else minutes += t.estimate_minutes;
  }
  return { minutes, missing, count: tasks.length };
}
