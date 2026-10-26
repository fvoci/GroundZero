import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { flushSync } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { NetworkError } from '$lib/api/client';
import CreateForm from '$lib/features/workspaces/CreateForm.svelte';
import { formDraftTabKey } from '$lib/features/common/form-draft';
import { me } from '$lib/me.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';
import { router } from '$lib/router.svelte';
import { markAccountSwitch } from '$lib/features/feedback/feedback.svelte';

const create = vi.fn<(id: string, name: string) => Promise<void>>();
const props = {
  id: 'new',
  label: 'Name',
  placeholder: '',
  submitLabel: 'Create',
  draftName: 'test:new:container',
  create,
};
const mount = () => render(CreateForm, { props });
const input = () => screen.getByRole('textbox', { name: 'Name' });
const keys = () =>
  Object.keys(localStorage).filter((key) => key.includes('form-draft:test:new:container:'));

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  me.userId = 'user-a';
  localStorage.clear();
  create.mockReset().mockResolvedValue(undefined);
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  router.guard = undefined;
});

it('offers a failed request on remount and retries only after restore with the same request ID', async () => {
  create.mockRejectedValue(new NetworkError('offline'));
  mount();
  await fireEvent.input(input(), { target: { value: '  남긴 이름  ' } });
  await fireEvent.click(screen.getByRole('button', { name: 'Create' }));
  await waitFor(() => {
    expect(create).toHaveBeenCalledOnce();
  });
  const id = create.mock.calls[0]?.[0];
  cleanup();
  mount();
  await screen.findByTestId('form-draft-offer');
  expect(input()).toHaveValue('');
  expect(create).toHaveBeenCalledOnce();
  await fireEvent.click(screen.getByRole('button', { name: '입력 되살리기' }));
  expect(input()).toHaveValue('  남긴 이름  ');
  create.mockResolvedValue(undefined);
  await fireEvent.click(screen.getByRole('button', { name: 'Create' }));
  await waitFor(() => {
    expect(create).toHaveBeenCalledTimes(2);
  });
  expect(create.mock.calls[1]?.slice(0, 2)).toEqual([id, '남긴 이름']);
  await waitFor(() => {
    expect(keys()).toHaveLength(0);
  });
});

it('keeps offered input intact when typing a different input before deciding', async () => {
  mount();
  await fireEvent.input(input(), { target: { value: 'old' } });
  cleanup();
  mount();
  await screen.findByTestId('form-draft-offer');
  await fireEvent.input(input(), { target: { value: 'new' } });
  expect(keys()).toHaveLength(2);
  expect(keys().some((key) => localStorage.getItem(key)?.includes('old'))).toBe(true);
  await fireEvent.click(screen.getByRole('button', { name: '남긴 입력 버리기' }));
  expect(input()).toHaveValue('new');
  expect(keys()).toHaveLength(1);
});

it('isolates account and target switches and ignores a late success', async () => {
  let finish: (() => void) | undefined;
  create.mockImplementation(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  const view = mount();
  await fireEvent.input(input(), { target: { value: 'A private' } });
  await fireEvent.click(screen.getByRole('button', { name: 'Create' }));
  flushSync(() => {
    me.userId = 'user-b';
  });
  await waitFor(() => expect(input()).toHaveValue(''));
  await fireEvent.input(input(), { target: { value: 'B private' } });
  finish?.();
  await waitFor(() => expect(input()).toHaveValue('B private'));
  expect(
    keys().some(
      (key) => key.includes('user-a') && localStorage.getItem(key)?.includes('A private'),
    ),
  ).toBe(true);
  await view.rerender({ ...props, draftName: 'test:other' });
  expect(input()).toHaveValue('');
  flushSync(() => {
    me.userId = 'user-a';
  });
  await view.rerender(props);
  await screen.findByTestId('form-draft-offer');
  await fireEvent.click(screen.getByRole('button', { name: '입력 되살리기' }));
  expect(input()).toHaveValue('A private');
});

it('guards navigation and unload even when storage is unavailable', async () => {
  vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
    throw new Error('full');
  });
  mount();
  await fireEvent.input(input(), { target: { value: 'unsaved' } });
  const unload = new Event('beforeunload', { cancelable: true });
  window.dispatchEvent(unload);
  expect(unload.defaultPrevented).toBe(true);
  const move = vi.fn();
  router.confirmLeave(move);
  await screen.findByRole('alertdialog');
  await fireEvent.click(screen.getByRole('button', { name: '계속 편집' }));
  expect(input()).toHaveValue('unsaved');
  expect(move).not.toHaveBeenCalled();
  router.confirmLeave(move);
  await fireEvent.click(await screen.findByRole('button', { name: '변경 버리고 닫기' }));
  expect(move).toHaveBeenCalledOnce();
  expect(input()).toHaveValue('');
});

it('leaves drafts in other tabs alone when saving', async () => {
  const key = formDraftTabKey(props.draftName);
  if (key === undefined) throw new Error('missing user');
  mount();
  const other = `${key}-other`;
  localStorage.setItem(
    other,
    JSON.stringify({
      savedAt: '2026-10-26',
      input: { name: 'other tab', requestId: null, context: '' },
    }),
  );
  await fireEvent.input(input(), { target: { value: 'this tab' } });
  await fireEvent.click(screen.getByRole('button', { name: 'Create' }));
  await waitFor(() => expect(input()).toHaveValue(''));
  expect(localStorage.getItem(key)).toBeNull();
  expect(localStorage.getItem(other)).toContain('other tab');
});

it('allows a new input to save while preserving an older offered draft', async () => {
  mount();
  await fireEvent.input(input(), { target: { value: 'offered' } });
  cleanup();
  mount();
  await screen.findByTestId('form-draft-offer');
  await fireEvent.input(input(), { target: { value: 'new input' } });
  await fireEvent.click(screen.getByRole('button', { name: 'Create' }));
  await waitFor(() => {
    expect(create).toHaveBeenCalledOnce();
  });
  expect(input()).toHaveValue('');
  expect(keys()).toHaveLength(1);
  expect(localStorage.getItem(keys()[0] ?? '')).toContain('offered');
});

it('fences account switching before me changes and lets the reload proceed', async () => {
  mount();
  await fireEvent.input(input(), { target: { value: 'original account input' } });
  markAccountSwitch();
  expect(me.userId).toBe('user-a');
  await fireEvent.click(screen.getByRole('button', { name: 'Create' }));
  expect(create).not.toHaveBeenCalled();
  const unload = new Event('beforeunload', { cancelable: true });
  window.dispatchEvent(unload);
  expect(unload.defaultPrevented).toBe(false);
  expect(
    keys().some(
      (key) =>
        key.includes('user-a') && localStorage.getItem(key)?.includes('original account input'),
    ),
  ).toBe(true);
  flushSync(() => {
    me.userId = 'user-b';
  });
  expect(input()).toHaveValue('');
  expect(keys().some((key) => key.includes('user-b'))).toBe(false);
});

it('does not clear the original draft on a late success after account switching begins', async () => {
  let finish: (() => void) | undefined;
  create.mockImplementation(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  mount();
  await fireEvent.input(input(), { target: { value: 'keep original' } });
  await fireEvent.click(screen.getByRole('button', { name: 'Create' }));
  markAccountSwitch();
  finish?.();
  await waitFor(() => {
    expect(keys().some((key) => localStorage.getItem(key)?.includes('keep original'))).toBe(true);
  });
  expect(input()).toHaveValue('keep original');
});

it('never copies same-tick input into the newly selected account key', async () => {
  mount();
  const typing = fireEvent.input(input(), { target: { value: 'belongs to A' } });
  flushSync(() => {
    me.userId = 'user-b';
  });
  await typing;
  expect(input()).toHaveValue('');
  expect(keys().some((key) => key.includes('user-b'))).toBe(false);
});

it('preserves newly typed input as another offer when restoring an older draft', async () => {
  mount();
  await fireEvent.input(input(), { target: { value: 'older' } });
  cleanup();
  mount();
  await screen.findByTestId('form-draft-offer');
  await fireEvent.input(input(), { target: { value: 'newer' } });
  await fireEvent.click(screen.getByRole('button', { name: '입력 되살리기' }));
  expect(input()).toHaveValue('older');
  expect(keys()).toHaveLength(2);
  expect(keys().some((key) => localStorage.getItem(key)?.includes('newer'))).toBe(true);
});

it('allows an explicitly restored draft to save even if storage subsequently fills up', async () => {
  mount();
  await fireEvent.input(input(), { target: { value: 'restore then save' } });
  cleanup();
  mount();
  await screen.findByTestId('form-draft-offer');
  vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
    throw new Error('full');
  });
  await fireEvent.click(screen.getByRole('button', { name: '입력 되살리기' }));
  expect(input()).toHaveValue('restore then save');
  await fireEvent.click(screen.getByRole('button', { name: 'Create' }));
  await waitFor(() => {
    expect(create).toHaveBeenCalledOnce();
  });
  expect(keys()).toHaveLength(0);
});

it('preserves a template and retry ID together and changes the ID for a different template', async () => {
  create.mockRejectedValue(new NetworkError('offline'));
  render(CreateForm, { props: { ...props, draftContext: 'template-a' } });
  await fireEvent.input(input(), { target: { value: 'title' } });
  await fireEvent.click(screen.getByRole('button', { name: 'Create' }));
  await waitFor(() => {
    expect(create).toHaveBeenCalledOnce();
  });
  const first = create.mock.calls[0]?.[0];
  cleanup();
  const view = mount();
  await screen.findByTestId('form-draft-offer');
  await fireEvent.click(screen.getByRole('button', { name: '입력 되살리기' }));
  await fireEvent.click(screen.getByRole('button', { name: 'Create' }));
  await waitFor(() => {
    expect(create).toHaveBeenCalledTimes(2);
  });
  expect(create.mock.calls[1]?.[0]).toBe(first);
  await waitFor(() => expect(screen.getByRole('button', { name: 'Create' })).toBeEnabled());
  await view.rerender({ ...props, draftContext: 'template-b' });
  await fireEvent.click(screen.getByRole('button', { name: 'Create' }));
  await waitFor(() => {
    expect(create).toHaveBeenCalledTimes(3);
  });
  expect(create.mock.calls[2]?.[0]).not.toBe(first);
});
