import { readSessionGeneration } from '$lib/features/auth/session-signal';

let taskGeneration = 0;
let observedGeneration: number | undefined;
let publicationOwner: symbol | undefined;

/** Foreign hints invalidate reads; our own notification of an accepted /me does not. */
export function readTaskSessionGeneration(): number {
  const generation = readSessionGeneration();
  if (generation !== observedGeneration) {
    observedGeneration = generation;
    taskGeneration += 1;
  }
  return taskGeneration;
}

export function trackTaskSessionPublications(): {
  published(before: number, after: number): void;
  dispose(): void;
} {
  const owner = Symbol();
  publicationOwner = owner;
  observedGeneration = readSessionGeneration();
  taskGeneration += 1;
  return {
    published(before, after) {
      if (
        publicationOwner === owner &&
        observedGeneration === before &&
        readSessionGeneration() === after
      )
        observedGeneration = after;
    },
    dispose() {
      if (publicationOwner !== owner) return;
      publicationOwner = undefined;
      observedGeneration = readSessionGeneration();
      taskGeneration += 1;
    },
  };
}

/** D-96: an early read belongs to one session, route and request, including A → B → A. */
export interface ContextCapture {
  userId: string;
  sequence: number;
  navigationGeneration: number;
  sessionGeneration: number;
  safetyGeneration: number;
  path: string;
}

export type ContextResult<T> =
  { kind: 'loaded'; value: T } | { kind: 'failed'; error: Error } | { kind: 'discarded' };

export interface CapturedRead<T> {
  take(): Promise<T>;
}

export class DiscardedTaskRead extends Error {
  constructor() {
    super('Task screen read no longer current');
  }
}

export function sameContext(captured: ContextCapture, now: ContextCapture): boolean {
  return (
    captured.userId === now.userId &&
    captured.sequence === now.sequence &&
    captured.navigationGeneration === now.navigationGeneration &&
    captured.sessionGeneration === now.sessionGeneration &&
    captured.safetyGeneration === now.safetyGeneration &&
    captured.path === now.path
  );
}

/** Start now, handle rejection immediately, and recheck ownership when consumed once. */
export function startCapturedRead<T extends { viewer_id: string }>(
  captured: ContextCapture,
  current: () => ContextCapture,
  pending: Promise<T>,
): CapturedRead<T> {
  const snapshot = { ...captured };
  const settled = createContextRead(snapshot, current, pending);
  let consumed = false;
  return {
    async take() {
      if (consumed) throw new Error('Task screen read already consumed');
      consumed = true;
      const first = await settled;
      if (first.kind === 'discarded') throw new DiscardedTaskRead();
      const checked = await createContextRead(
        snapshot,
        current,
        first.kind === 'loaded' ? Promise.resolve(first.value) : Promise.reject<T>(first.error),
      );
      if (checked.kind === 'loaded') return checked.value;
      if (checked.kind === 'failed') throw checked.error;
      throw new DiscardedTaskRead();
    },
  };
}

export async function createContextRead<T extends { viewer_id: string }>(
  captured: ContextCapture,
  current: () => ContextCapture,
  pending: Promise<T>,
): Promise<ContextResult<T>> {
  const matches = (): boolean => {
    return sameContext(captured, current());
  };
  try {
    const value = await pending;
    return matches() && value.viewer_id === captured.userId
      ? { kind: 'loaded', value }
      : { kind: 'discarded' };
  } catch (error: unknown) {
    const cause =
      error instanceof Error ? error : new Error('Task context read failed', { cause: error });
    return matches() ? { kind: 'failed', error: cause } : { kind: 'discarded' };
  }
}
