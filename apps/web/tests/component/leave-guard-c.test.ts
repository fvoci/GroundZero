import { flushSync } from 'svelte';
import { afterEach, expect, it, vi } from 'vitest';
import { createLeaveGuard } from '$lib/features/common/leave-guard.svelte';
import { router } from '$lib/router.svelte';

afterEach(() => {
  router.guard = undefined;
});

it('does not resurrect a detached sibling when children unmount out of order', () => {
  const previous = vi.fn(() => true);
  router.guard = previous;
  const first = createLeaveGuard({ dirty: () => true });
  const second = createLeaveGuard({ dirty: () => false });
  const detachFirst = first.attach();
  const detachSecond = second.attach();
  detachFirst();
  expect(router.guard(vi.fn())).toBe(true);
  expect(first.open).toBe(false);
  detachSecond();
  expect(router.guard).toBe(previous);
});

it('asks each dirty sibling once and resumes only after both accept', () => {
  const first = createLeaveGuard({ dirty: () => true });
  const second = createLeaveGuard({ dirty: () => true });
  const detachFirst = first.attach();
  const detachSecond = second.attach();
  const move = vi.fn();
  router.confirmLeave(move);
  expect(second.open).toBe(true);
  second.discard();
  expect(first.open).toBe(true);
  expect(move).not.toHaveBeenCalled();
  first.discard();
  expect(move).toHaveBeenCalledOnce();
  expect(first.open).toBe(false);
  expect(second.open).toBe(false);
  detachFirst();
  detachSecond();
});

it('shares discard/keep, busy and stale-session protection', () => {
  let dirty = true;
  let busy = false;
  let session = 1;
  const guard = createLeaveGuard({ dirty: () => dirty, busy: () => busy, identity: () => session });
  const close = vi.fn();
  guard.request(close);
  expect(guard.open).toBe(true);
  guard.keep();
  expect(close).not.toHaveBeenCalled();
  guard.request(close);
  session += 1;
  guard.discard();
  expect(close).not.toHaveBeenCalled();
  guard.request(close);
  busy = true;
  guard.discard();
  expect(guard.open).toBe(true);
  busy = false;
  guard.discard();
  expect(close).toHaveBeenCalledTimes(1);
  dirty = false;
  guard.request(close);
  expect(close).toHaveBeenCalledTimes(2);
});

it('guards navigation and unload and restores the previous router guard on cleanup', () => {
  const previous = vi.fn(() => true);
  router.guard = previous;
  const guard = createLeaveGuard({ dirty: () => true });
  const detach = guard.attach();
  const go = vi.fn();
  expect(router.guard(go)).toBe(false);
  flushSync(() => {
    guard.discard();
  });
  expect(go).toHaveBeenCalledOnce();
  const unload = new Event('beforeunload', { cancelable: true });
  window.dispatchEvent(unload);
  expect(unload.defaultPrevented).toBe(true);
  detach();
  expect(router.guard).toBe(previous);
});
