import type { Module, Workspace } from '@fvoci/api-client';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { api, NetworkError, type ItemView } from '$lib/api/client';
import { me } from '$lib/me.svelte';
import ModuleScreen from '$lib/features/modules/ModuleScreen.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';

const workspace: Workspace = {
  id: 'ws-restore',
  organization_id: 'org-1',
  name: '복원 시험 공간',
  access: 'owner',
  role: 'admin',
  is_owner: true,
  realtime_enabled: false,
};

function module(id: string): Module {
  return {
    id,
    workspace_id: workspace.id,
    kind: 'project',
    name: `모듈 ${id}`,
    key: id,
    previous_keys: [],
    archived: false,
    icon: '',
    description: '',
    access: 'owner',
  };
}

function template(id: string, moduleId: string): ItemView {
  return {
    id,
    module_id: moduleId,
    workspace_id: workspace.id,
    reference: `${moduleId}-1`,
    title: `템플릿 ${id}`,
    description: '',
    visibility: 'workspace',
    access: 'owner',
    archived: false,
    updated_at: '',
    status: 'active',
    is_template: true,
    version: 1,
    starts_on: null,
    ends_on: null,
  };
}

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  localStorage.clear();
  me.userId = 'restore-account-a';
  // Calendar and other subordinate reads are outside these creation flows.
  vi.stubGlobal('fetch', () => new Promise<Response>(() => undefined));
  vi.spyOn(api, 'module').mockImplementation((id) => Promise.resolve(module(id)));
  vi.spyOn(api, 'items').mockImplementation((id) =>
    Promise.resolve([template(`${id}-first`, id), template(`${id}-chosen`, id)]),
  );
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  localStorage.clear();
  me.userId = '';
});

async function mount(moduleId = 'DEV') {
  const view = render(ModuleScreen, { workspace, moduleId });
  await screen.findByRole('heading', { name: `모듈 ${moduleId}` });
  return view;
}

function warnsBeforeUnload(): void {
  const unload = new Event('beforeunload', { cancelable: true });
  window.dispatchEvent(unload);
  expect(unload.defaultPrevented).toBe(true);
}

it('restores a failed module item title only on request and retries its original creation ID', async () => {
  const create = vi.spyOn(api, 'createItem').mockRejectedValue(new NetworkError('lost response'));
  const first = await mount();
  await fireEvent.input(screen.getByLabelText(m.modules_new_item_title()), {
    target: { value: '계속 작성할 항목 日本語' },
  });
  warnsBeforeUnload();
  await fireEvent.click(screen.getByRole('button', { name: m.modules_create_item() }));
  await screen.findByRole('alert');
  expect(create).toHaveBeenCalledOnce();
  const original = create.mock.calls[0];
  expect(original).toEqual(['DEV', expect.any(String), '계속 작성할 항목 日本語']);
  expect(original?.[1]).toBeTruthy();
  first.unmount();

  const otherTarget = await mount('OPS');
  expect(screen.queryByTestId('form-draft-offer')).not.toBeInTheDocument();
  expect(screen.getByLabelText(m.modules_new_item_title())).toHaveValue('');
  otherTarget.unmount();
  me.userId = 'restore-account-b';
  const otherAccount = await mount();
  expect(screen.queryByTestId('form-draft-offer')).not.toBeInTheDocument();
  expect(screen.getByLabelText(m.modules_new_item_title())).toHaveValue('');
  expect(document.body).not.toHaveTextContent('계속 작성할 항목 日本語');
  otherAccount.unmount();

  me.userId = 'restore-account-a';
  await mount();
  await screen.findByTestId('form-draft-offer');
  expect(screen.getByLabelText(m.modules_new_item_title())).toHaveValue('');
  expect(create).toHaveBeenCalledOnce();
  await fireEvent.click(screen.getByRole('button', { name: m.common_draft_restore() }));
  expect(screen.getByLabelText(m.modules_new_item_title())).toHaveValue('계속 작성할 항목 日本語');
  expect(create).toHaveBeenCalledOnce();
  warnsBeforeUnload();
  await fireEvent.click(screen.getByRole('button', { name: m.modules_create_item() }));
  await waitFor(() => {
    expect(create).toHaveBeenCalledTimes(2);
  });
  expect(create.mock.calls[1]).toEqual(original);
  await screen.findByRole('alert');
});

it('restores module template title, chosen source and pending request ID without copying automatically', async () => {
  const copy = vi.spyOn(api, 'copyItem').mockRejectedValue(new NetworkError('lost response'));
  const first = await mount();
  await fireEvent.change(screen.getByLabelText(m.modules_template()), {
    target: { value: 'DEV-chosen' },
  });
  await fireEvent.input(screen.getByLabelText(m.modules_template_item_title()), {
    target: { value: '선택한 템플릿의 초안' },
  });
  warnsBeforeUnload();
  await fireEvent.click(screen.getByRole('button', { name: m.modules_from_template() }));
  await screen.findByRole('alert');
  expect(copy).toHaveBeenCalledOnce();
  const original = copy.mock.calls[0];
  expect(original).toEqual(['DEV-chosen', expect.any(String), 'DEV', '선택한 템플릿의 초안', true]);
  expect(original?.[1]).toBeTruthy();
  first.unmount();

  const otherTarget = await mount('OPS');
  expect(screen.queryByTestId('form-draft-offer')).not.toBeInTheDocument();
  expect(screen.getByLabelText(m.modules_template_item_title())).toHaveValue('');
  otherTarget.unmount();
  me.userId = 'restore-account-b';
  const otherAccount = await mount();
  expect(screen.queryByTestId('form-draft-offer')).not.toBeInTheDocument();
  expect(screen.getByLabelText(m.modules_template_item_title())).toHaveValue('');
  otherAccount.unmount();

  me.userId = 'restore-account-a';
  await mount();
  await screen.findByTestId('form-draft-offer');
  expect(screen.getByLabelText(m.modules_template_item_title())).toHaveValue('');
  expect(screen.getByLabelText(m.modules_template())).toHaveProperty('selectedIndex', -1);
  expect(copy).toHaveBeenCalledOnce();
  await fireEvent.click(screen.getByRole('button', { name: m.common_draft_restore() }));
  expect(screen.getByLabelText(m.modules_template_item_title())).toHaveValue(
    '선택한 템플릿의 초안',
  );
  expect(screen.getByLabelText(m.modules_template())).toHaveValue('DEV-chosen');
  expect(copy).toHaveBeenCalledOnce();
  warnsBeforeUnload();
  await fireEvent.click(screen.getByRole('button', { name: m.modules_from_template() }));
  await waitFor(() => {
    expect(copy).toHaveBeenCalledTimes(2);
  });
  expect(copy.mock.calls[1]).toEqual(original);
  await screen.findByRole('alert');
});
