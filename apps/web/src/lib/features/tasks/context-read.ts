/** D-96: an early read belongs to one session, route and request, including A → B → A. */
export interface ContextCapture {
  userId: string;
  sequence: number;
  sessionGeneration: number;
  safetyGeneration: number;
  path: string;
}

export type ContextResult<T> =
  { kind: 'loaded'; value: T } | { kind: 'failed'; error: Error } | { kind: 'discarded' };

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
