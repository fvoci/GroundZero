import { waitFor } from '@testing-library/svelte';
import { afterEach, expect, it, vi } from 'vitest';

import { printToPdf } from '$lib/features/documents/export-print';

afterEach(() => {
  document.querySelectorAll('iframe').forEach((frame) => {
    frame.remove();
  });
  vi.restoreAllMocks();
});

it('does not print old content when safety aborts while print images are loading', async () => {
  const controller = new AbortController();
  const pending = printToPdf('<p>old secret</p>', controller.signal).catch(() => undefined);
  const frame = document.querySelector('iframe');
  if (!frame?.contentWindow) throw new Error('print frame missing');
  const print = vi.spyOn(frame.contentWindow, 'print').mockImplementation(() => undefined);
  vi.spyOn(frame.contentWindow, 'focus').mockImplementation(() => undefined);
  const image = document.createElement('img');
  let loaded!: () => void;
  const decode = vi.fn(
    () =>
      new Promise<void>((resolve) => {
        loaded = resolve;
      }),
  );
  Object.defineProperty(image, 'complete', { value: false });
  image.decode = decode;
  Object.defineProperty(frame, 'contentDocument', { value: { images: [image] } });
  frame.dispatchEvent(new Event('load'));
  await waitFor(() => {
    expect(decode).toHaveBeenCalledOnce();
  });
  controller.abort();
  loaded();
  await pending;
  expect(print).not.toHaveBeenCalled();
  expect(frame.isConnected).toBe(false);
});
