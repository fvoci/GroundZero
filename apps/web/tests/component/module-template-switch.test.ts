import type { Module, Workspace } from '@fvoci/api-client';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { ItemView } from '$lib/api/client';
import { api, ApiError } from '$lib/api/client';
import ModuleScreen from '$lib/features/modules/ModuleScreen.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

const workspace: Workspace = {
  access: 'write',
  id: 'ws-1',
  organization_id: 'org-1',
  name: '공간',
  role: 'member',
  is_owner: false,
  realtime_enabled: false,
};

function module(id: string): Module {
  return {
    access: 'owner',
    id,
    workspace_id: 'ws-1',
    kind: 'project',
    name: `모듈 ${id}`,
    key: id,
    previous_keys: [],
    archived: false,
    icon: '',
    description: '',
  };
}

function template(id: string, moduleId: string): ItemView {
  return {
    id,
    module_id: moduleId,
    workspace_id: 'ws-1',
    reference: `${moduleId}-1`,
    title: `${moduleId} 템플릿`,
    description: '',
    visibility: 'workspace',
    access: 'write',
    archived: false,
    updated_at: '2026-10-19T00:00:00Z',
    status: 'active',
    is_template: true,
    version: 1,
    starts_on: null,
    ends_on: null,
  };
}

/** 모듈 DEV·OPS(같은 종류)와 각자의 템플릿. 모듈 달력 요청은 끝나지 않게 둔다. */
function mockModules(): void {
  vi.stubGlobal('fetch', () => new Promise<Response>(() => undefined));
  vi.spyOn(api, 'module').mockImplementation((id: string) => Promise.resolve(module(id)));
  vi.spyOn(api, 'items').mockImplementation((id: string) =>
    Promise.resolve([template(id === 'DEV' ? 't-dev' : 't-ops', id)]),
  );
}

function field(id: string): HTMLInputElement | HTMLSelectElement {
  const el = document.getElementById(id);
  if (el instanceof HTMLInputElement || el instanceof HTMLSelectElement) return el;
  throw new Error(id);
}

async function submitTemplateForm(): Promise<void> {
  await fireEvent.click(screen.getByRole('button', { name: '템플릿에서 만들기' }));
}

// 감사 W17·W29: 모듈 화면은 모듈을 옮겨도 다시 만들어지지 않는다. 앞 모듈에서 고른 템플릿이 남아, 새 모듈의 목록에
// 없는데도(선택 칸은 빈 것처럼 보인다) 그 템플릿으로 새 모듈에 항목을 복제했다. 지금 모듈의 템플릿만 보낸다.
describe('create from template after switching modules', () => {
  it('copies from the current module template, not the one picked in the previous module', async () => {
    await setLocale('ko', { reload: false });
    mockModules();
    const copy = vi
      .spyOn(api, 'copyItem')
      .mockResolvedValue({ item_id: 'new', skipped_documents: 0 });
    const view = render(ModuleScreen, { props: { workspace, moduleId: 'DEV' } });
    await screen.findByRole('option', { name: 'DEV 템플릿' });
    await fireEvent.change(field('template-source'), { target: { value: 't-dev' } });

    await view.rerender({ workspace, moduleId: 'OPS' });
    await screen.findByRole('option', { name: 'OPS 템플릿' });
    await fireEvent.input(field('template-title'), { target: { value: '새 운영 항목' } });
    await submitTemplateForm();

    await waitFor(() => {
      expect(copy).toHaveBeenCalledTimes(1);
    });
    expect(copy.mock.calls[0]?.[0]).toBe('t-ops');
    expect(copy.mock.calls[0]?.[2]).toBe('OPS');
  });

  it('does not carry the typed title or the error over to the next module', async () => {
    await setLocale('ko', { reload: false });
    mockModules();
    vi.spyOn(api, 'copyItem').mockRejectedValue(new ApiError(400, 'bad_request', 'bad copy'));
    const view = render(ModuleScreen, { props: { workspace, moduleId: 'DEV' } });
    await screen.findByRole('option', { name: 'DEV 템플릿' });
    await fireEvent.input(field('template-title'), { target: { value: 'DEV에서 쓴 제목' } });
    await submitTemplateForm();
    expect(await screen.findByRole('alert')).toHaveTextContent('만들지 못했습니다');

    await view.rerender({ workspace, moduleId: 'OPS' });
    await screen.findByRole('option', { name: 'OPS 템플릿' });
    expect(field('template-title').value).toBe('');
    expect(screen.queryByRole('alert')).toBeNull();
  });
});

// 항목 목록에서 긴 제목 옆의 참조 번호가 글자 단위로 갈라졌다("DEV/U-9", screen-stories 세션이 발견). 하위 문서 목록과
// 같이 번호는 한 줄로 두고 제목이 대신 줄바꿈한다. 배치는 jsdom이 계산하지 않아 구조만 지킨다.
describe('module item list', () => {
  it('keeps the item reference on one line beside a long title', async () => {
    await setLocale('ko', { reload: false });
    vi.stubGlobal('fetch', () => new Promise<Response>(() => undefined));
    vi.spyOn(api, 'module').mockResolvedValue(module('DEVU'));
    const long =
      'https://example.test/workspaces/aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee/items/ffffffff';
    vi.spyOn(api, 'items').mockResolvedValue([
      { ...template('i9', 'DEVU'), reference: 'DEVU-9', title: long, is_template: false },
    ]);
    render(ModuleScreen, { props: { workspace, moduleId: 'DEVU' } });
    const list = await screen.findByTestId('item-list');
    await waitFor(() => {
      expect(list.textContent).toContain('DEVU-9');
    });
    const reference = [...list.querySelectorAll('span')].find((s) => s.textContent === 'DEVU-9');
    expect(reference).toHaveClass('shrink-0', 'whitespace-nowrap');
    const title = [...list.querySelectorAll('span')].find((s) => s.textContent === long);
    expect(title).toHaveClass('min-w-0', '[overflow-wrap:anywhere]');
  });
});
