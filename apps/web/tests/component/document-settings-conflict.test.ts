import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { api, type DocumentView } from '$lib/api/client';
import { documentsApi } from '$lib/features/documents/api';
import DocumentActions from '$lib/features/documents/DocumentActions.svelte';
import { me } from '$lib/me.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';
import { router } from '$lib/router.svelte';

const initial: DocumentView = {
  id: 'd1',
  workspace_id: 'ws-1',
  item_id: null,
  parent_id: null,
  placement: 'wiki',
  reference: 'WIKI-1',
  title: '문서',
  body: { type: 'doc' },
  schema_version: 1,
  revision: 3,
  updated_at: '2026-10-19T00:00:00Z',
  access: 'owner',
  can_edit: true,
  visibility: 'workspace',
  allow_copy: true,
  can_manage: true,
  can_delete: true,
  is_template: false,
  can_export: true,
};

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  me.userId = 'owner';
  vi.spyOn(api, 'shares').mockResolvedValue([]);
  vi.spyOn(api, 'publicLinks').mockResolvedValue([]);
  vi.spyOn(api, 'document').mockResolvedValue(initial);
});
afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  me.userId = '';
});

function delaySettings() {
  type Reply = Awaited<ReturnType<typeof documentsApi.setSettings>>;
  let finish: (reply: Reply) => void = () => undefined;
  const response = new Promise<Reply>((resolve) => {
    finish = resolve;
  });
  const save = vi.spyOn(documentsApi, 'setSettings').mockReturnValue(response);
  return { save, response, finish };
}

it('guards closing while settings are pending and keeps the selection after a late 409', async () => {
  const pending = delaySettings();
  render(DocumentActions, { props: { doc: initial, workspaces: [], onchanged: () => undefined } });
  await fireEvent.click(screen.getByRole('button', { name: '공유·설정' }));
  const input = await screen.findByRole('checkbox', { name: /^비공개/ });
  await fireEvent.click(input);
  await fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
  await screen.findByRole('alertdialog');
  await fireEvent.click(screen.getByRole('button', { name: '계속 편집' }));
  expect(input).toBeChecked();
  pending.finish({
    kind: 'conflict',
    current: { ...initial, visibility: 'private' },
    changed: [],
    requestId: 'late',
  });
  await pending.response;
  await screen.findByRole('region', { name: '지금과 비교' });
  expect(input).toBeChecked();
  expect(pending.save).toHaveBeenCalledTimes(1);
});

it('guards navigation before a settings reply and preserves the input after cancelling navigation', async () => {
  const pending = delaySettings();
  render(DocumentActions, { props: { doc: initial, workspaces: [], onchanged: () => undefined } });
  await fireEvent.click(screen.getByRole('button', { name: '공유·설정' }));
  const input = await screen.findByRole('checkbox', { name: '볼 수 있는 사람의 복제 허용' });
  await fireEvent.click(input);
  const navigate = vi.fn();
  if (router.guard === undefined) throw new Error('missing navigation guard');
  expect(router.guard(navigate, '/another-document')).toBe(false);
  await screen.findByRole('alertdialog');
  await fireEvent.click(screen.getByRole('button', { name: '계속 편집' }));
  expect(navigate).not.toHaveBeenCalled();
  expect(input).not.toBeChecked();
  pending.finish({
    kind: 'conflict',
    current: { ...initial, allow_copy: false },
    changed: [],
    requestId: 'late-navigation',
  });
  await pending.response;
  await screen.findByRole('region', { name: '지금과 비교' });
  expect(input).not.toBeChecked();
  expect(pending.save).toHaveBeenCalledTimes(1);
});

it('warns before unloading pending settings and clears the warning only after success', async () => {
  const pending = delaySettings();
  render(DocumentActions, { props: { doc: initial, workspaces: [], onchanged: () => undefined } });
  await fireEvent.click(screen.getByRole('button', { name: '공유·설정' }));
  await fireEvent.click(await screen.findByRole('checkbox', { name: /^비공개/ }));
  const before = new Event('beforeunload', { cancelable: true });
  window.dispatchEvent(before);
  expect(before.defaultPrevented).toBe(true);
  pending.finish({ kind: 'saved', document: { ...initial, visibility: 'private' } });
  await pending.response;
  await waitFor(() => {
    const after = new Event('beforeunload', { cancelable: true });
    window.dispatchEvent(after);
    expect(after.defaultPrevented).toBe(false);
  });
});

it('preserves a settings selection after 409, shows current settings and retries only on request', async () => {
  const current = { ...initial, visibility: 'private' as const, allow_copy: false };
  const save = vi
    .spyOn(documentsApi, 'setSettings')
    .mockResolvedValueOnce({ kind: 'conflict', current, changed: [], requestId: 'settings-race' })
    .mockResolvedValue({ kind: 'saved', document: current });
  render(DocumentActions, { props: { doc: initial, workspaces: [], onchanged: () => undefined } });
  await fireEvent.click(screen.getByRole('button', { name: '공유·설정' }));
  const input = await screen.findByRole('checkbox', { name: /^비공개/ });
  await fireEvent.click(input);
  const comparison = await screen.findByRole('region', { name: '지금과 비교' });
  expect(input).toBeChecked();
  expect(within(comparison).getByRole('checkbox', { name: /^비공개/ })).toBeChecked();
  expect(
    within(comparison).getByRole('checkbox', { name: '볼 수 있는 사람의 복제 허용' }),
  ).not.toBeChecked();
  expect(save).toHaveBeenCalledTimes(1);
  expect(save).toHaveBeenLastCalledWith('d1', {
    visibility: 'private',
    previous_visibility: 'workspace',
  });
  await fireEvent.click(screen.getByRole('button', { name: '다시 시도' }));
  await waitFor(() => {
    expect(save).toHaveBeenCalledTimes(2);
  });
  expect(save).toHaveBeenLastCalledWith('d1', {
    visibility: 'private',
    previous_visibility: 'private',
  });
  await waitFor(() => {
    expect(screen.queryByRole('region', { name: '지금과 비교' })).toBeNull();
  });
});

it('warns before discarding a rejected setting without storing it as a durable draft', async () => {
  const current = { ...initial, allow_copy: false };
  vi.spyOn(documentsApi, 'setSettings').mockResolvedValue({
    kind: 'conflict',
    current,
    changed: [],
    requestId: 'race',
  });
  render(DocumentActions, { props: { doc: initial, workspaces: [], onchanged: () => undefined } });
  await fireEvent.click(screen.getByRole('button', { name: '공유·설정' }));
  const storedBefore = Object.entries(window.localStorage);
  await fireEvent.click(
    await screen.findByRole('checkbox', { name: '볼 수 있는 사람의 복제 허용' }),
  );
  await screen.findByRole('region', { name: '지금과 비교' });
  await fireEvent.keyDown(document.activeElement ?? document.body, { key: 'Escape' });
  await screen.findByRole('alertdialog');
  await fireEvent.click(screen.getByRole('button', { name: '계속 편집' }));
  expect(screen.getByRole('dialog')).toBeInTheDocument();
  expect(Object.entries(window.localStorage)).toEqual(storedBefore);
});

it('keeps edits to another setting local until the conflict is explicitly retried', async () => {
  const current = { ...initial, visibility: 'private' as const };
  const save = vi
    .spyOn(documentsApi, 'setSettings')
    .mockResolvedValue({ kind: 'conflict', current, changed: [], requestId: 'race' });
  render(DocumentActions, { props: { doc: initial, workspaces: [], onchanged: () => undefined } });
  await fireEvent.click(screen.getByRole('button', { name: '공유·설정' }));
  await fireEvent.click(await screen.findByRole('checkbox', { name: /^비공개/ }));
  await screen.findByRole('region', { name: '지금과 비교' });
  const [copyInput] = within(screen.getByRole('dialog')).getAllByRole('checkbox', {
    name: '볼 수 있는 사람의 복제 허용',
  });
  if (copyInput === undefined) throw new Error('missing copy setting');
  await fireEvent.click(copyInput);
  expect(save).toHaveBeenCalledTimes(1);
  await fireEvent.click(screen.getByRole('button', { name: '다시 시도' }));
  expect(save).toHaveBeenLastCalledWith('d1', {
    visibility: 'private',
    previous_visibility: 'private',
    allow_copy: false,
    previous_allow_copy: true,
  });
});

it('drops a settings conflict arriving after an account change', async () => {
  let finish: () => void = () => undefined;
  vi.spyOn(documentsApi, 'setSettings').mockImplementation(
    () =>
      new Promise((resolve) => {
        finish = () => {
          resolve({
            kind: 'conflict',
            current: { ...initial, visibility: 'private' },
            changed: [],
            requestId: 'old-account',
          });
        };
      }),
  );
  const changed = vi.fn();
  render(DocumentActions, { props: { doc: initial, workspaces: [], onchanged: changed } });
  await fireEvent.click(screen.getByRole('button', { name: '공유·설정' }));
  await waitFor(() => {
    expect(changed).toHaveBeenCalledTimes(1);
  });
  await fireEvent.click(await screen.findByRole('checkbox', { name: /^비공개/ }));
  me.userId = 'other';
  finish();
  await waitFor(() => {
    expect(screen.queryByRole('button', { name: '다시 시도' })).toBeNull();
  });
  expect(changed).toHaveBeenCalledTimes(1);
});

it('parses a settings conflict and leaves account_changed as an error', async () => {
  const current = { ...initial, allow_copy: false };
  const fetch = vi
    .fn()
    .mockResolvedValueOnce(
      new Response(
        JSON.stringify({
          error: 'conflict',
          message: 'settings changed',
          current,
          changed: [],
          request_id: 'race',
        }),
        { status: 409 },
      ),
    )
    .mockResolvedValueOnce(
      new Response(
        JSON.stringify({
          error: 'account_changed',
          message: 'account changed',
        }),
        { status: 409 },
      ),
    );
  vi.stubGlobal('fetch', fetch);
  await expect(
    documentsApi.setSettings('d1', { allow_copy: false, previous_allow_copy: true }),
  ).resolves.toEqual({ kind: 'conflict', current, changed: [], requestId: 'race' });
  await expect(
    documentsApi.setSettings('d1', { allow_copy: false, previous_allow_copy: true }),
  ).rejects.toMatchObject({ status: 409, code: 'account_changed' });
});
