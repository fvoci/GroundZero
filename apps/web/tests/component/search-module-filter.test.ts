import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { Module } from '@fvoci/api-client';

import type { SearchFilters } from '$lib/api/client';
import { api } from '$lib/api/client';
import SearchDialog from '$lib/features/search/SearchDialog.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

function module(id: string, name: string, archived = false): Module {
  return {
    access: 'owner',
    id,
    workspace_id: 'ws-1',
    kind: 'project',
    name,
    key: name.toUpperCase(),
    previous_keys: [],
    archived,
    icon: '',
    description: '',
  };
}

// D-39의 모듈 필터: 검색 API의 `module_id`는 있었지만 대화상자에서 고를 수 없었다. 지금 보는 워크스페이스의 모듈(보관
// 제외)을 고르면 요청에 `moduleId`로 실리고, 워크스페이스가 바뀌어 그 모듈이 없으면 선택을 거둔다.
describe('search module filter', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
    Element.prototype.hasPointerCapture = (): boolean => false;
    Element.prototype.setPointerCapture = (): void => undefined;
    Element.prototype.releasePointerCapture = (): void => undefined;
    Element.prototype.scrollIntoView = (): void => undefined;
    vi.spyOn(api, 'members').mockResolvedValue({
      members: [],
      my_role: 'admin',
      i_am_owner: true,
      can_manage_members: true,
    });
  });
  afterEach(() => {
    vi.restoreAllMocks();
  });

  async function openModules(): Promise<void> {
    const trigger = document.querySelector<HTMLElement>('[aria-labelledby="search-module-label"]');
    if (trigger === null) throw new Error('module trigger');
    await fireEvent.pointerDown(trigger, { button: 0, pointerType: 'mouse' });
    await fireEvent.click(trigger);
  }
  async function pick(name: string, alreadyOpen = false): Promise<void> {
    if (!alreadyOpen) await openModules();
    const option = await screen.findByRole('option', { name });
    await fireEvent.pointerUp(option, { button: 0, pointerType: 'mouse' });
    await fireEvent.click(option);
  }

  it('sends the chosen module as module_id and lists only live modules', async () => {
    const modules = vi
      .spyOn(api, 'modules')
      .mockResolvedValue([module('m-1', '출시'), module('m-2', '옛 모듈', true)]);
    const search = vi.spyOn(api, 'search').mockResolvedValue([]);
    render(SearchDialog, {
      props: { open: true, memberOf: () => true, workspace: { id: 'ws-1', name: '공간' } },
    });
    await waitFor(() => {
      expect(modules).toHaveBeenCalledWith('ws-1');
    });
    await fireEvent.input(screen.getByTestId('search-input'), { target: { value: '회의' } });
    await waitFor(() => {
      expect(search).toHaveBeenCalled();
    });
    expect(search.mock.calls.at(-1)?.[2]).not.toHaveProperty('moduleId');
    // 메뉴가 열린 상태에서: 보관하지 않은 모듈만 고를 수 있다.
    await openModules();
    expect(await screen.findByRole('option', { name: '출시' })).toBeTruthy();
    expect(screen.queryByRole('option', { name: '옛 모듈' })).toBeNull();
    await pick('출시', true);
    await waitFor(() => {
      const last: SearchFilters | undefined = search.mock.calls.at(-1)?.[2];
      expect(last?.moduleId).toBe('m-1');
    });
  });

  it('drops the chosen module when the next workspace does not have it', async () => {
    vi.spyOn(api, 'modules')
      .mockResolvedValueOnce([module('m-1', '출시')])
      .mockResolvedValue([]);
    const search = vi.spyOn(api, 'search').mockResolvedValue([]);
    const view = render(SearchDialog, {
      props: { open: true, memberOf: () => true, workspace: { id: 'ws-1', name: '공간' } },
    });
    await screen.findByText('모든 모듈');
    await fireEvent.input(screen.getByTestId('search-input'), { target: { value: '회의' } });
    await pick('출시');
    await waitFor(() => {
      expect(search.mock.calls.at(-1)?.[2]?.moduleId).toBe('m-1');
    });
    // 새 워크스페이스의 모듈 목록이 오기 전이라도 옛 모듈로 검색하지 않는다(목록 응답을 미룬다).
    let giveModules: (list: Module[]) => void = () => undefined;
    vi.spyOn(api, 'modules').mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          giveModules = resolve;
        }),
    );
    await view.rerender({
      open: true,
      memberOf: () => true,
      workspace: { id: 'ws-2', name: '다른 공간' },
    });
    await waitFor(() => {
      expect(search.mock.calls.at(-1)?.[2]).not.toHaveProperty('moduleId');
    });
    giveModules([]);
    expect(screen.getByText('모든 모듈')).toBeTruthy();
  });

  it('keeps showing the chosen module while the list reloads on reopening', async () => {
    // 같은 워크스페이스에서 닫았다 다시 열면 모듈 목록을 다시 읽는다. 그 사이에도 선택(거르기)은 남으므로 버튼도 고른
    // 모듈을 보여야 한다. 전에는 목록이 올 때까지 '모든 모듈'로 보였는데 검색은 그 모듈로 좁혀졌다.
    const modules = vi.spyOn(api, 'modules').mockResolvedValue([module('m-1', '출시')]);
    const search = vi.spyOn(api, 'search').mockResolvedValue([]);
    const view = render(SearchDialog, {
      props: { open: true, memberOf: () => true, workspace: { id: 'ws-1', name: '공간' } },
    });
    const trigger = (): HTMLElement => {
      const all = document.querySelectorAll<HTMLElement>('[aria-labelledby="search-module-label"]');
      expect(all.length).toBe(1);
      const el = all[0];
      if (el === undefined) throw new Error('module trigger');
      return el;
    };
    await screen.findByText('모든 모듈');
    expect(modules).toHaveBeenCalledTimes(1);
    await fireEvent.input(screen.getByTestId('search-input'), { target: { value: '회의' } });
    await pick('출시');
    await waitFor(() => {
      expect(search.mock.calls.at(-1)?.[2]?.moduleId).toBe('m-1');
    });
    expect(trigger().textContent.trim()).toBe('출시');
    await view.rerender({
      open: false,
      memberOf: () => true,
      workspace: { id: 'ws-1', name: '공간' },
    });
    // 다시 열 때의 목록 응답을 미룬다.
    let giveModules: (list: Module[]) => void = () => undefined;
    modules.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          giveModules = resolve;
        }),
    );
    await view.rerender({
      open: true,
      memberOf: () => true,
      workspace: { id: 'ws-1', name: '공간' },
    });
    await waitFor(() => {
      expect(modules).toHaveBeenCalledTimes(2);
    });
    expect(trigger().textContent.trim()).toBe('출시');
    // 목록이 오기 전에 찾아도 버튼이 말하는 대로 그 모듈로 좁힌다.
    await fireEvent.input(screen.getByTestId('search-input'), { target: { value: '회의록' } });
    await waitFor(() => {
      expect(search.mock.calls.at(-1)?.[0]).toBe('회의록');
    });
    expect(search.mock.calls.at(-1)?.[2]?.moduleId).toBe('m-1');
    expect(trigger().textContent.trim()).toBe('출시');
    giveModules([module('m-1', '출시')]);
    await waitFor(() => {
      expect(trigger().textContent.trim()).toBe('출시');
    });
    expect(search.mock.calls.at(-1)?.[2]?.moduleId).toBe('m-1');
  });
});

// 클라이언트가 `moduleId`를 요청의 `module_id`로 싣는지(대화상자 시험은 `api.search`를 대신하므로 따로 본다).
describe('search client module parameter', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('sends module_id as a query parameter only when a module is chosen', async () => {
    const urlOf = (input: unknown): string =>
      input instanceof Request
        ? input.url
        : input instanceof URL
          ? input.href
          : typeof input === 'string'
            ? input
            : '';
    const fetchSpy = vi
      .spyOn(globalThis, 'fetch')
      .mockImplementation(() =>
        Promise.resolve(
          new Response('[]', { status: 200, headers: { 'content-type': 'application/json' } }),
        ),
      );
    await api.search('회의', null, { moduleId: 'm-1' });
    expect(urlOf(fetchSpy.mock.calls[0]?.[0])).toContain('module_id=m-1');
    await api.search('회의', null, {});
    expect(urlOf(fetchSpy.mock.calls[1]?.[0])).not.toContain('module_id');
  });
});
