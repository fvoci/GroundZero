import { act, cleanup, render, screen, waitFor } from '@testing-library/svelte';
import { flushSync } from 'svelte';
import { afterEach, expect, it, vi } from 'vitest';

import OfficePreview from '$lib/features/files/OfficePreview.svelte';
import { xlsxSheets } from '$lib/features/files/office';
import { accountSafety, hideSession } from '$lib/features/feedback/feedback.svelte';
import { me } from '$lib/me.svelte';

vi.mock('$lib/features/files/office', async (original) => ({
  ...(await original<typeof import('$lib/features/files/office')>()),
  xlsxSheets: vi.fn(() => Promise.resolve([{ name: 'old sheet', rows: [['old secret']] }])),
}));
afterEach(() => {
  cleanup();
  hideSession('account');
  accountSafety.switching = false;
  me.userId = '';
  vi.restoreAllMocks();
  vi.clearAllMocks();
});

it('aborts office bytes and discards late content when session safety starts', async () => {
  me.userId = 'user-a';
  let finish!: (response: Response) => void;
  let signal: AbortSignal | null | undefined;
  vi.spyOn(globalThis, 'fetch').mockImplementation((_input, init) => {
    signal = init?.signal;
    return new Promise<Response>((resolve) => {
      finish = resolve;
    });
  });
  render(OfficePreview, {
    fileId: 'file',
    name: 'secret.xlsx',
    size: 10,
    kind: 'xlsx',
    onclose: () => undefined,
  });
  await waitFor(() => {
    expect(fetch).toHaveBeenCalledOnce();
  });
  hideSession('restore');
  flushSync();
  expect.soft(signal?.aborted).toBe(true);
  await act(() => {
    finish(new Response('old bytes'));
  });
  expect(xlsxSheets).not.toHaveBeenCalled();
  expect(screen.queryByText('old secret')).toBeNull();
});
