import { afterEach, expect, it, vi } from 'vitest';

import {
  PRELOAD_RELOAD_KEY,
  registerPreloadErrorHandler,
  watchPreloadErrors,
} from '$lib/features/common/preload-reload';

afterEach(() => {
  sessionStorage.clear();
});

function failure(error: Error): boolean {
  const event = Object.assign(new Event('vite:preloadError', { cancelable: true }), {
    payload: error,
  });
  window.dispatchEvent(event);
  return event.defaultPrevented;
}

it('lets a later local handler own feedback without reload, and removes it on disposal', () => {
  const error = new Error('optional renderer');
  const reload = vi.fn();
  const ready = vi.fn(() => true);
  const onStale = vi.fn();
  const stop = watchPreloadErrors({ reload, ready, onStale });
  const local = vi.fn((cause: unknown) => cause === error);
  const dispose = registerPreloadErrorHandler(local);
  try {
    expect(failure(error)).toBe(true);
    expect(local).toHaveBeenCalledWith(error);
    expect(reload).not.toHaveBeenCalled();
    expect(ready).not.toHaveBeenCalled();
    expect(onStale).not.toHaveBeenCalled();
    expect(sessionStorage.getItem(PRELOAD_RELOAD_KEY)).toBeNull();
    dispose();
    expect(failure(error)).toBe(true);
    expect(reload).toHaveBeenCalledTimes(1);
    expect(ready).toHaveBeenCalledTimes(1);
  } finally {
    dispose();
    stop();
  }
});

it('keeps the original reload path for an unhandled chunk', () => {
  const reload = vi.fn();
  const stop = watchPreloadErrors({ reload });
  const dispose = registerPreloadErrorHandler(() => false);
  try {
    expect(failure(new Error('another chunk'))).toBe(true);
    expect(reload).toHaveBeenCalledTimes(1);
    expect(sessionStorage.getItem(PRELOAD_RELOAD_KEY)).not.toBeNull();
  } finally {
    dispose();
    stop();
  }
});
