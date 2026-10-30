import { afterEach, expect, it, vi } from 'vitest';

import type { MathRenderer } from '$lib/features/documents/katex-loader';

// The stylesheet URL/load is a fixture; keep the real loader and global guard.
vi.mock('../../src/styles/katex-layout.css?url', () => ({ default: '/assets/katex-layout.css' }));

const cleanups: (() => Promise<void>)[] = [];

afterEach(async () => {
  for (const cleanup of cleanups.splice(0)) await cleanup();
  vi.doUnmock('katex');
  vi.resetModules();
  sessionStorage.clear();
});

async function pendingImport() {
  vi.resetModules();
  let finish: (value: { default: MathRenderer }) => void = () => undefined;
  const module = new Promise<{ default: MathRenderer }>((resolve) => {
    finish = resolve;
  });
  const started = vi.fn();
  vi.doMock('katex', () => {
    started();
    return module;
  });
  // Import both after resetting modules so the loader and guard share a registry.
  const { watchPreloadErrors, PRELOAD_RELOAD_KEY } =
    await import('$lib/features/common/preload-reload');
  const { loadKatex } = await import('$lib/features/documents/katex-loader');
  const reload = vi.fn();
  const ready = vi.fn(() => true);
  const stop = watchPreloadErrors({ reload, ready });
  const loading = loadKatex();
  const style = document.head.querySelector('link[href$="/assets/katex-layout.css"]');
  const complete = (): void => {
    finish({ default: { render: vi.fn() } });
  };
  cleanups.push(async () => {
    complete();
    try {
      await loading;
    } finally {
      stop();
      style?.remove();
    }
  });
  if (!(style instanceof HTMLLinkElement)) throw new Error('missing math stylesheet fixture');
  style.dispatchEvent(new Event('load'));
  await vi.waitFor(() => {
    expect(started).toHaveBeenCalledTimes(1);
  });
  return { complete, loading, reload, ready, marker: PRELOAD_RELOAD_KEY };
}

function preloadError(message: string): boolean {
  const event = Object.assign(new Event('vite:preloadError', { cancelable: true }), {
    payload: new Error(message),
  });
  window.dispatchEvent(event);
  return event.defaultPrevented;
}

it('handles a URL-less error locally while the KaTeX import is pending', async () => {
  const { reload, ready, marker } = await pendingImport();
  expect(preloadError('Importing a module script failed')).toBe(true);
  expect(reload).not.toHaveBeenCalled();
  expect(ready).not.toHaveBeenCalled();
  expect(sessionStorage.getItem(marker)).toBeNull();
});

it('keeps URL-less errors on the global path after the import completes', async () => {
  const { complete, loading, reload, ready, marker } = await pendingImport();
  complete();
  await loading;
  expect(preloadError('Importing a module script failed')).toBe(true);
  expect(reload).toHaveBeenCalledTimes(1);
  expect(ready).toHaveBeenCalledTimes(1);
  expect(sessionStorage.getItem(marker)).not.toBeNull();
});

it('keeps another chunk URL on the global path while KaTeX is pending', async () => {
  const { reload, ready, marker } = await pendingImport();
  const url = new URL('/assets/other-chunk.js', location.href).href;
  expect(preloadError(`Failed to fetch dynamically imported module: ${url}`)).toBe(true);
  expect(reload).toHaveBeenCalledTimes(1);
  expect(ready).toHaveBeenCalledTimes(1);
  expect(sessionStorage.getItem(marker)).not.toBeNull();
});
