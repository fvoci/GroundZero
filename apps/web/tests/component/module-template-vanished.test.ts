import type { Module, Workspace } from '@fvoci/api-client';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { ItemView } from '$lib/api/client';
import { api } from '$lib/api/client';
import ModuleScreen from '$lib/features/modules/ModuleScreen.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';
import './fixtures/authenticated';

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

const devModule: Module = {
  access: 'owner',
  id: 'DEV',
  workspace_id: 'ws-1',
  kind: 'project',
  name: '개발',
  key: 'DEV',
  previous_keys: [],
  archived: false,
  icon: '',
  description: '',
};

function template(id: string, title: string): ItemView {
  return {
    id,
    module_id: 'DEV',
    workspace_id: 'ws-1',
    reference: `DEV-${id}`,
    title,
    description: '',
    visibility: 'workspace',
    access: 'write',
    archived: false,
    updated_at: '2026-10-22T00:00:00Z',
    status: 'active',
    is_template: true,
    version: 1,
    starts_on: null,
    ends_on: null,
  };
}

function field(id: string): HTMLInputElement | HTMLSelectElement {
  const el = document.getElementById(id);
  if (el instanceof HTMLInputElement || el instanceof HTMLSelectElement) return el;
  throw new Error(id);
}

// 리뷰: 같은 모듈에서 설정을 저장하면 항목 목록을 다시 읽는다. 그사이 고른 템플릿이 보관·해제되어 목록에서 빠지면
// 선택 칸은 빈 칸으로 보이는데 고른 id는 남아, 만들기를 누르면 고르지 않은 첫 템플릿으로 항목을 복제했다.
describe('create from template after the picked template leaves the list', () => {
  it('asks to pick again instead of copying the first template', async () => {
    await setLocale('ko', { reload: false });
    // 모듈 달력 요청은 끝나지 않게 둔다.
    vi.stubGlobal('fetch', () => new Promise<Response>(() => undefined));
    vi.spyOn(api, 'module').mockResolvedValue(devModule);
    vi.spyOn(api, 'items')
      .mockResolvedValueOnce([template('t-1', '회의 템플릿'), template('t-2', '배포 템플릿')])
      .mockResolvedValue([template('t-2', '배포 템플릿')]);
    vi.spyOn(api, 'updateModule').mockImplementation((id: string, name: string, key: string) =>
      Promise.resolve({ ...devModule, id, name, key }),
    );
    const copy = vi
      .spyOn(api, 'copyItem')
      .mockResolvedValue({ item_id: 'new', skipped_documents: 0 });
    render(ModuleScreen, { props: { workspace, moduleId: 'DEV' } });
    await screen.findByRole('option', { name: '회의 템플릿' });
    await fireEvent.change(field('template-source'), { target: { value: 't-1' } });

    // 설정 저장 뒤 다시 읽은 목록에는 고른 템플릿(t-1)이 없다.
    const name = field('module-edit-name');
    await fireEvent.submit(name.closest('form') ?? name);
    await waitFor(() => {
      expect(screen.queryByRole('option', { name: '회의 템플릿' })).toBeNull();
    });
    await fireEvent.input(field('template-title'), { target: { value: '새 항목' } });
    await fireEvent.click(screen.getByRole('button', { name: '템플릿에서 만들기' }));

    expect(copy).not.toHaveBeenCalled();
    expect(await screen.findByRole('alert')).toHaveTextContent('템플릿과 새 항목 제목을 고르세요.');
  });
});
