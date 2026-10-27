import type { Module, Workspace } from '@fvoci/api-client';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { api } from '$lib/api/client';
import ModuleScreen from '$lib/features/modules/ModuleScreen.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

afterEach(() => {
  vi.restoreAllMocks();
});

const workspace: Workspace = {
  id: 'ws-1',
  organization_id: 'org-1',
  name: '공간',
  role: 'admin',
  is_owner: true,
  realtime_enabled: false,
};

function module(id: string, key: string): Module {
  return {
    id,
    workspace_id: 'ws-1',
    kind: 'project',
    name: `모듈 ${id}`,
    key,
    previous_keys: [],
    archived: false,
    icon: '',
    description: '',
  };
}

// 감사 3번: A 모듈 설정을 저장하는 도중 B로 옮겨 가면, A의 새 키가 B에 적용되거나 B 화면에 A의 결과가 들어가면 안 된다.
describe('module settings save while switching modules', () => {
  it('saves to the module it started on and leaves the next module alone', async () => {
    await setLocale('ko', { reload: false });
    vi.spyOn(api, 'module').mockImplementation((id: string) =>
      Promise.resolve(id === 'A' ? module('A', 'AAA') : module('B', 'BBB')),
    );
    const items = vi.spyOn(api, 'items').mockResolvedValue([]);
    let finishDescription: () => void = () => undefined;
    const description = vi.spyOn(api, 'setModuleDescription').mockImplementation(
      () =>
        new Promise<Module>((resolve) => {
          finishDescription = () => {
            resolve(module('A', 'AAA'));
          };
        }),
    );
    const update = vi
      .spyOn(api, 'updateModule')
      .mockImplementation((id: string, name: string, key: string) =>
        Promise.resolve({ ...module(id, key), name }),
      );
    const view = render(ModuleScreen, { props: { workspace, moduleId: 'A' } });
    const key = await screen.findByLabelText('키');
    await fireEvent.input(key, { target: { value: 'AAX' } });
    await fireEvent.input(screen.getByLabelText('설명'), { target: { value: '바뀐 설명' } });
    await fireEvent.submit(key.closest('form') ?? key);
    await waitFor(() => {
      expect(description).toHaveBeenCalledWith('A', '바뀐 설명');
    });

    // 설명 저장이 끝나기 전에 B로 옮긴다.
    await view.rerender({ workspace, moduleId: 'B' });
    await waitFor(() => {
      expect(inputValue(screen.getByLabelText('키'))).toBe('BBB');
    });
    items.mockClear();
    finishDescription();
    // A의 저장은 설명 다음에 이름·키를 A로, A의 입력값(AAX)으로 보낸다. 그 호출을 기다린다(고정 대기 대신).
    await waitFor(() => {
      expect(update).toHaveBeenCalled();
    });
    expect(update.mock.calls.at(-1)?.[0]).toBe('A');
    expect(update.mock.calls.at(-1)?.[2]).toBe('AAX');

    for (const call of update.mock.calls) expect(call[0]).toBe('A');
    expect(items.mock.calls.map((c) => c[0])).not.toContain('A');
    expect(inputValue(screen.getByLabelText('키'))).toBe('BBB');
  });
});

function inputValue(element: HTMLElement): string {
  return element instanceof HTMLInputElement ? element.value : '';
}
