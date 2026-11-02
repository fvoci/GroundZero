import type { Module, Workspace } from '@fvoci/api-client';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { api, NetworkError, type ItemView } from '$lib/api/client';
import { me } from '$lib/me.svelte';
import ModuleScreen from '$lib/features/modules/ModuleScreen.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';
import { router } from '$lib/router.svelte';
import { unloadBlocked } from './fixtures/leave-warning';
import { permissionState } from './fixtures/permission-state.svelte';

// #289: 새 항목·템플릿 폼은 `canCreate`(공간 read 아님, 모듈 보관 아님)일 때만 보인다. 숨은 뒤에도 그 입력이
// 떠날 때 경고를 남기면 안 된다. 같은 화면에서 계속 보이는 설정 입력은 그대로 보호한다(D-27).

const workspace: Workspace = {
  id: 'ws-hidden',
  organization_id: 'org-1',
  name: '숨은 입력 공간',
  access: 'owner',
  role: 'admin',
  is_owner: true,
};

function module(archived = false): Module {
  return {
    id: 'DEV',
    workspace_id: workspace.id,
    kind: 'project',
    name: '모듈 DEV',
    key: 'DEV',
    previous_keys: [],
    archived,
    icon: '',
    description: '',
    access: 'owner',
  };
}

function template(id: string): ItemView {
  return {
    id,
    module_id: 'DEV',
    workspace_id: workspace.id,
    reference: 'DEV-1',
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
  me.userId = 'hidden-account';
  vi.stubGlobal('fetch', () => new Promise<Response>(() => undefined));
  vi.spyOn(api, 'module').mockResolvedValue(module());
  vi.spyOn(api, 'items').mockResolvedValue([template('first'), template('chosen')]);
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  localStorage.clear();
  me.userId = '';
});

async function archive(): Promise<void> {
  vi.spyOn(api, 'archiveModule').mockResolvedValue(module(true));
  await fireEvent.click(screen.getByRole('button', { name: m.modules_archive() }));
  await screen.findByRole('button', { name: m.modules_unarchive() });
}

it('drops the hidden template choice after archiving while guarding visible settings', async () => {
  const copy = vi.spyOn(api, 'copyItem');
  const update = vi.spyOn(api, 'updateModule');
  render(ModuleScreen, { workspace, moduleId: 'DEV' });
  await screen.findByRole('heading', { name: '모듈 DEV' });
  await fireEvent.change(screen.getByLabelText(m.modules_template()), {
    target: { value: 'chosen' },
  });
  await fireEvent.input(screen.getByLabelText(m.modules_template_item_title()), {
    target: { value: '숨을 템플릿 제목' },
  });
  await fireEvent.input(screen.getByLabelText(m.modules_name()), {
    target: { value: '보이는 설정' },
  });
  await archive();
  expect(screen.queryByLabelText(m.modules_template_item_title())).toBeNull();
  // 설정은 관리자에게 계속 보이므로 경고가 남는다.
  expect(screen.getByLabelText(m.modules_name())).toHaveValue('보이는 설정');
  expect(unloadBlocked()).toBe(true);
  await fireEvent.input(screen.getByLabelText(m.modules_name()), {
    target: { value: '모듈 DEV' },
  });
  expect(unloadBlocked()).toBe(false);
  expect(router.guard?.(vi.fn(), '/')).toBe(true);
  // 다시 만들 수 있게 되면 빈 템플릿 폼에서 시작한다.
  vi.spyOn(api, 'archiveModule').mockResolvedValue(module(false));
  await fireEvent.click(screen.getByRole('button', { name: m.modules_unarchive() }));
  expect(await screen.findByLabelText(m.modules_template_item_title())).toHaveValue('');
  expect(screen.getByLabelText(m.modules_template())).toHaveProperty('selectedIndex', -1);
  expect(copy).not.toHaveBeenCalled();
  expect(update).not.toHaveBeenCalled();
});

it('drops the hidden template title when workspace access falls to read', async () => {
  const currentWorkspace = permissionState<Workspace>({ ...workspace, access: 'write' });
  render(ModuleScreen, { workspace: currentWorkspace, moduleId: 'DEV' });
  await screen.findByRole('heading', { name: '모듈 DEV' });
  await fireEvent.input(screen.getByLabelText(m.modules_template_item_title()), {
    target: { value: '읽기로 숨을 제목' },
  });
  expect(unloadBlocked()).toBe(true);
  currentWorkspace.access = 'read';
  await waitFor(() => {
    expect(screen.queryByLabelText(m.modules_template_item_title())).toBeNull();
  });
  expect(unloadBlocked()).toBe(false);
  expect(router.guard?.(vi.fn(), '/')).toBe(true);
  currentWorkspace.access = 'write';
  expect(await screen.findByLabelText(m.modules_template_item_title())).toHaveValue('');
});

it('keeps a sent template copy locked until its response, then clears the hidden input', async () => {
  let reject: (e: Error) => void = () => undefined;
  const copy = vi.spyOn(api, 'copyItem').mockReturnValue(
    new Promise((_, fail) => {
      reject = fail;
    }),
  );
  const currentWorkspace = permissionState<Workspace>({ ...workspace, access: 'write' });
  render(ModuleScreen, { workspace: currentWorkspace, moduleId: 'DEV' });
  await screen.findByRole('heading', { name: '모듈 DEV' });
  await fireEvent.input(screen.getByLabelText(m.modules_template_item_title()), {
    target: { value: '보내는 중인 제목' },
  });
  await fireEvent.click(screen.getByRole('button', { name: m.modules_from_template() }));
  expect(copy).toHaveBeenCalledOnce();
  currentWorkspace.access = 'read';
  await waitFor(() => {
    expect(screen.queryByLabelText(m.modules_template_item_title())).toBeNull();
  });
  // 응답 전에는 이동을 막는다(보내는 중 잠금).
  expect(router.guard?.(vi.fn(), '/')).toBe(false);
  reject(new Error('forbidden'));
  await waitFor(() => {
    expect(unloadBlocked()).toBe(false);
  });
  expect(router.guard?.(vi.fn(), '/')).toBe(true);
  expect(screen.queryByRole('alert')).toBeNull();
  currentWorkspace.access = 'write';
  expect(await screen.findByLabelText(m.modules_template_item_title())).toHaveValue('');
  expect(copy).toHaveBeenCalledOnce();
});

it('releases the new item title guard when its form unmounts and keeps its stored draft', async () => {
  const create = vi.spyOn(api, 'createItem');
  render(ModuleScreen, { workspace, moduleId: 'DEV' });
  await screen.findByRole('heading', { name: '모듈 DEV' });
  await fireEvent.input(screen.getByLabelText(m.modules_new_item_title()), {
    target: { value: '보관 전에 쓴 새 항목' },
  });
  expect(unloadBlocked()).toBe(true);
  await archive();
  expect(screen.queryByLabelText(m.modules_new_item_title())).toBeNull();
  expect(unloadBlocked()).toBe(false);
  expect(router.guard?.(vi.fn(), '/')).toBe(true);
  vi.spyOn(api, 'archiveModule').mockResolvedValue(module(false));
  await fireEvent.click(screen.getByRole('button', { name: m.modules_unarchive() }));
  await screen.findByLabelText(m.modules_new_item_title());
  // 긴 입력이 아니어도 자식 폼이 남긴 초안은 지우지 않는다. 다시 보이면 되살리기를 제안한다.
  expect(await screen.findByTestId('form-draft-offer')).toBeInTheDocument();
  expect(create).not.toHaveBeenCalled();
});

it('keeps a lost template copy as an offer with its request ID across an access loss', async () => {
  const copy = vi.spyOn(api, 'copyItem').mockRejectedValue(new NetworkError('lost response'));
  const currentWorkspace = permissionState<Workspace>({ ...workspace, access: 'write' });
  render(ModuleScreen, { workspace: currentWorkspace, moduleId: 'DEV' });
  await screen.findByRole('heading', { name: '모듈 DEV' });
  await fireEvent.change(screen.getByLabelText(m.modules_template()), {
    target: { value: 'chosen' },
  });
  await fireEvent.input(screen.getByLabelText(m.modules_template_item_title()), {
    target: { value: '응답을 잃은 복제' },
  });
  await fireEvent.click(screen.getByRole('button', { name: m.modules_from_template() }));
  await screen.findByRole('alert');
  expect(copy).toHaveBeenCalledOnce();
  const original = copy.mock.calls[0];
  currentWorkspace.access = 'read';
  await waitFor(() => {
    expect(screen.queryByLabelText(m.modules_template_item_title())).toBeNull();
  });
  expect(unloadBlocked()).toBe(false);
  expect(router.guard?.(vi.fn(), '/')).toBe(true);
  // 숨은 동안 남긴 입력은 안내와 버리기만 보인다.
  expect(await screen.findByTestId('form-draft-offer')).toHaveTextContent(
    m.common_draft_read_only(),
  );
  currentWorkspace.access = 'write';
  expect(await screen.findByLabelText(m.modules_template_item_title())).toHaveValue('');
  await fireEvent.click(await screen.findByRole('button', { name: m.common_draft_restore() }));
  expect(screen.getByLabelText(m.modules_template_item_title())).toHaveValue('응답을 잃은 복제');
  expect(screen.getByLabelText(m.modules_template())).toHaveValue('chosen');
  await fireEvent.click(screen.getByRole('button', { name: m.modules_from_template() }));
  await waitFor(() => {
    expect(copy).toHaveBeenCalledTimes(2);
  });
  // 같은 입력의 다시 내기는 같은 요청 id로 보내 중복 복제를 막는다.
  expect(copy.mock.calls[1]).toEqual(original);
});
