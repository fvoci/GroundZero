import type { Workspace } from '@fvoci/api-client';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { api, NetworkError } from '$lib/api/client';
import { me } from '$lib/me.svelte';
import ModulesSection from '$lib/features/modules/ModulesSection.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';
import { router } from '$lib/router.svelte';
import { unloadBlocked } from './fixtures/leave-warning';
import { permissionState } from './fixtures/permission-state.svelte';

// #289: 모듈 만들기 폼은 공간 owner에게만 보인다. owner가 아니게 되면 숨은 이름·키·종류가 떠날 때 경고를 남기면
// 안 되고, 남긴 입력을 되살려 숨은 칸을 채워서도 안 된다(D-27, D-113).

const workspace: Workspace = {
  id: 'ws-modules-hidden',
  organization_id: 'org-1',
  name: '공간',
  access: 'owner',
  role: 'admin',
  is_owner: true,
};

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  localStorage.clear();
  me.userId = 'modules-hidden-user';
  vi.stubGlobal('fetch', () => new Promise<Response>(() => undefined));
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  localStorage.clear();
  me.userId = '';
});

it('drops the hidden new module input when owner access ends', async () => {
  const create = vi.spyOn(api, 'createModule');
  const current = permissionState({ ...workspace });
  render(ModulesSection, { workspace: current });
  await fireEvent.input(screen.getByLabelText(m.modules_name()), {
    target: { value: '숨을 모듈' },
  });
  await fireEvent.input(screen.getByLabelText(m.modules_key()), { target: { value: 'HID' } });
  expect(unloadBlocked()).toBe(true);
  current.access = 'write';
  await waitFor(() => {
    expect(screen.queryByRole('form', { name: m.modules_create() })).toBeNull();
  });
  expect(unloadBlocked()).toBe(false);
  expect(router.guard?.(vi.fn(), '/')).toBe(true);
  current.access = 'owner';
  expect(await screen.findByLabelText(m.modules_name())).toHaveValue('');
  expect(screen.getByLabelText(m.modules_key())).toHaveValue('');
  expect(create).not.toHaveBeenCalled();
});

it('keeps a sent creation locked until its response, then clears the hidden input', async () => {
  let reject: (e: Error) => void = () => undefined;
  const create = vi.spyOn(api, 'createModule').mockReturnValue(
    new Promise((_, fail) => {
      reject = fail;
    }),
  );
  const current = permissionState({ ...workspace });
  render(ModulesSection, { workspace: current });
  await fireEvent.input(screen.getByLabelText(m.modules_name()), {
    target: { value: '보내는 모듈' },
  });
  await fireEvent.input(screen.getByLabelText(m.modules_key()), { target: { value: 'SND' } });
  await fireEvent.click(screen.getByRole('button', { name: m.modules_create() }));
  expect(create).toHaveBeenCalledOnce();
  current.access = 'write';
  await waitFor(() => {
    expect(screen.queryByRole('form', { name: m.modules_create() })).toBeNull();
  });
  expect(router.guard?.(vi.fn(), '/')).toBe(false);
  reject(new NetworkError('offline'));
  await waitFor(() => {
    expect(unloadBlocked()).toBe(false);
  });
  expect(router.guard?.(vi.fn(), '/')).toBe(true);
  current.access = 'owner';
  expect(await screen.findByLabelText(m.modules_name())).toHaveValue('');
  expect(screen.queryByRole('alert')).toBeNull();
});

it('does not restore a kept draft into the hidden form while not the owner', async () => {
  vi.spyOn(api, 'createModule').mockRejectedValue(new NetworkError('offline'));
  const first = render(ModulesSection, { workspace });
  await fireEvent.input(screen.getByLabelText(m.modules_name()), {
    target: { value: '남긴 모듈' },
  });
  await fireEvent.input(screen.getByLabelText(m.modules_key()), { target: { value: 'KPT' } });
  await fireEvent.click(screen.getByRole('button', { name: m.modules_create() }));
  await screen.findByRole('alert');
  first.unmount();
  const current = permissionState<Workspace>({ ...workspace, access: 'write' });
  render(ModulesSection, { workspace: current });
  await screen.findByTestId('form-draft-offer');
  expect(screen.queryByRole('button', { name: m.common_draft_restore() })).toBeNull();
  expect(screen.getByTestId('form-draft-offer')).toHaveTextContent(m.common_draft_read_only());
  expect(unloadBlocked()).toBe(false);
  current.access = 'owner';
  await fireEvent.click(await screen.findByRole('button', { name: m.common_draft_restore() }));
  expect(screen.getByLabelText(m.modules_name())).toHaveValue('남긴 모듈');
});

it('keeps a lost module creation as an offer with its request ID across an owner downgrade', async () => {
  const create = vi.spyOn(api, 'createModule').mockRejectedValue(new NetworkError('offline'));
  const current = permissionState({ ...workspace });
  render(ModulesSection, { workspace: current });
  await fireEvent.input(screen.getByLabelText(m.modules_name()), {
    target: { value: '응답을 잃은 모듈' },
  });
  await fireEvent.input(screen.getByLabelText(m.modules_key()), { target: { value: 'LST' } });
  await fireEvent.click(screen.getByRole('button', { name: m.modules_create() }));
  await screen.findByRole('alert');
  const original = create.mock.calls[0]?.[1].requestId;
  expect(original).toBeTruthy();
  current.access = 'write';
  await waitFor(() => {
    expect(screen.queryByRole('form', { name: m.modules_create() })).toBeNull();
  });
  expect(unloadBlocked()).toBe(false);
  expect(router.guard?.(vi.fn(), '/')).toBe(true);
  expect(await screen.findByTestId('form-draft-offer')).toHaveTextContent(
    m.common_draft_read_only(),
  );
  current.access = 'owner';
  expect(await screen.findByLabelText(m.modules_name())).toHaveValue('');
  await fireEvent.click(await screen.findByRole('button', { name: m.common_draft_restore() }));
  expect(screen.getByLabelText(m.modules_name())).toHaveValue('응답을 잃은 모듈');
  await fireEvent.click(screen.getByRole('button', { name: m.modules_create() }));
  await waitFor(() => {
    expect(create).toHaveBeenCalledTimes(2);
  });
  // 같은 입력의 다시 내기는 같은 요청 id로 보내 중복 모듈을 막는다.
  expect(create.mock.calls[1]?.[1].requestId).toBe(original);
});
