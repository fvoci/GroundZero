import { afterEach, beforeEach, expect, it } from 'vitest';

import {
  publishLogoutPending,
  publishLogoutResult,
  publishSessionUser,
  readSessionGeneration,
} from '$lib/features/auth/session-signal';

const key = 'fvoci:auth-session';
const queued: (() => void)[] = [];
let originalLocks: PropertyDescriptor | undefined;

beforeEach(() => {
  localStorage.removeItem(key);
  originalLocks = Object.getOwnPropertyDescriptor(navigator, 'locks');
  Object.defineProperty(navigator, 'locks', {
    configurable: true,
    value: {
      request: (_name: string, update: () => unknown) =>
        new Promise<unknown>((resolve) => {
          queued.push(() => {
            resolve(update());
          });
        }),
    },
  });
});

afterEach(() => {
  queued.length = 0;
  localStorage.removeItem(key);
  if (originalLocks === undefined) Reflect.deleteProperty(navigator, 'locks');
  else Object.defineProperty(navigator, 'locks', originalLocks);
});

function grant(): void {
  const next = queued.shift();
  if (next === undefined) throw new Error('no queued marker update');
  next();
}

it('checks the response generation inside the marker lock after a newer logout wins', async () => {
  const observed = readSessionGeneration();
  const logout = publishLogoutPending('user-a', 'attempt-a');
  const stale = publishSessionUser('user-a', observed);
  grant();
  await logout;
  const pending = localStorage.getItem(key);
  expect(readSessionGeneration()).toBeGreaterThan(observed);
  grant();
  expect(await stale).toBe(false);
  expect(localStorage.getItem(key)).toBe(pending);
});

it('preserves the current attempt when an older failure reaches the lock later', async () => {
  const first = publishLogoutPending('user-a', 'attempt-a');
  const second = publishLogoutPending('user-a', 'attempt-b');
  const failure = publishLogoutResult('user-a', 'attempt-a');
  grant();
  await first;
  const generation = readSessionGeneration();
  grant();
  await second;
  const pending = localStorage.getItem(key);
  expect(readSessionGeneration()).toBeGreaterThan(generation);
  grant();
  await failure;
  expect(localStorage.getItem(key)).toBe(pending);
});

it('makes final anonymity newer than both attempts and rejects their old response generation', async () => {
  const first = publishLogoutPending('user-a', 'attempt-a');
  grant();
  await first;
  const observed = readSessionGeneration();
  const second = publishLogoutPending('user-a', 'attempt-b');
  const success = publishLogoutResult(null, 'attempt-a');
  grant();
  await second;
  const otherGeneration = readSessionGeneration();
  grant();
  await success;
  const final = localStorage.getItem(key);
  expect(readSessionGeneration()).toBeGreaterThan(otherGeneration);
  const stale = publishSessionUser('user-a', observed);
  grant();
  expect(await stale).toBe(false);
  expect(localStorage.getItem(key)).toBe(final);
});

it.each(['missing', 'rejected', 'throws'])(
  'skips conditional writes when Web Locks is %s but still publishes logout hiding signals',
  async (mode) => {
    if (mode === 'missing') Reflect.deleteProperty(navigator, 'locks');
    else
      Object.defineProperty(navigator, 'locks', {
        configurable: true,
        value: {
          request: () => {
            if (mode === 'throws') throw new Error('locks denied');
            return Promise.reject(new Error('locks denied'));
          },
        },
      });
    expect.soft(await publishSessionUser('user-a', readSessionGeneration())).toBe(false);
    expect.soft(await publishSessionUser(null, readSessionGeneration())).toBe(false);
    expect.soft(localStorage.getItem(key)).toBeNull();
    await publishLogoutPending('user-a', 'attempt-a');
    const pending = localStorage.getItem(key);
    expect(pending).toContain('logout-pending');
    await publishLogoutResult('user-a', 'attempt-a');
    expect.soft(localStorage.getItem(key)).toBe(pending);
    await publishLogoutResult(null, 'attempt-a');
    const final: unknown = JSON.parse(localStorage.getItem(key) ?? '{}');
    expect(final).toMatchObject({ state: 'settled', userId: null });
  },
);
