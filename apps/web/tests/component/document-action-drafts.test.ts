import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { api, NetworkError } from '$lib/api/client';
import DocumentActions from '$lib/features/documents/DocumentActions.svelte';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';
import { router } from '$lib/router.svelte';
import { ACCESS_DOCUMENT } from '../../stories/fixtures/access';

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  localStorage.clear();
  me.userId = 'user-a';
  vi.spyOn(api, 'shares').mockResolvedValue([]);
  vi.spyOn(api, 'publicLinks').mockResolvedValue([]);
  vi.spyOn(api, 'document').mockResolvedValue(ACCESS_DOCUMENT);
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  me.userId = '';
  localStorage.clear();
});

function mount() {
  return render(DocumentActions, {
    doc: ACCESS_DOCUMENT,
    workspaces: [],
    onchanged: () => undefined,
  });
}

async function openCopy(): Promise<HTMLElement> {
  await fireEvent.click(screen.getByRole('button', { name: m.documents_more_actions() }));
  await fireEvent.click(await screen.findByRole('menuitem', { name: m.documents_copy() }));
  return screen.findByRole('dialog');
}

it('offers a sharing email after remount without sending it and guards closing', async () => {
  const share = vi.spyOn(api, 'share');
  mount();
  await fireEvent.click(screen.getByRole('button', { name: m.documents_share_settings() }));
  await fireEvent.input(await screen.findByLabelText(m.documents_email()), {
    target: { value: 'reader@example.test' },
  });
  cleanup();
  mount();
  await fireEvent.click(screen.getByRole('button', { name: m.documents_share_settings() }));
  expect(await screen.findByLabelText(m.documents_email())).toHaveValue('');
  await fireEvent.click(await screen.findByRole('button', { name: m.common_draft_restore() }));
  expect(screen.getByLabelText(m.documents_email())).toHaveValue('reader@example.test');
  expect(share).not.toHaveBeenCalled();
  await fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
  await screen.findByRole('alertdialog');
  await fireEvent.click(screen.getByRole('button', { name: m.common_discard_keep() }));
  expect(screen.getByLabelText(m.documents_email())).toHaveValue('reader@example.test');
  const unload = new Event('beforeunload', { cancelable: true });
  window.dispatchEvent(unload);
  expect(unload.defaultPrevented).toBe(true);
});

it('retains the default-title copy request ID after a lost response and explicit restore', async () => {
  const copy = vi.spyOn(api, 'copyDocument').mockRejectedValue(new NetworkError('offline'));
  mount();
  const dialog = await openCopy();
  await fireEvent.click(within(dialog).getByRole('button', { name: m.documents_copy() }));
  await screen.findByRole('alert');
  const first = copy.mock.calls[0];
  expect(first).toBeDefined();
  cleanup();
  mount();
  const again = await openCopy();
  expect(copy).toHaveBeenCalledTimes(1);
  await fireEvent.click(await screen.findByRole('button', { name: m.common_draft_restore() }));
  await fireEvent.click(within(again).getByRole('button', { name: m.documents_copy() }));
  await waitFor(() => {
    expect(copy).toHaveBeenCalledTimes(2);
  });
  expect(copy.mock.calls[1]).toEqual(first);
});

it('does not navigate or erase a new title when an old copy completes after unmount', async () => {
  let finish: ((value: typeof ACCESS_DOCUMENT) => void) | undefined;
  const pending = new Promise<typeof ACCESS_DOCUMENT>((resolve) => {
    finish = resolve;
  });
  vi.spyOn(api, 'copyDocument').mockReturnValue(pending);
  const navigate = vi.spyOn(router, 'navigate');
  mount();
  const dialog = await openCopy();
  await fireEvent.click(within(dialog).getByRole('button', { name: m.documents_copy() }));
  cleanup();
  mount();
  await openCopy();
  await fireEvent.input(screen.getByLabelText(m.documents_new_title()), {
    target: { value: 'new unsaved title' },
  });
  if (finish === undefined) throw new Error('missing completion');
  finish({ ...ACCESS_DOCUMENT, id: 'copied' });
  await pending;
  await new Promise<void>((resolve) => setTimeout(resolve, 0));
  expect(navigate).not.toHaveBeenCalled();
  expect(screen.getByLabelText(m.documents_new_title())).toHaveValue('new unsaved title');
});
