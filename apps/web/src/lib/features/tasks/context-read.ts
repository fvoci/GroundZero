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
      if (first.kind === 'discarded') throw new Error('Task screen read no longer current');
      const checked = await createContextRead(
        snapshot,
        current,
        first.kind === 'loaded' ? Promise.resolve(first.value) : Promise.reject<T>(first.error),
      );
      if (checked.kind === 'loaded') return checked.value;
      if (checked.kind === 'failed') throw checked.error;
      throw new Error('Task screen read no longer current');
    },
  };
}

export async function createContextRead<T extends { viewer_id: string }>(
  captured: ContextCapture,
  current: () => ContextCapture,
  pending: Promise<T>,
): Promise<ContextResult<T>> {
  const matches = (): boolean => {
    const now = current();
    return (
      captured.userId === now.userId &&
      captured.sequence === now.sequence &&
      captured.navigationGeneration === now.navigationGeneration &&
      captured.sessionGeneration === now.sessionGeneration &&
      captured.safetyGeneration === now.safetyGeneration &&
      captured.path === now.path
    );
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
