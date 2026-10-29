import type { Module, Workspace } from '@fvoci/api-client';
import { fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, expect, it, vi } from 'vitest';
import { api } from '$lib/api/client';
import { me } from '$lib/me.svelte';
import ModuleScreen from '$lib/features/modules/ModuleScreen.svelte';
import ModulesSection from '$lib/features/modules/ModulesSection.svelte';
import ItemCopy from '$lib/features/modules/ItemCopy.svelte';
import type { ItemView } from '$lib/api/client';
import { NetworkError } from '$lib/api/client';
import { setLocale } from '$lib/paraglide/runtime.js';
import { m } from '$lib/paraglide/messages.js';

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  localStorage.clear();
  me.userId = '';
});

it('keeps a failed new module request and restores only for its workspace', async () => {
  await setLocale('ko', { reload: false });
  me.userId = 'module-draft-user';
  vi.stubGlobal('fetch', () => new Promise<Response>(() => undefined));
  const create = vi.spyOn(api, 'createModule').mockRejectedValue(new NetworkError('offline'));
  const first = render(ModulesSection, { workspace });
  await fireEvent.input(screen.getByLabelText('이름'), { target: { value: '새 모듈' } });
  await fireEvent.input(screen.getByLabelText('키'), { target: { value: 'NEW' } });
  await fireEvent.click(screen.getByRole('button', { name: '모듈 만들기' }));
  await screen.findByRole('alert');
  const firstRequest = create.mock.calls[0]?.[1].requestId;
  expect(firstRequest).toBeTruthy();
  first.unmount();
  const other = render(ModulesSection, { workspace: { ...workspace, id: 'other' } });
  expect(screen.queryByTestId('form-draft-offer')).not.toBeInTheDocument();
  other.unmount();
  render(ModulesSection, { workspace });
  await fireEvent.click(await screen.findByRole('button', { name: m.common_draft_restore() }));
  expect(screen.getByLabelText('이름')).toHaveValue('새 모듈');
  expect(create).toHaveBeenCalledTimes(1);
  await fireEvent.click(screen.getByRole('button', { name: '모듈 만들기' }));
  await screen.findByRole('alert');
  expect(create.mock.calls[1]?.[1].requestId).toBe(firstRequest);
});
const workspace: Workspace = {
  id: 'ws',
  organization_id: 'org',
  name: '공간',
  access: 'owner',
  role: 'admin',
  is_owner: true,
};
const mod: Module = {
  id: 'mod',
  workspace_id: 'ws',
  kind: 'project',
  name: '개발',
  key: 'DEV',
  previous_keys: [],
  archived: false,
  icon: '',
  description: '',
  access: 'owner',
};
const item: ItemView = {
  id: 'copy-source',
  module_id: mod.id,
  workspace_id: workspace.id,
  reference: 'DEV-1',
  title: '원본',
  description: '',
  visibility: 'workspace',
  access: 'write',
  archived: false,
  updated_at: '',
  status: 'active',
  is_template: true,
  version: 1,
  starts_on: null,
  ends_on: null,
};

it.each([true, false])(
  'restores a failed item copy with its request ID (edited title: %s)',
  async (edited) => {
    await setLocale('ko', { reload: false });
    me.userId = 'module-draft-user';
    vi.spyOn(api, 'modules').mockResolvedValue([mod]);
    const copy = vi.spyOn(api, 'copyItem').mockRejectedValue(new NetworkError('offline'));
    const first = render(ItemCopy, { item, module: mod });
    await fireEvent.click(screen.getByRole('button', { name: m.modules_copy() }));
    const input = await screen.findByLabelText(m.modules_copy_new_title());
    if (edited) await fireEvent.input(input, { target: { value: '새 사본' } });
    await fireEvent.click(screen.getByRole('button', { name: m.modules_copy() }));
    await screen.findByRole('alert');
    const requestId = copy.mock.calls[0]?.[1];
    first.unmount();
    render(ItemCopy, { item, module: mod });
    await fireEvent.click(await screen.findByRole('button', { name: m.common_draft_restore() }));
    expect(screen.getByLabelText(m.modules_copy_new_title())).toHaveValue(
      edited ? '새 사본' : m.modules_copy_title_default({ title: item.title }),
    );
    await screen.findByRole('option', { name: '개발 (DEV)' });
    expect(copy).toHaveBeenCalledTimes(1);
    await fireEvent.click(screen.getByRole('button', { name: m.modules_copy() }));
    await screen.findByRole('alert');
    expect(copy.mock.calls[1]?.[1]).toBe(requestId);
  },
);

it('offers module settings after remount without saving until explicitly restored', async () => {
  await setLocale('ko', { reload: false });
  me.userId = 'module-draft-user';
  vi.stubGlobal('fetch', () => new Promise<Response>(() => undefined));
  vi.spyOn(api, 'module').mockResolvedValue(mod);
  vi.spyOn(api, 'items').mockResolvedValue([]);
  const save = vi.spyOn(api, 'updateModule').mockResolvedValue(mod);
  const first = render(ModuleScreen, { workspace, moduleId: mod.id });
  await screen.findByRole('heading', { name: '개발' });
  await fireEvent.input(screen.getByLabelText('설명'), {
    target: { value: '보존할 설명\n두 번째 줄' },
  });
  first.unmount();
  render(ModuleScreen, { workspace, moduleId: mod.id });
  await screen.findByTestId('form-draft-offer');
  expect(save).not.toHaveBeenCalled();
  await screen.findByRole('heading', { name: '개발' });
  await fireEvent.click(screen.getByRole('button', { name: m.common_draft_restore() }));
  expect(screen.getByLabelText('설명')).toHaveValue('보존할 설명\n두 번째 줄');
  expect(save).not.toHaveBeenCalled();
});
