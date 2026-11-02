import { expect, it, vi } from 'vitest';
import { DiscardedTaskRead, type ContextCapture } from '$lib/features/tasks/context-read';
import {
  createTaskSessionRecovery,
  readTaskWithRecovery,
  type TaskSessionProof,
} from '$lib/features/tasks/session-recovery';

function held<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((ok) => {
    resolve = ok;
  });
  return { promise, resolve };
}
const capture = (): ContextCapture => ({
  userId: 'A',
  sequence: 1,
  navigationGeneration: 1,
  sessionGeneration: 2,
  safetyGeneration: 3,
  path: '/tasks',
});
const proof = (now: ContextCapture): TaskSessionProof => ({
  userId: now.userId,
  sessionGeneration: now.sessionGeneration,
  safetyGeneration: now.safetyGeneration,
});

it('shares one authority check for concurrent sections of the same user', async () => {
  const authority = held<TaskSessionProof | undefined>();
  const verify = vi.fn(() => authority.promise);
  const recovery = createTaskSessionRecovery(verify);
  const first = recovery.confirm('A');
  const second = recovery.confirm('A');
  expect(first).toBe(second);
  authority.resolve(proof(capture()));
  expect(await first).toEqual(proof(capture()));
  expect(verify).toHaveBeenCalledOnce();
});

it('waits for authority before starting a fresh read from an expired seed', async () => {
  const now = capture();
  const authority = held<TaskSessionProof | undefined>();
  const confirm = vi.fn(() => authority.promise);
  const read = vi.fn(() => Promise.resolve({ viewer_id: 'A' }));
  const result = readTaskWithRecovery(
    now,
    () => now,
    () => true,
    { confirm },
    read,
    Promise.reject(new DiscardedTaskRead()),
  );
  await vi.waitFor(() => {
    expect(confirm).toHaveBeenCalledOnce();
  });
  expect(read).not.toHaveBeenCalled();
  authority.resolve(proof(now));
  expect((await result).value).toEqual({ viewer_id: 'A' });
  expect(read).toHaveBeenCalledOnce();
});

it.each(['userId', 'sequence', 'navigationGeneration', 'path'] as const)(
  'does not retry if %s changes during the authority check',
  async (field) => {
    let now = capture();
    const first = { ...now };
    const authority = held<TaskSessionProof | undefined>();
    const confirm = vi.fn(() => authority.promise);
    const read = vi.fn(() => Promise.resolve({ viewer_id: 'A' }));
    const result = readTaskWithRecovery(
      first,
      () => now,
      () => true,
      { confirm },
      read,
      Promise.reject(new DiscardedTaskRead()),
    );
    const rejected = expect(result).rejects.toBeInstanceOf(DiscardedTaskRead);
    await vi.waitFor(() => {
      expect(confirm).toHaveBeenCalledOnce();
    });
    now = { ...now, [field]: typeof now[field] === 'number' ? 99 : 'other' };
    authority.resolve(proof(first));
    await rejected;
    expect(read).not.toHaveBeenCalled();
  },
);

it.each([undefined, { ...proof(capture()), userId: 'B' }])(
  'does not read for an unconfirmed or different user',
  async (checked) => {
    const now = capture();
    const read = vi.fn(() => Promise.resolve({ viewer_id: 'A' }));
    await expect(
      readTaskWithRecovery(
        now,
        () => now,
        () => true,
        { confirm: () => Promise.resolve(checked) },
        read,
        Promise.reject(new DiscardedTaskRead()),
      ),
    ).rejects.toBeInstanceOf(DiscardedTaskRead);
    expect(read).not.toHaveBeenCalled();
  },
);

it('rechecks a proof made stale just before it is consumed', async () => {
  const first = capture();
  const now = { ...first, sessionGeneration: 4 };
  const confirm = vi
    .fn<() => Promise<TaskSessionProof | undefined>>()
    .mockResolvedValueOnce(proof(first))
    .mockResolvedValue(proof(now));
  const read = vi.fn(() => Promise.resolve({ viewer_id: 'A' }));
  const result = await readTaskWithRecovery(
    now,
    () => now,
    () => true,
    { confirm },
    read,
    Promise.reject(new DiscardedTaskRead()),
  );
  expect(result.captured.sessionGeneration).toBe(4);
  expect(confirm).toHaveBeenCalledTimes(2);
  expect(read).toHaveBeenCalledOnce();
});

it('does not retry more than once for the same verified epoch', async () => {
  const now = capture();
  const read = vi.fn(() => Promise.reject<{ viewer_id: string }>(new DiscardedTaskRead()));
  await expect(
    readTaskWithRecovery(
      now,
      () => now,
      () => true,
      { confirm: () => Promise.resolve(proof(now)) },
      read,
      Promise.reject(new DiscardedTaskRead()),
    ),
  ).rejects.toBeInstanceOf(DiscardedTaskRead);
  expect(read).toHaveBeenCalledOnce();
});

it('bounds unstable authority without issuing a fresh GET', async () => {
  const now = capture();
  const confirm = vi.fn(() => Promise.resolve({ ...proof(now), sessionGeneration: -1 }));
  const read = vi.fn(() => Promise.resolve({ viewer_id: 'A' }));
  await expect(
    readTaskWithRecovery(
      now,
      () => now,
      () => true,
      { confirm },
      read,
      Promise.reject(new DiscardedTaskRead()),
    ),
  ).rejects.toThrow('verification did not settle');
  expect(confirm).toHaveBeenCalledTimes(3);
  expect(read).not.toHaveBeenCalled();
});

it('does not spend the safety retry budget on distinct verified notification epochs', async () => {
  let now = capture();
  let reads = 0;
  const read = vi.fn(() => {
    reads += 1;
    if (reads <= 5) {
      now = { ...now, sessionGeneration: now.sessionGeneration + 1 };
      return Promise.reject<{ viewer_id: string }>(new DiscardedTaskRead());
    }
    return Promise.resolve({ viewer_id: 'A' });
  });
  const result = await readTaskWithRecovery(
    now,
    () => now,
    () => true,
    { confirm: () => Promise.resolve(proof(now)) },
    read,
    Promise.reject(new DiscardedTaskRead()),
  );
  expect(result.value).toEqual({ viewer_id: 'A' });
  expect(read).toHaveBeenCalledTimes(6);
});
