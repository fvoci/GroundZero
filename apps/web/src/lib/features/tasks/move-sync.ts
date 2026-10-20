// 칸반 이동의 저장 확인 (D-79·D-82). 화면은 먼저 옮기고(낙관적 이동) 서버 저장 결과로 확정한다.
// 이동 API(`PUT /api/tasks/{id}/position`)에는 요청 id도 버전 조건도 없어 같은 요청을 다시 보내면 그사이 다른 사람이 옮긴
// 것을 덮을 수 있다. 그래서 자동으로 다시 보내지 않고, 결과를 모르면 목록을 다시 읽어 대조한다. 이동은 열이 바뀔 때만
// 업무 판(version)을 올리고 같은 열 안의 순서는 올리지 않으므로, 대조는 열·바로 뒤 업무·판을 함께 본다.
import { ApiError, NetworkError } from '$lib/api/client';

import type { BoardMove, BoardTask } from './board';
import { applyMove, lanes } from './board';

/** 대조에 쓰는 업무 모양(목록 응답의 일부). */
export interface SyncTask extends BoardTask {
  version: number;
}

/** 이동 직전 서버 기준의 자리: 열, 바로 뒤 업무(없으면 열 끝), 판. */
export interface MoveOrigin {
  status: BoardTask['status'];
  nextId: string | null;
  version: number;
}

/** 전체 목록(거르지 않은 서버 순서)에서 업무 바로 뒤의 같은 열 업무. */
export function nextInLane(tasks: readonly BoardTask[], id: string): string | null | undefined {
  const task = tasks.find((t) => t.id === id);
  if (task === undefined) return undefined;
  const lane = lanes(tasks)[task.status];
  return lane[lane.findIndex((t) => t.id === id) + 1]?.id ?? null;
}

export function originOf(tasks: readonly SyncTask[], id: string): MoveOrigin | undefined {
  const task = tasks.find((t) => t.id === id);
  const nextId = nextInLane(tasks, id);
  if (task === undefined || nextId === undefined) return undefined;
  return { status: task.status, nextId, version: task.version };
}

/**
 * 저장 실패의 종류. 'failed'는 서버가 처리하지 않았다고 답한 것(앱 형식의 4xx·5xx 응답, 트랜잭션은 커밋되지 않음),
 * 'uncertain'은 반영됐는지 알 수 없는 것: 응답이 오지 않음, 앞단 게이트웨이가 대신 답함(502·503·504, 시간 초과일 수
 * 있다), 예상하지 못한 응답, 성공 응답을 읽지 못함.
 */
export function classifyMoveError(error: unknown): 'failed' | 'uncertain' {
  if (error instanceof NetworkError) return 'uncertain';
  if (error instanceof ApiError) {
    return error.code === 'unreachable' || error.code === 'unexpected' ? 'uncertain' : 'failed';
  }
  return 'uncertain';
}

/**
 * 다시 읽은 서버 목록으로 이동 결과를 정한다.
 * - 'applied': 업무가 목표 열에서 목표 업무 바로 앞(없으면 열 끝)에 있다.
 * - 'not_applied': 이동 전 자리·판 그대로다(이 요청은 반영되지 않았다).
 * - 'changed': 둘 다 아니다(다른 사람이 옮기거나 고쳤다). 서버 상태를 따른다.
 * - 'gone': 업무가 목록에 없다(지워졌거나 볼 수 없다).
 */
export function reconcileMove(
  server: readonly SyncTask[],
  id: string,
  origin: MoveOrigin,
  plan: BoardMove,
): 'applied' | 'not_applied' | 'changed' | 'gone' {
  const task = server.find((t) => t.id === id);
  const nextId = nextInLane(server, id);
  if (task === undefined || nextId === undefined) return 'gone';
  if (task.status === plan.status && nextId === plan.beforeId) return 'applied';
  if (
    task.status === origin.status &&
    nextId === origin.nextId &&
    task.version === origin.version
  ) {
    return 'not_applied';
  }
  return 'changed';
}

/** 서버 목록 위에 아직 저장을 기다리는 다른 카드들의 이동을 다시 얹는다(다시 읽기가 그 카드들을 되돌리지 않게). */
export function overlayPending<T extends BoardTask>(
  server: readonly T[],
  pending: readonly { id: string; plan: BoardMove }[],
): T[] {
  let out = [...server];
  for (const p of pending) out = applyMove(out, p.id, p.plan);
  return out;
}

/** 다시 시도할 때의 계획: 목표 업무가 아직 그 열에 있으면 그 앞, 아니면 열 끝. */
export function replan(server: readonly BoardTask[], plan: BoardMove): BoardMove {
  const before = plan.beforeId === null ? undefined : server.find((t) => t.id === plan.beforeId);
  return before?.status === plan.status ? plan : { status: plan.status, beforeId: null };
}
