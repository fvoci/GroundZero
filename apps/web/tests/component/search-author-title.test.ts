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
