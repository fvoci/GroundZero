import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { SearchFilters } from '$lib/api/client';
import { api } from '$lib/api/client';
import SearchDialog from '$lib/features/search/SearchDialog.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

// 검색의 작성자·제목만: 고른 조건이 요청 인자로 가고, 작성자 목록은 지금 워크스페이스의 멤버다.
const urlOf = (input: unknown): string =>
  input instanceof Request
    ? input.url
    : input instanceof URL
      ? input.href
      : typeof input === 'string'
        ? input
        : '';

describe('search author and title only', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
  });
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('sends author and title_only as query parameters', async () => {
    const fetchSpy = vi
      .spyOn(globalThis, 'fetch')
      .mockImplementation(() =>
        Promise.resolve(
          new Response('[]', { status: 200, headers: { 'content-type': 'application/json' } }),
        ),
      );
    await api.search('회의', 'ws-1', { author: 'me', titleOnly: true });
    const url = urlOf(fetchSpy.mock.calls[0]?.[0]);
    expect(url).toContain('author=me');
    expect(url).toContain('title_only=true');
    await api.search('회의', null, {});
    const plain = urlOf(fetchSpy.mock.calls[1]?.[0]);
    expect(plain).not.toContain('author=');
    expect(plain).not.toContain('title_only');
  });

  it('reads the members of the current workspace and searches titles only when ticked', async () => {
    const members = vi.spyOn(api, 'members').mockResolvedValue({
      members: [
        { user_id: 'u-1', display_name: '가상 관리자', email: null, role: 'admin', is_owner: true },
      ],
      my_role: 'admin',
      i_am_owner: true,
      can_manage_members: true,
    });
    const search = vi.spyOn(api, 'search').mockResolvedValue([]);
    render(SearchDialog, {
      props: { open: true, memberOf: () => true, workspace: { id: 'ws-1', name: '공간' } },
    });
    await waitFor(() => {
      expect(members).toHaveBeenCalledWith('ws-1');
    });
    expect(screen.getByText(/작성자는 그 문서·업무 등을 만든 사람입니다/)).toBeTruthy();
    await fireEvent.input(screen.getByTestId('search-input'), { target: { value: '회의' } });
    await fireEvent.click(screen.getByRole('checkbox', { name: '제목만' }));
    await waitFor(() => {
      const last: SearchFilters | undefined = search.mock.calls.at(-1)?.[2];
      expect(last?.titleOnly).toBe(true);
    });
    expect(search.mock.calls.at(-1)?.[2]).not.toHaveProperty('author');
  });
});

describe('search author that is no longer in the member list', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
    // jsdom에 없는 포인터 잡기·스크롤(고르기 목록이 쓴다).
    Element.prototype.hasPointerCapture = (): boolean => false;
    Element.prototype.setPointerCapture = (): void => undefined;
    Element.prototype.releasePointerCapture = (): void => undefined;
    Element.prototype.scrollIntoView = (): void => undefined;
  });
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('drops the chosen person when the next workspace does not have them', async () => {
    const members = vi
      .spyOn(api, 'members')
      .mockResolvedValueOnce({
        members: [
          {
            user_id: 'u-2',
            display_name: '가상 멤버',
            email: null,
            role: 'writer',
            is_owner: false,
          },
        ],
        my_role: 'admin',
        i_am_owner: true,
        can_manage_members: true,
      })
      .mockResolvedValue({
        members: [],
        my_role: 'admin',
        i_am_owner: true,
        can_manage_members: true,
      });
    const search = vi.spyOn(api, 'search').mockResolvedValue([]);
    const view = render(SearchDialog, {
      props: { open: true, memberOf: () => true, workspace: { id: 'ws-1', name: '공간' } },
    });
    await waitFor(() => {
      expect(members).toHaveBeenCalledWith('ws-1');
    });
    const trigger = document.querySelector<HTMLElement>('[aria-labelledby="search-author-label"]');
    if (trigger === null) throw new Error('author trigger');
    await fireEvent.pointerDown(trigger, { button: 0, pointerType: 'mouse' });
    await fireEvent.click(trigger);
    const option = await screen.findByRole('option', { name: '가상 멤버' });
    await fireEvent.pointerUp(option, { button: 0, pointerType: 'mouse' });
    await fireEvent.click(option);
    await fireEvent.input(screen.getByTestId('search-input'), { target: { value: '회의' } });
    await waitFor(() => {
      const last: SearchFilters | undefined = search.mock.calls.at(-1)?.[2];
      expect(last?.author).toBe('u-2');
    });
    await view.rerender({
      open: true,
      memberOf: () => true,
      workspace: { id: 'ws-2', name: '다른 공간' },
    });
    await waitFor(() => {
      expect(members).toHaveBeenCalledWith('ws-2');
    });
    await waitFor(() => {
      const last: SearchFilters | undefined = search.mock.calls.at(-1)?.[2];
      expect(last?.author).toBeUndefined();
    });
    expect(trigger.textContent.trim()).toBe('누구나');
  });

  it('keeps showing the chosen person while the member list reloads on reopening', async () => {
    // 같은 워크스페이스에서 닫았다 다시 열면 멤버 목록을 다시 읽는다. 그 사이에도 작성자 거르기는 남으므로 버튼도 고른
    // 사람을 보여야 한다. 전에는 목록이 올 때까지 '누구나'로 보였는데 검색은 그 사람으로 좁혀졌다.
    vi.spyOn(api, 'modules').mockResolvedValue([]);
    const roster: Awaited<ReturnType<typeof api.members>> = {
      members: [
        { user_id: 'u-1', display_name: '가상 멤버', email: null, role: 'writer', is_owner: false },
      ],
      my_role: 'admin',
      i_am_owner: true,
    };
    const members = vi.spyOn(api, 'members').mockResolvedValue(roster);
    const search = vi.spyOn(api, 'search').mockResolvedValue([]);
    const dialog = render(SearchDialog, {
      props: { open: true, memberOf: () => true, workspace: { id: 'ws-1', name: '공간' } },
    });
    const trigger = (): HTMLElement => {
      const all = document.querySelectorAll<HTMLElement>('[aria-labelledby="search-author-label"]');
      expect(all.length).toBe(1);
      const el = all[0];
      if (el === undefined) throw new Error('author trigger');
      return el;
    };
    await waitFor(() => {
      expect(members).toHaveBeenCalledTimes(1);
    });
    await fireEvent.pointerDown(trigger(), { button: 0, pointerType: 'mouse' });
    await fireEvent.click(trigger());
    const option = await screen.findByRole('option', { name: '가상 멤버' });
    await fireEvent.pointerUp(option, { button: 0, pointerType: 'mouse' });
    await fireEvent.click(option);
    await fireEvent.input(screen.getByTestId('search-input'), { target: { value: '회의' } });
    await waitFor(() => {
      expect(search.mock.calls.at(-1)?.[2]?.author).toBe('u-1');
    });
    expect(trigger().textContent.trim()).toBe('가상 멤버');
    await dialog.rerender({
      open: false,
      memberOf: () => true,
      workspace: { id: 'ws-1', name: '공간' },
    });
    // 다시 열 때의 멤버 응답을 미룬다.
    let giveMembers: (v: Awaited<ReturnType<typeof api.members>>) => void = () => undefined;
    members.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          giveMembers = resolve;
        }),
    );
    await dialog.rerender({
      open: true,
      memberOf: () => true,
      workspace: { id: 'ws-1', name: '공간' },
    });
    await waitFor(() => {
      expect(members).toHaveBeenCalledTimes(2);
    });
    expect(trigger().textContent.trim()).toBe('가상 멤버');
    // 목록이 오기 전에 찾아도 버튼이 말하는 대로 그 사람으로 좁힌다.
    await fireEvent.input(screen.getByTestId('search-input'), { target: { value: '회의록' } });
    await waitFor(() => {
      expect(search.mock.calls.at(-1)?.[0]).toBe('회의록');
    });
    expect(search.mock.calls.at(-1)?.[2]?.author).toBe('u-1');
    expect(trigger().textContent.trim()).toBe('가상 멤버');
    giveMembers(roster);
    await waitFor(() => {
      expect(trigger().textContent.trim()).toBe('가상 멤버');
    });
    expect(search.mock.calls.at(-1)?.[2]?.author).toBe('u-1');
  });
});
