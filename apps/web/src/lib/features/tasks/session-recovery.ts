import { SESSION_CHECK_MAX_ATTEMPTS } from '$lib/features/auth/session-check';
import { DiscardedTaskRead, startCapturedRead, type ContextCapture } from './context-read';

export const TASK_SESSION_RECOVERY = Symbol('task session recovery');
export interface TaskSessionProof {
  userId: string;
  sessionGeneration: number;
  safetyGeneration: number;
}
export interface TaskSessionRecovery {
  confirm(userId: string): Promise<TaskSessionProof | undefined>;
}

/** One App owns the authority check; concurrent task sections share its result. */
export function createTaskSessionRecovery(
  verify: TaskSessionRecovery['confirm'],
): TaskSessionRecovery {
  let pending: { userId: string; result: Promise<TaskSessionProof | undefined> } | undefined;
  return {
    confirm(userId) {
      if (pending?.userId === userId) return pending.result;
      const result = Promise.resolve()
        .then(() => verify(userId))
        .catch(() => undefined);
      pending = { userId, result };
      void result.then(() => {
        if (pending?.result === result) pending = undefined;
      });
      return result;
    },
  };
}

export function sameTaskRequest(first: ContextCapture, now: ContextCapture): boolean {
  return (
    first.userId === now.userId &&
    first.sequence === now.sequence &&
    first.navigationGeneration === now.navigationGeneration &&
    first.path === now.path
  );
}

/** Rebase only session epochs, after /me confirms the original user and the request is live. */
export async function readTaskWithRecovery<T extends { viewer_id: string }>(
  captured: ContextCapture,
  current: () => ContextCapture,
  active: () => boolean,
  recovery: TaskSessionRecovery | undefined,
  read: () => Promise<T>,
  initial?: Promise<T>,
): Promise<{ value: T; captured: ContextCapture }> {
  let snapshot = { ...captured };
  let pending = initial;
  const retriedEpochs = new Set<string>();
  for (let attempt = 0; attempt < SESSION_CHECK_MAX_ATTEMPTS; attempt += 1) {
    try {
      const value = await startCapturedRead(snapshot, current, pending ?? read()).take();
      return { value, captured: snapshot };
    } catch (cause: unknown) {
      if (
        !(cause instanceof DiscardedTaskRead) ||
        recovery === undefined ||
        !active() ||
        !sameTaskRequest(captured, current())
      )
        throw cause;
      let verified: ContextCapture | undefined;
      for (let check = 0; check < SESSION_CHECK_MAX_ATTEMPTS; check += 1) {
        const proof = await recovery.confirm(captured.userId);
        const now = current();
        if (
          proof === undefined ||
          !active() ||
          !sameTaskRequest(captured, now) ||
          proof.userId !== captured.userId
        )
          throw new DiscardedTaskRead();
        if (
          proof.sessionGeneration === now.sessionGeneration &&
          proof.safetyGeneration === now.safetyGeneration
        ) {
          verified = { ...now };
          break;
        }
      }
      if (verified === undefined)
        throw new Error('Task session verification did not settle', { cause });
      const now = verified;
      const epoch = `${now.sessionGeneration.toString()}:${now.safetyGeneration.toString()}`;
      if (retriedEpochs.has(epoch)) throw new DiscardedTaskRead();
      retriedEpochs.add(epoch);
      snapshot = { ...now };
      pending = undefined;
    }
  }
  throw new Error('Task screen recovery did not settle');
}
