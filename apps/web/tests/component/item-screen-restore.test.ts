import './fixtures/task-statuses';
import type { Module, Workspace } from '@fvoci/api-client';
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { api, NetworkError, type ItemView } from '$lib/api/client';
import ItemScreen from '$lib/features/modules/ItemScreen.svelte';
import { me } from '$lib/me.svelte';
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
const mod: Module = {
  id: 'mod-restore',
  workspace_id: workspace.id,
  kind: 'project',
  name: '개발',
  key: 'DEV',
  previous_keys: [],
  archived: false,
  icon: '',
  description: '',
  access: 'owner',
};

function item(id = 'item-a', version = 3): ItemView {
  return {
    id,
    module_id: mod.id,
    workspace_id: workspace.id,
    reference: 'DEV-1',
    title: `서버 항목 ${id}`,
    description: '서버 설명',
    visibility: 'private',
    access: 'owner',
    archived: false,
    updated_at: '',
    status: 'active',
    is_template: false,
    version,
    starts_on: null,
    ends_on: null,
  };
}

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  localStorage.clear();
  me.userId = 'restore-account-a';
  // Keep unrelated task/calendar/record reads pending; the screens and their draft forms are real.
  vi.stubGlobal('fetch', () => new Promise<Response>(() => undefined));
  vi.spyOn(api, 'item').mockImplementation((id) => Promise.resolve(item(id)));
  vi.spyOn(api, 'module').mockResolvedValue(mod);
  vi.spyOn(api, 'itemDocuments').mockResolvedValue([]);
  vi.spyOn(api, 'documentTemplates').mockResolvedValue([
    { id: 'template-first', title: '첫 양식', reference: 'WIKI-1', personal: false },
    { id: 'template-chosen', title: '선택할 양식', reference: 'WIKI-2', personal: false },
  ]);
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  localStorage.clear();
  me.userId = '';
});

async function mount(itemId = 'item-a', title = `서버 항목 ${itemId}`) {
  const view = render(ItemScreen, { workspace, itemId, workspaces: [workspace] });
  await screen.findByRole('heading', { name: title });
  await screen.findByRole('option', { name: /선택할 양식/ });
  return view;
}

function warnsBeforeUnload(): void {
  const unload = new Event('beforeunload', { cancelable: true });
  window.dispatchEvent(unload);
  expect(unload.defaultPrevented).toBe(true);
}

it('restores an item document title, template and failed request ID only for its account and item', async () => {
  const create = vi
    .spyOn(api, 'createItemDocument')
    .mockRejectedValue(new NetworkError('lost response'));
  const first = await mount();
  await fireEvent.input(screen.getByLabelText(m.modules_new_document_title()), {
    target: { value: '문서 초안 日本語' },
  });
  await fireEvent.change(screen.getByLabelText(m.documents_template_label()), {
    target: { value: 'template-chosen' },
  });
  warnsBeforeUnload();
  await fireEvent.click(screen.getByRole('button', { name: m.modules_create_document() }));
  await screen.findByRole('alert');
  expect(create).toHaveBeenCalledOnce();
  const original = create.mock.calls[0];
  expect(original).toEqual([
    'item-a',
    expect.any(String),
    '문서 초안 日本語',
    null,
    'template-chosen',
  ]);
  expect(original?.[1]).toBeTruthy();
  first.unmount();

  const otherTarget = await mount('item-b');
  expect(screen.queryByTestId('form-draft-offer')).not.toBeInTheDocument();
  expect(screen.getByLabelText(m.modules_new_document_title())).toHaveValue('');
  expect(screen.getByLabelText(m.documents_template_label())).toHaveValue('');
  otherTarget.unmount();
  me.userId = 'restore-account-b';
  const otherAccount = await mount();
  expect(screen.queryByTestId('form-draft-offer')).not.toBeInTheDocument();
  expect(screen.getByLabelText(m.modules_new_document_title())).toHaveValue('');
  expect(document.body).not.toHaveTextContent('문서 초안 日本語');
  otherAccount.unmount();

  me.userId = 'restore-account-a';
  await mount();
  await screen.findByTestId('form-draft-offer');
  expect(screen.getByLabelText(m.modules_new_document_title())).toHaveValue('');
  expect(screen.getByLabelText(m.documents_template_label())).toHaveValue('');
  expect(create).toHaveBeenCalledOnce();
  await fireEvent.click(screen.getByRole('button', { name: m.common_draft_restore() }));
  expect(screen.getByLabelText(m.modules_new_document_title())).toHaveValue('문서 초안 日本語');
  expect(screen.getByLabelText(m.documents_template_label())).toHaveValue('template-chosen');
  expect(create).toHaveBeenCalledOnce();
  warnsBeforeUnload();
  await fireEvent.click(screen.getByRole('button', { name: m.modules_create_document() }));
  await waitFor(() => {
    expect(create).toHaveBeenCalledTimes(2);
  });
  expect(create.mock.calls[1]).toEqual(original);
  await screen.findByRole('alert');
});

it('restores item information and its edit-start version before the first explicit save on a newer server', async () => {
  const advanced = { ...item('item-a', 9), title: '서버 최신 제목', description: '서버 최신 설명' };
  const update = vi
    .spyOn(api, 'updateItem')
    .mockResolvedValue({ kind: 'conflict', current: advanced });
  const first = await mount();
  await fireEvent.input(screen.getByLabelText(m.modules_title()), {
    target: { value: '복원할 제목' },
  });
  await fireEvent.input(screen.getByLabelText(m.modules_description()), {
    target: { value: '복원할 설명\n둘째 줄' },
  });
  await fireEvent.click(screen.getByRole('checkbox', { name: m.modules_share_with_members() }));
  expect(screen.getByRole('checkbox', { name: m.modules_share_with_members() })).toBeChecked();
  warnsBeforeUnload();
  expect(update).not.toHaveBeenCalled();
  first.unmount();

  me.userId = 'restore-account-b';
  const otherAccount = await mount();
  expect(screen.queryByTestId('form-draft-offer')).not.toBeInTheDocument();
  expect(screen.getByLabelText(m.modules_title())).toHaveValue('서버 항목 item-a');
  expect(screen.getByLabelText(m.modules_description())).toHaveValue('서버 설명');
  expect(screen.getByRole('checkbox', { name: m.modules_share_with_members() })).not.toBeChecked();
  expect(document.body).not.toHaveTextContent('복원할 설명');
  otherAccount.unmount();
  me.userId = 'restore-account-a';
  const otherTarget = await mount('item-b');
  expect(screen.queryByTestId('form-draft-offer')).not.toBeInTheDocument();
  expect(screen.getByLabelText(m.modules_title())).toHaveValue('서버 항목 item-b');
  otherTarget.unmount();

  vi.mocked(api.item).mockResolvedValue(advanced);
  await mount('item-a', advanced.title);
  await screen.findByTestId('form-draft-offer');
  expect(screen.getByLabelText(m.modules_title())).toHaveValue(advanced.title);
  expect(screen.getByLabelText(m.modules_description())).toHaveValue(advanced.description);
  expect(screen.getByRole('checkbox', { name: m.modules_share_with_members() })).not.toBeChecked();
  expect(update).not.toHaveBeenCalled();
  await fireEvent.click(screen.getByRole('button', { name: m.common_draft_restore() }));
  expect(screen.getByLabelText(m.modules_title())).toHaveValue('복원할 제목');
  expect(screen.getByLabelText(m.modules_description())).toHaveValue('복원할 설명\n둘째 줄');
  expect(screen.getByRole('checkbox', { name: m.modules_share_with_members() })).toBeChecked();
  expect(update).not.toHaveBeenCalled();
  warnsBeforeUnload();
  const form = screen.getByRole('form', { name: m.modules_item_info() });
  await fireEvent.click(within(form).getByRole('button', { name: m.common_save() }));
  await waitFor(() => {
    expect(update).toHaveBeenCalledOnce();
  });
  expect(update).toHaveBeenCalledWith('item-a', 3, {
    title: '복원할 제목',
    description: '복원할 설명\n둘째 줄',
    visibility: 'workspace',
  });
  await screen.findByText(m.modules_item_conflict());
  expect(screen.getByLabelText(m.modules_title())).toHaveValue('복원할 제목');
});
