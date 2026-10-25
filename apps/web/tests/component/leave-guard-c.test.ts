import { flushSync } from 'svelte';
import { afterEach, expect, it, vi } from 'vitest';
import { createLeaveGuard } from '$lib/features/common/leave-guard.svelte';
import { router } from '$lib/router.svelte';

afterEach(() => {
  router.guard = undefined;
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
