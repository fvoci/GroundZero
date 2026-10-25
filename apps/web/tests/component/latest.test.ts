import { describe, expect, it } from 'vitest';

import { createLatest } from '$lib/latest';

// 겹치는 다시 읽기: 마지막 요청만 적용하고, 먼저 부른 쪽도 마지막 요청이 끝난 뒤에 돌아간다(CodeRabbit #141 ReceivedInvitations).
describe('createLatest', () => {
  it('applies only the newest run and returns earlier callers after it finished', async () => {
    const latest = createLatest();
    const applied: string[] = [];
    let finishFirst: (() => void) | undefined;
    let finishSecond: (() => void) | undefined;
    const first = latest.run(async (isCurrent) => {
      await new Promise<void>((resolve) => {
        finishFirst = resolve;
      });
      if (isCurrent()) applied.push('first');
    });
    const second = latest.run(async (isCurrent) => {
      await new Promise<void>((resolve) => {
        finishSecond = resolve;
      });
      if (isCurrent()) applied.push('second');
    });
    let firstDone = false;
    void first.then(() => {
      firstDone = true;
    });

    finishFirst?.();
    for (let i = 0; i < 5; i += 1) await Promise.resolve();
    expect(applied).toEqual([]);
    expect(firstDone).toBe(false);

    finishSecond?.();
    await second;
    await first;
    expect(applied).toEqual(['second']);
    expect(firstDone).toBe(true);
  });

  it('passes the newest run failure on to earlier callers', async () => {
    const latest = createLatest();
    let finishFirst: (() => void) | undefined;
    const first = latest.run(
      () =>
        new Promise<void>((resolve) => {
          finishFirst = resolve;
        }),
    );
    const second = latest.run(() => Promise.reject(new Error('offline')));
    await expect(second).rejects.toThrow('offline');
    finishFirst?.();
    await expect(first).rejects.toThrow('offline');
  });
});
