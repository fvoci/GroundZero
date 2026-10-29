import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { flushSync } from 'svelte';
import { afterEach, expect, it, vi } from 'vitest';

import OfficePreview from '$lib/features/files/OfficePreview.svelte';
import { OFFICE_PREVIEW_MAX_BYTES, xlsxSheets } from '$lib/features/files/office';
import { accountSafety, hideSession } from '$lib/features/feedback/feedback.svelte';
import { me } from '$lib/me.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

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

const props = {
  fileId: 'file',
  name: 'secret.xlsx',
  size: 10,
  kind: 'xlsx',
  onclose: () => undefined,
} as const;

it.each(['content-length', 'no-length', 'underreported-length'])(
  'rejects a larger latest version after reauthentication using %s instead of the old size prop',
  async (caseName) => {
    await setLocale('ko', { reload: false });
    me.userId = 'user-a';
    vi.mocked(xlsxSheets).mockResolvedValue([{ name: 'old sheet', rows: [['old secret']] }]);
    const fetch = vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce(new Response('old bytes'));
    render(OfficePreview, props);
    await screen.findByText('old secret');
    hideSession('reauth');
    flushSync();
    const max = OFFICE_PREVIEW_MAX_BYTES;
    const response =
      caseName === 'content-length'
        ? new Response('small body', { headers: { 'Content-Length': String(max + 1) } })
        : new Response(
            new Uint8Array(max + 1),
            caseName === 'underreported-length' ? { headers: { 'Content-Length': '1' } } : {},
          );
    const read = vi.spyOn(response, 'arrayBuffer');
    fetch.mockResolvedValueOnce(response);
    accountSafety.switching = false;
    accountSafety.restoring = false;
    flushSync();
    expect(await screen.findByRole('alert')).toHaveTextContent('파일이 커서 미리볼 수 없습니다');
    expect(read).toHaveBeenCalledTimes(caseName === 'content-length' ? 0 : 1);
    expect(xlsxSheets).toHaveBeenCalledTimes(1);
    expect(screen.queryByText('old secret')).toBeNull();
  },
);

it('accepts response bytes exactly at the preview limit', async () => {
  me.userId = 'user-a';
  vi.mocked(xlsxSheets).mockResolvedValue([{ name: 'limit sheet', rows: [['at the limit']] }]);
  const max = OFFICE_PREVIEW_MAX_BYTES;
  vi.spyOn(globalThis, 'fetch').mockResolvedValue(
    new Response(new Uint8Array(max), { headers: { 'Content-Length': String(max) } }),
  );
  render(OfficePreview, props);
  await screen.findByText('at the limit');
});

it.each([
  { selected: 2, nextCount: 1, expected: 0 },
  { selected: 1, nextCount: 2, expected: 1 },
])(
  'clamps an out-of-range sheet after reauthentication and preserves a valid selection: %j',
  async ({ selected, nextCount, expected }) => {
    me.userId = 'user-a';
    vi.spyOn(globalThis, 'fetch').mockImplementation(() => Promise.resolve(new Response('bytes')));
    vi.mocked(xlsxSheets)
      .mockResolvedValueOnce(
        [0, 1, 2].map((n) => ({ name: `old ${String(n)}`, rows: [[`old cell ${String(n)}`]] })),
      )
      .mockResolvedValueOnce(
        Array.from({ length: nextCount }, (_, n) => ({
          name: `new ${String(n)}`,
          rows: [[`new cell ${String(n)}`]],
        })),
      );
    render(OfficePreview, props);
    await fireEvent.click(await screen.findByRole('button', { name: `old ${String(selected)}` }));
    await screen.findByText(`old cell ${String(selected)}`);
    hideSession('reauth');
    flushSync();
    accountSafety.switching = false;
    accountSafety.restoring = false;
    flushSync();
    await screen.findByRole('region', { name: `new ${String(expected)}` });
    expect(screen.getByText(`new cell ${String(expected)}`)).toBeVisible();
    expect(fetch).toHaveBeenCalledTimes(2);
  },
);
