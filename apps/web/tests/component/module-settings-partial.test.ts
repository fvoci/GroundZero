import type { Module, Workspace } from '@fvoci/api-client';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
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
  id: 'ws-1',
  name: '공간',
  role: 'admin',
  is_owner: true,
  realtime_enabled: false,
};

const before: Module = {
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
});
