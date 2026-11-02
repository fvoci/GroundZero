import {
  DiscardedTaskRead,
  sameContext,
  startCapturedRead,
  type CapturedRead,
  type ContextCapture,
  type ContextResult,
} from './context-read';
import { taskData } from './detail-data';
import { accountSafety } from '$lib/features/feedback/feedback.svelte';
import { taskScreenApi, type BoardRead, type DetailRead, type TaskContext } from './screen-api';

export interface TaskStartupReads {
  itemId: string;
  board: CapturedRead<BoardRead>;
  detail: { taskId: string; read: CapturedRead<DetailRead> } | undefined;
}
export interface TaskStartupSeed {
  itemId: string;
  navigationGeneration: number;
  result: ContextResult<TaskContext>;
  reads?: TaskStartupReads | undefined;
}

/** Called only after App has verified both the session lists and item context. */
export function startTaskReads(
  captured: ContextCapture,
  current: () => ContextCapture,
  itemId: string,
  context: TaskContext,
  taskId?: string,
): TaskStartupReads {
  const snapshot = { ...captured };
  if (
    accountSafety.switching ||
    accountSafety.restoring ||
    !sameContext(snapshot, current()) ||
    context.viewer_id !== snapshot.userId ||
    context.item.id !== itemId
  )
    throw new DiscardedTaskRead();
  const detail =
    taskId === undefined
      ? undefined
      : {
          taskId,
          read: startCapturedRead(snapshot, current, taskScreenApi.detail(taskId)),
        };
  const board = startCapturedRead(
    snapshot,
    current,
    (async () => {
      if (context.item.access !== 'read' && !context.item.archived)
        await taskData.syncRecurrences(snapshot.userId, itemId).catch(() => 0);
      if (accountSafety.switching || accountSafety.restoring || !sameContext(snapshot, current()))
        throw new DiscardedTaskRead();
      return taskScreenApi.board(itemId);
    })(),
  );
  return { itemId, board, detail };
}
