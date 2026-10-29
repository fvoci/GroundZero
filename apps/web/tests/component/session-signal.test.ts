import { afterEach, beforeEach, expect, it } from 'vitest';

import {
  publishLogoutPending,
  publishLogoutResult,
  publishSessionCheck,
  readLogoutPending,
  readSessionGeneration,
} from '$lib/features/auth/session-signal';

const key = 'fvoci:auth-session';
const queued: (() => void)[] = [];
let originalLocks: PropertyDescriptor | undefined;

beforeEach(() => {
  localStorage.removeItem(key);
  localStorage.removeItem(`${key}:recheck`);
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
  localStorage.removeItem(`${key}:recheck`);
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
  publishLogoutPending('attempt-a');
  const stale = publishSessionCheck(observed);
  const pending = localStorage.getItem(key);
  expect(readSessionGeneration()).toBeGreaterThan(observed);
  grant();
  expect(await stale).toBe(false);
  expect(localStorage.getItem(key)).toBe(pending);
});

it('preserves the current attempt when an older failure reaches the lock later', async () => {
  publishLogoutPending('attempt-a');
  const generation = readSessionGeneration();
  publishLogoutPending('attempt-b');
  const failure = publishLogoutResult('failure', 'attempt-a');
  expect(localStorage.getItem(`${key}:recheck`)).not.toBeNull();
  const pending = localStorage.getItem(key);
  expect(readSessionGeneration()).toBeGreaterThan(generation);
  grant();
  await failure;
  expect(localStorage.getItem(key)).toBe(pending);
});

it('makes completion newer than both attempts and rejects their old response generation', async () => {
  publishLogoutPending('attempt-a');
  const observed = readSessionGeneration();
  publishLogoutPending('attempt-b');
  const otherGeneration = readSessionGeneration();
  const success = publishLogoutResult('success', 'attempt-a');
  await success;
  const final = localStorage.getItem(key);
  expect(readSessionGeneration()).toBeGreaterThan(otherGeneration);
  const stale = publishSessionCheck(observed);
  grant();
  expect(await stale).toBe(false);
  expect(localStorage.getItem(key)).toBe(final);
});

it.each(['missing', 'rejected', 'throws'])(
  'skips conditional writes when Web Locks is %s but still publishes logout recheck signals',
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
    expect.soft(await publishSessionCheck(readSessionGeneration())).toBe(false);
    expect.soft(localStorage.getItem(key)).toBeNull();
    publishLogoutPending('attempt-a');
    const pending = localStorage.getItem(key);
    expect(pending).toContain('logout-pending');
    await publishLogoutResult('failure', 'attempt-a');
    expect.soft(localStorage.getItem(key)).toBe(pending);
    expect.soft(localStorage.getItem(`${key}:recheck`)).not.toBeNull();
    await publishLogoutResult('success', 'attempt-a');
    const final: unknown = JSON.parse(localStorage.getItem(key) ?? '{}');
    expect(final).toMatchObject({ state: 'settled' });
  },
);

it('publishes fresh opaque change tokens without comparing identities', async () => {
  const first = publishSessionCheck(readSessionGeneration());
  grant();
  expect(await first).toBe(true);
  const marker = localStorage.getItem(key);
  const next = publishSessionCheck(readSessionGeneration());
  grant();
  expect(await next).toBe(true);
  expect(localStorage.getItem(key)).not.toBe(marker);
});

it('SR-242-2 stores only opaque tokens and attempt metadata in every marker kind', async () => {
  const accountId = '04f0af2d-786f-4e65-9ad0-6a523b564c13';
  const check = (fields: string[]): void => {
    const raw = localStorage.getItem(key) ?? '{}';
    const marker: unknown = JSON.parse(raw);
    expect(typeof marker).toBe('object');
    if (marker === null || typeof marker !== 'object') throw new Error('missing marker');
    expect(Object.keys(marker).sort()).toEqual(fields.sort());
    expect(raw).not.toContain(accountId);
  };
  const settledFields = ['state', 'generation', 'sequence'];
  publishLogoutPending('opaque-attempt');
  check([...settledFields, 'attemptId', 'startedAt']);
  const failure = publishLogoutResult('failure', 'opaque-attempt');
  grant();
  await failure;
  check(settledFields);
  await publishLogoutResult('success', 'opaque-attempt');
  check(settledFields);
  const verified = publishSessionCheck(readSessionGeneration());
  grant();
  await verified;
  check(settledFields);
});

it.each(['settled', 'logout-pending'])(
  'SR-242-2 removes legacy account metadata from a stored %s marker',
  (state) => {
    const startedAt = Date.now();
    localStorage.setItem(
      key,
      JSON.stringify({
        state,
        userId: '04f0af2d-786f-4e65-9ad0-6a523b564c13',
        attemptId: 'opaque-attempt',
        startedAt,
        generation: 12,
      }),
    );
    if (state === 'logout-pending')
      expect(readLogoutPending()).toEqual({ attemptId: 'opaque-attempt', startedAt });
    expect(readSessionGeneration()).toBe(12);
    const marker: unknown = JSON.parse(localStorage.getItem(key) ?? '{}');
    expect(marker).not.toHaveProperty('userId');
    expect(marker).toMatchObject({ state, generation: 12 });
  },
);
