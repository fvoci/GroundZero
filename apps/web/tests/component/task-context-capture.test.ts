import { describe, expect, it } from 'vitest';
import {
  createContextRead,
  startCapturedRead,
  type ContextCapture,
} from '$lib/features/tasks/context-read';

const capture = (userId = 'A', sequence = 1): ContextCapture => ({
  userId,
  sequence,
  navigationGeneration: 0,
  sessionGeneration: 10,
  safetyGeneration: 20,
  path: '/w/w/i/i/tasks/board',
});

function held<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => (resolve = done));
  return { promise, resolve };
}

describe('D-96 task context ownership', () => {
  it('starts a captured read immediately but rechecks ownership at one-time consumption', async () => {
    let current = capture();
    const read = startCapturedRead(current, () => current, Promise.resolve({ viewer_id: 'A' }));
    await Promise.resolve();
    current = { ...current, navigationGeneration: 2 };
    await expect(read.take()).rejects.toThrow();
    const latest = startCapturedRead(current, () => current, Promise.resolve({ viewer_id: 'A' }));
    expect(await latest.take()).toEqual({ viewer_id: 'A' });
    await expect(latest.take()).rejects.toThrow();
  });
  it.each([
    'userId',
    'sequence',
    'navigationGeneration',
    'sessionGeneration',
    'safetyGeneration',
    'path',
  ] as const)('discards a response when %s changes', async (field) => {
    const first = capture();
    const next = { ...first, [field]: typeof first[field] === 'number' ? 99 : 'changed' };
    const pending = held<{ viewer_id: string }>();
    let current = first;
    const read = createContextRead(first, () => current, pending.promise);
    current = next;
    pending.resolve({ viewer_id: 'A' });
    expect(await read).toEqual({ kind: 'discarded' });
  });

  it('checks the authenticated response viewer independently of the browser capture', async () => {
    const first = capture();
    const result = await createContextRead(first, () => first, Promise.resolve({ viewer_id: 'B' }));
    expect(result).toEqual({ kind: 'discarded' });
  });

  it('does not revive the first A response after A → B → A', async () => {
    let current = capture();
    const old = held<{ viewer_id: string; title: string }>();
    const first = createContextRead(current, () => current, old.promise);
    current = capture('B', 2);
    current = capture('A', 3);
    const latest = await createContextRead(
      current,
      () => current,
      Promise.resolve({ viewer_id: 'A', title: 'current A' }),
    );
    let shown = latest.kind === 'loaded' ? latest.value.title : '';
    old.resolve({ viewer_id: 'A', title: 'discarded A' });
    const stale = await first;
    if (stale.kind === 'loaded') shown = stale.value.title;
    expect(shown).toBe('current A');
    expect(stale).toEqual({ kind: 'discarded' });
  });

  it('handles early rejection and discards it after leaving the task screen', async () => {
    const first = capture();
    let current = first;
    const pending = createContextRead(first, () => current, Promise.reject(new Error('early')));
    current = { ...first, path: '/notes' };
    expect(await pending).toEqual({ kind: 'discarded' });
  });
});
