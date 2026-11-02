import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { createSessionNotificationRetry } from '$lib/features/auth/session-notification-retry';

beforeEach(() => {
  vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
});
afterEach(() => {
  vi.useRealTimers();
});

it('allows two immediate retries, then grows the pause up to two seconds', async () => {
  const retry = createSessionNotificationRetry();
  const lifetime = new AbortController();
  expect(await retry.wait(lifetime.signal)).toBe(true);
  expect(await retry.wait(lifetime.signal)).toBe(true);
  expect(vi.getTimerCount()).toBe(0);
  for (const delay of [250, 500, 1_000, 2_000, 2_000, 2_000]) {
    let complete = false;
    const pending = retry.wait(lifetime.signal).then((proceed) => {
      complete = true;
      return proceed;
    });
    await vi.advanceTimersByTimeAsync(delay - 1);
    expect(complete).toBe(false);
    await vi.advanceTimersByTimeAsync(1);
    expect(await pending).toBe(true);
    expect(vi.getTimerCount()).toBe(0);
  }
});

it('cancels a pending pause and removes its timer on abort', async () => {
  const retry = createSessionNotificationRetry();
  const lifetime = new AbortController();
  await retry.wait(lifetime.signal);
  await retry.wait(lifetime.signal);
  const pending = retry.wait(lifetime.signal);
  expect(vi.getTimerCount()).toBe(1);
  lifetime.abort();
  expect(await pending).toBe(false);
  expect(vi.getTimerCount()).toBe(0);
  expect(await retry.wait(lifetime.signal)).toBe(false);
  expect(vi.getTimerCount()).toBe(0);
});

it('does not wait or spend another chain’s immediate retry budget', async () => {
  const first = createSessionNotificationRetry();
  const second = createSessionNotificationRetry();
  const lifetime = new AbortController();
  await first.wait(lifetime.signal);
  await first.wait(lifetime.signal);
  const paused = first.wait(lifetime.signal);
  expect(await second.wait(lifetime.signal)).toBe(true);
  expect(await second.wait(lifetime.signal)).toBe(true);
  expect(vi.getTimerCount()).toBe(1);
  lifetime.abort();
  expect(await paused).toBe(false);
});

it('returns immediately for an already aborted lifetime', async () => {
  const lifetime = new AbortController();
  lifetime.abort();
  expect(await createSessionNotificationRetry().wait(lifetime.signal)).toBe(false);
  expect(vi.getTimerCount()).toBe(0);
});
