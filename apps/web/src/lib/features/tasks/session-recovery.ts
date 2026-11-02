import { SESSION_CHECK_MAX_ATTEMPTS } from '$lib/features/auth/session-check';
import {
  DiscardedTaskRead,
  sameContext,
  startCapturedRead,
  type ContextCapture,
} from './context-read';

export const TASK_SESSION_RECOVERY = Symbol('task session recovery');
export class TaskSessionUnsettled extends Error {}
export interface TaskSessionProof {
  userId: string;
  sessionGeneration: number;
  safetyGeneration: number;
}
export interface TaskSessionRecovery {
  confirm(userId: string): Promise<TaskSessionProof | undefined>;
}
export interface TaskReadConsumer<T> {
  loaded(value: T, captured: ContextCapture): void;
  failed?(cause: unknown, captured: ContextCapture): void;
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
  consumer?: TaskReadConsumer<T>,
): Promise<{ value: T; captured: ContextCapture }> {
  let snapshot = { ...captured };
  let pending = initial;
  const retriedEpochs = new Set<string>();
  let safetyChanges = 0;
  try {
    for (;;) {
      try {
        const value = await startCapturedRead(snapshot, current, pending ?? read()).take();
        if (!active() || !sameContext(snapshot, current())) throw new DiscardedTaskRead();
        // No await between the final fence and state adoption, including after a recovery.
        consumer?.loaded(value, snapshot);
        return { value, captured: snapshot };
      } catch (cause: unknown) {
        if (
          (!(cause instanceof DiscardedTaskRead) && sameContext(snapshot, current())) ||
          recovery === undefined ||
          !active() ||
          !sameTaskRequest(captured, current())
        )
          throw cause;
        let verified: ContextCapture | undefined;
        let rejectedEpoch: string | undefined;
        let rejectedChecks = 0;
        for (;;) {
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
          const epoch = `${now.sessionGeneration.toString()}:${now.safetyGeneration.toString()}`;
          rejectedChecks = epoch === rejectedEpoch ? rejectedChecks + 1 : 1;
          rejectedEpoch = epoch;
          if (rejectedChecks >= SESSION_CHECK_MAX_ATTEMPTS) break;
        }
        if (verified === undefined)
          throw new TaskSessionUnsettled('Task session verification did not settle', { cause });
        const now = verified;
        if (now.safetyGeneration !== snapshot.safetyGeneration) {
          safetyChanges += 1;
          if (safetyChanges >= SESSION_CHECK_MAX_ATTEMPTS)
            throw new TaskSessionUnsettled('Task safety verification did not settle', { cause });
        }
        const epoch = `${now.sessionGeneration.toString()}:${now.safetyGeneration.toString()}`;
        if (retriedEpochs.has(epoch)) throw new DiscardedTaskRead();
        retriedEpochs.add(epoch);
        snapshot = { ...now };
        pending = undefined;
      }
    }
  } catch (cause: unknown) {
    if (cause instanceof TaskSessionUnsettled) {
      const now = current();
      if (active() && sameTaskRequest(captured, now)) consumer?.failed?.(cause, now);
    } else if (
      !(cause instanceof DiscardedTaskRead) &&
      active() &&
      sameContext(snapshot, current())
    )
      consumer?.failed?.(cause, snapshot);
    throw cause;
  }
}
