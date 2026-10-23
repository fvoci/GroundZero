import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import SearchDialog from '$lib/features/search/SearchDialog.svelte';
import { me } from '$lib/me.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

// D-101: 검색 날짜의 하루는 서버가 설정의 시간대로 정한다. 화면은 날짜(YYYY-MM-DD)를 그대로 보내고 설정의 시간대를
// `tz`로 함께 보낸다(경계를 스스로 계산하지 않는다). 날짜를 고르지 않으면 시간대도 보내지 않는다.
describe('search date range time zone', () => {
  const searched: URL[] = [];
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
    me.timeZone = 'America/New_York';
    searched.length = 0;
    vi.stubGlobal('fetch', (input: RequestInfo | URL) => {
      const url = new URL(input instanceof Request ? input.url : String(input), 'http://web.test');
      if (url.pathname === '/api/search') searched.push(url);
      return Promise.resolve(
        new Response('[]', { status: 200, headers: { 'Content-Type': 'application/json' } }),
      );
    });
  });
  afterEach(() => {
    me.timeZone = '';
    vi.unstubAllGlobals();
  });

  it('sends the dates as typed with the settings time zone', async () => {
    render(SearchDialog, { props: { open: true, memberOf: () => true } });
    await fireEvent.input(screen.getByTestId('search-input'), { target: { value: '회의' } });
    await waitFor(() => {
      expect(searched).toHaveLength(1);
    });
    expect(searched[0]?.searchParams.has('tz')).toBe(false);

    const from = document.querySelector<HTMLInputElement>('#search-from');
    if (from === null) throw new Error('from input');
    await fireEvent.input(from, { target: { value: '2026-03-08' } });
    await fireEvent.change(from);
    await waitFor(() => {
      expect(searched.at(-1)?.searchParams.get('from')).toBe('2026-03-08');
    });
    const last = searched.at(-1)?.searchParams;
    expect(last?.get('tz')).toBe('America/New_York');
    expect(last?.has('to')).toBe(false);
  });
});
