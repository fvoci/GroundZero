import type { Module, Workspace } from '@fvoci/api-client';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { api, ApiError } from '$lib/api/client';
import ModuleScreen from '$lib/features/modules/ModuleScreen.svelte';
import { workspaceNav } from '$lib/features/nav/workspace-nav.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

const workspace: Workspace = {
  access: 'owner',
  id: 'ws-1',
  organization_id: 'org-1',
  name: '공간',
  role: 'admin',
  is_owner: true,
  realtime_enabled: false,
};

const before: Module = {
  access: 'owner',
  id: 'A',
  workspace_id: 'ws-1',
  kind: 'project',
  name: '개발',
  key: 'DEV',
  previous_keys: [],
  archived: false,
  icon: '',
  description: '예전 설명',
};

function field(id: string): HTMLInputElement | HTMLTextAreaElement {
  const el = document.getElementById(id);
  if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement) return el;
  throw new Error(id);
}

// 감사 W20: 모듈 설정 저장은 아이콘·설명·이름과 키를 따로 보낸다. 키가 이미 쓰여 마지막 요청이 409로 막히면 아이콘과
// 설명은 이미 저장됐는데 화면 머리는 옛 값을 그대로 보였다. 저장된 부분은 화면에 넣고 모듈 목록도 다시 읽는다.
describe('module settings partial save', () => {
  it('shows the saved icon and description when the key update is refused', async () => {
    await setLocale('ko', { reload: false });
    vi.stubGlobal('fetch', () => new Promise<Response>(() => undefined));
    vi.spyOn(api, 'module').mockResolvedValue(before);
    vi.spyOn(api, 'items').mockResolvedValue([]);
    const icon = vi.spyOn(api, 'setModuleIcon').mockResolvedValue(undefined);
    const description = vi
      .spyOn(api, 'setModuleDescription')
      .mockResolvedValue({ ...before, icon: '📚', description: '새 설명' });
    vi.spyOn(api, 'updateModule').mockRejectedValue(
      new ApiError(409, 'conflict', 'the key is taken'),
    );
    const refresh = vi.spyOn(workspaceNav, 'refreshModules').mockReturnValue(undefined);
    render(ModuleScreen, { props: { workspace, moduleId: 'A' } });
    await screen.findByRole('heading', { name: '개발' });

    await fireEvent.input(field('module-edit-icon'), { target: { value: '📚' } });
    await fireEvent.input(field('module-edit-description'), { target: { value: '새 설명' } });
    await fireEvent.input(field('module-edit-key'), { target: { value: 'OPS' } });
    await fireEvent.submit(field('module-edit-key').closest('form') ?? field('module-edit-key'));

    expect(await screen.findByRole('alert')).toHaveTextContent('이미 쓰고 있거나');
    expect(icon).toHaveBeenCalledWith('A', '📚');
    expect(description).toHaveBeenCalledWith('A', '새 설명');
    // 이미 저장된 아이콘과 설명이 머리에 보인다(아이콘은 aria-hidden이라 글자로 본다).
    expect(screen.getByRole('heading', { level: 1 }).textContent).toContain('📚');
    expect(screen.getByText('새 설명', { selector: 'p' })).toBeInTheDocument();
    await waitFor(() => {
      expect(refresh).toHaveBeenCalled();
    });
    // 막힌 키 입력은 그대로 남는다.
    expect(field('module-edit-key').value).toBe('OPS');
  });

  // 리뷰: 저장 중에도 다시 보낼 수 있어 저장이 겹쳤다. 먼저 시작한 저장이 늦게 실패하면 그 저장의 일부(아이콘만 바꾼 옛
  // 모듈)를 화면에 넣어, 뒤 저장이 서버에 넣은 이름을 화면에서 되돌렸다. 저장이 끝날 때까지 다시 보내지 않는다.
  it('does not start a second save while one is in flight', async () => {
    await setLocale('ko', { reload: false });
    vi.stubGlobal('fetch', () => new Promise<Response>(() => undefined));
    vi.spyOn(api, 'module').mockResolvedValue(before);
    vi.spyOn(api, 'items').mockResolvedValue([]);
    vi.spyOn(api, 'setModuleIcon').mockResolvedValue(undefined);
    vi.spyOn(workspaceNav, 'refreshModules').mockReturnValue(undefined);
    // 서버에 저장된 이름. 첫 저장은 이름·키 단계에서 늦게 실패하고, 그 뒤의 저장은 받아들인다.
    let serverName = before.name;
    let failFirst: (e: unknown) => void = () => undefined;
    const update = vi
      .spyOn(api, 'updateModule')
      .mockImplementationOnce(
        () =>
          new Promise<Module>((_resolve, reject) => {
            failFirst = reject;
          }),
      )
      .mockImplementation((id: string, name: string, key: string) => {
        serverName = name;
        return Promise.resolve({ ...before, id, name, key, icon: '📚' });
      });
    render(ModuleScreen, { props: { workspace, moduleId: 'A' } });
    await screen.findByRole('heading', { name: '개발' });

    await fireEvent.input(field('module-edit-icon'), { target: { value: '📚' } });
    await fireEvent.input(field('module-edit-name'), { target: { value: '새 이름' } });
    const form = field('module-edit-name').closest('form');
    if (form === null) throw new Error('settings form');
    await fireEvent.submit(form);
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(1);
    });
    // 첫 저장이 끝나기 전에 한 번 더 보낸다(두 번 누르기·Enter). 그 저장이 있었다면 끝까지 돌게 둔다.
    await fireEvent.submit(form);
    await new Promise((r) => setTimeout(r, 0));
    failFirst(new ApiError(500, 'internal', 'boom'));
    await screen.findByRole('alert');

    // 화면 머리는 서버에 저장된 이름과 같아야 한다.
    expect(screen.getByRole('heading', { level: 1 }).textContent).toContain(serverName);
    expect(update).toHaveBeenCalledTimes(1);
  });

  it('disables the save button while saving', async () => {
    await setLocale('ko', { reload: false });
    vi.stubGlobal('fetch', () => new Promise<Response>(() => undefined));
    vi.spyOn(api, 'module').mockResolvedValue(before);
    vi.spyOn(api, 'items').mockResolvedValue([]);
    vi.spyOn(api, 'updateModule').mockImplementation(() => new Promise<Module>(() => undefined));
    render(ModuleScreen, { props: { workspace, moduleId: 'A' } });
    await screen.findByRole('heading', { name: '개발' });

    const form = field('module-edit-name').closest('form');
    if (form === null) throw new Error('settings form');
    const save = within(form).getByRole('button', { name: '저장' });
    expect(save).toBeEnabled();
    await fireEvent.submit(form);
    await waitFor(() => {
      expect(save).toBeDisabled();
    });
  });
});
import './fixtures/authenticated';
