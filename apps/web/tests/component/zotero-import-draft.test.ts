import './fixtures/authenticated';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { flushSync } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { api, NetworkError } from '$lib/api/client';
import { formDraftTabKey } from '$lib/features/common/form-draft';
import { markAccountSwitch } from '$lib/features/feedback/feedback.svelte';
import ZoteroImport from '$lib/features/records/ZoteroImport.svelte';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';
import { router } from '$lib/router.svelte';

const name = 'zotero-import:item-a';
const imported = vi.fn();
const result = { created: 1, updated: 0, skipped: 0 };
const mount = (itemId = 'item-a') => render(ZoteroImport, { itemId, onimported: imported });
function field(id: string): HTMLInputElement | HTMLSelectElement {
  const element = document.getElementById(`zotero-${id}`);
  if (element instanceof HTMLInputElement || element instanceof HTMLSelectElement) return element;
  throw new Error(`Missing Zotero field ${id}`);
}
const edit = (id: string, value: string) => fireEvent.input(field(id), { target: { value } });
const choose = (id: string, value: string) => fireEvent.change(field(id), { target: { value } });
const open = () => fireEvent.click(screen.getByRole('button', { name: m.records_zotero_import() }));
const read = () =>
  fireEvent.click(screen.getByRole('button', { name: m.records_zotero_read_collections() }));
const run = () => fireEvent.click(screen.getByRole('button', { name: m.records_zotero_run() }));
const keys = () =>
  Object.keys(localStorage).filter((key) => key.includes('form-draft:zotero-import:'));
const values = () =>
  keys()
    .map((key) => localStorage.getItem(key))
    .join('\n');
const restore = () =>
  fireEvent.click(screen.getByRole('button', { name: m.common_draft_restore() }));
function unload(): boolean {
  const event = new Event('beforeunload', { cancelable: true });
  window.dispatchEvent(event);
  return event.defaultPrevented;
}
async function fill(): Promise<void> {
  await choose('type', 'group');
  await edit('id', ' 00123 ');
  await edit('key', 'synthetic-test-only');
}
async function selectCollection(): Promise<void> {
  await read();
  await screen.findByRole('option', { name: 'Collection A' });
  await choose('collection', 'COL-A');
}
beforeEach(async () => {
  await setLocale('ko', { reload: false });
  imported.mockClear();
  vi.spyOn(api, 'zoteroCollections').mockResolvedValue([{ key: 'COL-A', name: 'Collection A' }]);
  vi.spyOn(api, 'zoteroImport').mockResolvedValue(result);
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  router.guard = undefined;
});

it('restores raw ID, type and collection explicitly without retaining the API key or sending requests', async () => {
  mount();
  await open();
  await fill();
  await selectCollection();
  expect(field('id')).toHaveAttribute('inputmode', 'numeric');
  expect(values()).toContain(' 00123 ');
  expect(values()).toContain('COL-A');
  expect(values()).not.toContain('synthetic-test-only');
  expect(values()).not.toMatch(/apiKey|api_key/);
  cleanup();
  mount();
  await screen.findByTestId('form-draft-offer');
  await restore();
  expect(field('id')).toHaveValue(' 00123 ');
  expect(field('type')).toHaveValue('group');
  expect(field('collection')).toHaveValue('COL-A');
  expect(field('key')).toHaveValue('');
  expect(api.zoteroCollections).toHaveBeenCalledOnce();
  expect(api.zoteroImport).not.toHaveBeenCalled();
  await edit('key', 'synthetic-new-key');
  expect(screen.getByRole('button', { name: m.records_zotero_run() })).toBeDisabled();
  await read();
  await waitFor(() =>
    expect(screen.getByRole('button', { name: m.records_zotero_run() })).toBeEnabled(),
  );
  await run();
  await waitFor(() => {
    expect(api.zoteroImport).toHaveBeenCalledWith(
      'item-a',
      {
        library_id: '00123',
        library_type: 'group',
        api_key: 'synthetic-new-key',
      },
      'COL-A',
    );
  });
  await waitFor(() => {
    expect(keys()).toHaveLength(0);
  });
});

it('keeps a missing restored collection until the user explicitly chooses a current option', async () => {
  mount();
  await open();
  await fill();
  await selectCollection();
  cleanup();
  mount();
  await screen.findByTestId('form-draft-offer');
  await restore();
  vi.mocked(api.zoteroCollections).mockResolvedValue([{ key: 'COL-B', name: 'Collection B' }]);
  await edit('key', 'synthetic-test-only');
  await read();
  await screen.findByRole('option', { name: 'Collection B' });
  expect(field('collection')).toHaveValue('COL-A');
  expect(screen.getByRole('button', { name: m.records_zotero_run() })).toBeDisabled();
  await fireEvent.submit(screen.getByRole('form'));
  expect(api.zoteroImport).not.toHaveBeenCalled();
  await choose('collection', 'COL-B');
  await run();
  await waitFor(() => {
    expect(api.zoteroImport).toHaveBeenCalledWith('item-a', expect.anything(), 'COL-B');
  });
});

it('retains failed import input and the original validation and payload contracts', async () => {
  vi.mocked(api.zoteroImport).mockRejectedValue(new NetworkError('offline'));
  mount();
  await open();
  await edit('id', '1e');
  await edit('key', 'synthetic-test-only');
  await run();
  expect(await screen.findByRole('alert')).toHaveTextContent(m.records_zotero_invalid());
  expect(api.zoteroImport).not.toHaveBeenCalled();
  await fill();
  await run();
  await waitFor(() => {
    expect(screen.getByRole('alert')).toHaveTextContent(m.records_zotero_failed());
  });
  expect(field('id')).toHaveValue(' 00123 ');
  expect(api.zoteroImport).toHaveBeenCalledWith(
    'item-a',
    { library_id: '00123', library_type: 'group', api_key: 'synthetic-test-only' },
    null,
  );
  cleanup();
  mount();
  await screen.findByTestId('form-draft-offer');
  await restore();
  expect(field('id')).toHaveValue(' 00123 ');
  expect(field('key')).toHaveValue('');
  expect(api.zoteroImport).toHaveBeenCalledOnce();
});

it('guards close, navigation and unload, preserves on keep and clears only after explicit discard', async () => {
  mount();
  await open();
  await edit('id', '1e');
  expect(unload()).toBe(true);
  await fireEvent.click(screen.getByRole('button', { name: m.records_zotero_close() }));
  await screen.findByRole('alertdialog');
  await fireEvent.click(screen.getByRole('button', { name: m.common_discard_keep() }));
  expect(field('id')).toHaveValue('1e');
  const move = vi.fn();
  router.confirmLeave(move);
  await screen.findByRole('alertdialog');
  await fireEvent.click(screen.getByRole('button', { name: m.common_discard_confirm() }));
  expect(move).toHaveBeenCalledOnce();
  expect(keys()).toHaveLength(0);
  await edit('id', '45');
  await edit('key', 'synthetic-test-only');
  await fireEvent.click(screen.getByRole('button', { name: m.records_zotero_close() }));
  await fireEvent.click(await screen.findByRole('button', { name: m.common_discard_confirm() }));
  await open();
  expect(field('id')).toHaveValue('');
  expect(field('key')).toHaveValue('');
});

it('does not add a leave warning or durable draft for key-only edits', async () => {
  mount();
  await open();
  await edit('key', 'synthetic-test-only');
  expect(unload()).toBe(false);
  expect(keys()).toHaveLength(0);
  await fireEvent.click(screen.getByRole('button', { name: m.records_zotero_close() }));
  expect(screen.queryByRole('alertdialog')).not.toBeInTheDocument();
  await open();
  expect(field('key')).toHaveValue('');
});

it('warns and preserves live input when local storage fails, including account-switch reload', async () => {
  vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
    throw new Error('full');
  });
  mount();
  await open();
  await fill();
  expect(unload()).toBe(true);
  markAccountSwitch();
  expect(unload()).toBe(true);
  await run();
  expect(api.zoteroImport).not.toHaveBeenCalled();
  expect(field('id')).toHaveValue(' 00123 ');
});

it('isolates accounts and item targets and ignores late success from a previous owner', async () => {
  let finish: ((value: typeof result) => void) | undefined;
  const pending = new Promise<typeof result>((resolve) => {
    finish = resolve;
  });
  vi.mocked(api.zoteroImport).mockReturnValue(pending);
  const view = mount();
  await open();
  await fill();
  await run();
  flushSync(() => {
    me.userId = 'user-b';
  });
  expect(field('id')).toHaveValue('');
  expect(field('key')).toHaveValue('');
  await edit('id', '222');
  finish?.(result);
  await pending;
  await waitFor(() =>
    expect(screen.getByRole('button', { name: m.records_zotero_run() })).toBeEnabled(),
  );
  expect(field('id')).toHaveValue('222');
  expect(imported).not.toHaveBeenCalled();
  await view.rerender({ itemId: 'item-b', onimported: imported });
  expect(field('id')).toHaveValue('');
  expect(screen.queryByTestId('form-draft-offer')).not.toBeInTheDocument();
  flushSync(() => {
    me.userId = 'user-a';
  });
  await view.rerender({ itemId: 'item-a', onimported: imported });
  await screen.findByTestId('form-draft-offer');
  await restore();
  expect(field('id')).toHaveValue(' 00123 ');
});

it('preserves additional input typed during an import and completes only the submitted current snapshot', async () => {
  let finish: ((value: typeof result) => void) | undefined;
  vi.mocked(api.zoteroImport).mockImplementation(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  mount();
  await open();
  await fill();
  await run();
  await edit('id', '00999');
  finish?.(result);
  await waitFor(() => {
    expect(imported).toHaveBeenCalledOnce();
  });
  expect(field('id')).toHaveValue('00999');
  expect(values()).toContain('00999');
  expect(unload()).toBe(true);
});

it('keeps other tabs and offered drafts when the current import succeeds', async () => {
  const own = formDraftTabKey(name);
  if (own === undefined) throw new Error('missing owner');
  const other = `${own}-other`;
  const stored = JSON.stringify({
    savedAt: '2026-10-26',
    input: { libraryId: '777', libraryType: 'user', collection: '' },
  });
  localStorage.setItem(other, stored);
  mount();
  await open();
  await fill();
  await run();
  await waitFor(() => {
    expect(imported).toHaveBeenCalledOnce();
  });
  expect(localStorage.getItem(own)).toBeNull();
  expect(localStorage.getItem(other)).toBe(stored);
  expect(screen.getByTestId('form-draft-offer')).toBeInTheDocument();
});

it('ignores late collection results after library edits and account switches', async () => {
  let finish: ((value: { key: string; name: string }[]) => void) | undefined;
  vi.mocked(api.zoteroCollections).mockImplementation(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  mount();
  await open();
  await fill();
  await read();
  await edit('id', '222');
  finish?.([{ key: 'OLD', name: 'Old collection' }]);
  await waitFor(() =>
    expect(screen.getByRole('button', { name: m.records_zotero_run() })).toBeEnabled(),
  );
  expect(screen.queryByRole('option', { name: 'Old collection' })).not.toBeInTheDocument();
  await read();
  flushSync(() => {
    me.userId = 'user-b';
  });
  finish?.([{ key: 'PRIVATE', name: 'Private collection' }]);
  await new Promise<void>((resolve) => setTimeout(resolve, 0));
  expect(screen.queryByRole('option', { name: 'Private collection' })).not.toBeInTheDocument();
});

it('fences late success when account switching starts before user ID changes', async () => {
  let finish: ((value: typeof result) => void) | undefined;
  vi.mocked(api.zoteroImport).mockImplementation(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  mount();
  await open();
  await fill();
  await run();
  markAccountSwitch();
  finish?.(result);
  await new Promise<void>((resolve) => setTimeout(resolve, 0));
  expect(imported).not.toHaveBeenCalled();
  expect(values()).toContain(' 00123 ');
  expect(unload()).toBe(false);
});

it('discards an offered draft explicitly without submitting it', async () => {
  mount();
  await open();
  await fill();
  cleanup();
  mount();
  await screen.findByTestId('form-draft-offer');
  await fireEvent.click(screen.getByRole('button', { name: m.common_draft_discard() }));
  expect(keys()).toHaveLength(0);
  expect(api.zoteroImport).not.toHaveBeenCalled();
});

it('does not let a disposed import request clear a remounted draft', async () => {
  let finish: ((value: typeof result) => void) | undefined;
  vi.mocked(api.zoteroImport).mockImplementation(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  mount();
  await open();
  await fill();
  await run();
  cleanup();
  mount();
  await screen.findByTestId('form-draft-offer');
  await restore();
  await edit('id', '333');
  finish?.(result);
  await new Promise<void>((resolve) => setTimeout(resolve, 0));
  expect(field('id')).toHaveValue('333');
  expect(values()).toContain('333');
  expect(imported).not.toHaveBeenCalled();
});

it('requires explicit revalidation after library or key edits and keeps a selection on lookup failure', async () => {
  mount();
  await open();
  await fill();
  await selectCollection();
  await edit('id', '222');
  expect(field('collection')).toHaveValue('COL-A');
  await fireEvent.submit(screen.getByRole('form'));
  expect(api.zoteroImport).not.toHaveBeenCalled();
  await read();
  await waitFor(() => {
    expect(screen.getByRole('button', { name: m.records_zotero_run() })).toBeEnabled();
  });
  await edit('key', 'synthetic-other-key');
  expect(screen.getByRole('button', { name: m.records_zotero_run() })).toBeDisabled();
  vi.mocked(api.zoteroCollections).mockRejectedValue(new NetworkError('offline'));
  await read();
  await waitFor(() => {
    expect(screen.getByRole('alert')).toHaveTextContent(m.records_zotero_unreachable());
  });
  expect(field('collection')).toHaveValue('COL-A');
  expect(screen.getByRole('button', { name: m.records_zotero_run() })).toBeDisabled();
  expect(values()).toContain('COL-A');
});

it('preserves a type-only edit and ignores a previous account late failure', async () => {
  let fail: ((error: Error) => void) | undefined;
  vi.mocked(api.zoteroImport).mockImplementation(
    () =>
      new Promise((_resolve, reject) => {
        fail = reject;
      }),
  );
  mount();
  await open();
  await choose('type', 'group');
  expect(unload()).toBe(true);
  expect(values()).toContain('group');
  await fill();
  await run();
  flushSync(() => {
    me.userId = 'user-b';
  });
  await edit('id', '444');
  fail?.(new NetworkError('offline'));
  await new Promise<void>((resolve) => setTimeout(resolve, 0));
  expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  expect(field('id')).toHaveValue('444');
  expect(values()).toContain('444');
});
