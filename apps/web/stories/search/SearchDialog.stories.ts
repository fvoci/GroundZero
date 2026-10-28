import type { Meta, StoryObj } from '@storybook/svelte';
import { expect, spyOn, userEvent, waitFor, within } from 'storybook/test';

import type { SearchResultView } from '$lib/api/client';
import { ApiError } from '$lib/api/client';
import { router } from '$lib/router.svelte';

import { pressEscape } from '../fixtures/keys';
import SearchHost from '../fixtures/SearchHost.svelte';
import { never, setUpScreen, type ScreenApi } from '../fixtures/screens';
import { searchResults, WORKSPACE } from '../fixtures/synthetic';

// 통합 검색 창(src/lib/features/search/SearchDialog.svelte): 검색어 입력·선택 상자(종류·상태·모듈·작성자)·날짜 입력·
// 확인란, 결과·빈 결과·오류, 키보드(↑↓·Escape)와 닫은 뒤 초점 복귀. 결과는 서버가 고른 것을 그대로 보이므로 story는
// 개발 seed의 문서·업무로 만든 결과 줄을 돌려준다(검색어로 거르는 일은 서버의 일이라 여기서 하지 않는다).
// 대화상자는 body에 그려지므로(포털) 화면 전체에서 찾는다.
const ALL = searchResults();
const GUIDE = ALL[0];
if (GUIDE === undefined) throw new RangeError('no synthetic search result');

function story(overrides: Partial<ScreenApi> = {}): () => () => void {
  return () => {
    const restore = setUpScreen('/', overrides);
    // 결과를 고르면 앱은 그 경로로 옮긴다. Storybook 틀의 주소는 바꾸지 않는다.
    spyOn(router, 'navigate').mockImplementation(() => undefined);
    return restore;
  };
}

const screen = within(document.body);
// 열 때 첫 칸으로 초점 옮기기는 실제 브라우저의 초점 처리에 기대므로 jsdom 재사용(tests/component/stories.test.ts)에서는
// 확인하지 않는다.
const inBrowser = !navigator.userAgent.includes('jsdom');

async function openAndType(query: string): Promise<HTMLElement> {
  await userEvent.click(screen.getByRole('button', { name: '검색 열기' }));
  const input = await screen.findByRole('searchbox', { name: '검색어' });
  if (inBrowser) await waitFor(() => expect(input).toHaveFocus());
  if (query !== '') await userEvent.type(input, query);
  return input;
}

const meta = {
  title: 'Search/SearchDialog',
  component: SearchHost,
  args: { workspace: { id: WORKSPACE.id, name: WORKSPACE.name } },
  beforeEach: story({ search: () => Promise.resolve(ALL) }),
  globals: { theme: 'light', fontScale: '100', viewport: { value: 'desktop' } },
} satisfies Meta<typeof SearchHost>;

export default meta;
type Story = StoryObj<typeof meta>;

/** 열면 검색어 칸에 초점이 있고 필터(선택 상자·날짜·확인란)가 한 줄씩 감긴다. 검색어가 없으면 결과 안내가 없다. */
export const Opened: Story = {
  play: async () => {
    await openAndType('');
    await expect(screen.getByTestId('search-filters')).toBeInTheDocument();
    await expect(screen.queryByTestId('search-results')).toBeNull();
  },
};

/** 결과: 긴 제목은 한 줄 말줄임(가리키면 전체), 요약은 두 줄까지. ↓로 결과로, ↑로 검색어 칸으로 돌아간다. */
export const Results: Story = {
  play: async () => {
    const input = await openAndType('회의');
    await expect(await screen.findByText(`${String(ALL.length)}건`)).toBeInTheDocument();
    await userEvent.keyboard('{ArrowDown}');
    const first = screen.getByTestId('search-results').querySelector('button');
    await expect(first).toHaveFocus();
    await userEvent.keyboard('{ArrowUp}');
    await expect(input).toHaveFocus();
  },
};

/** 같은 제목이 여럿: 종류·번호로 구별된다. */
export const SameTitles: Story = {
  beforeEach: story({
    search: () =>
      Promise.resolve(
        Array.from({ length: 12 }, (_, i): SearchResultView => ({
          ...GUIDE,
          id: `same-${String(i)}`,
          reference: `WIKI-${String(100 + i)}`,
        })),
      ),
  }),
  play: async () => {
    await openAndType('연구실');
  },
};

/** 결과가 없다. */
export const NoResults: Story = {
  beforeEach: story({ search: () => Promise.resolve([]) }),
  play: async () => {
    await openAndType('없는 말');
    await expect(await screen.findByText('찾은 것이 없습니다.')).toBeInTheDocument();
  },
};

/** 응답을 기다리는 중. */
export const Searching: Story = {
  beforeEach: story({ search: () => never() }),
  play: async () => {
    await openAndType('회의');
    await expect(screen.getByText('찾는 중…')).toBeInTheDocument();
  },
};

/** 검색 실패: 이유를 알리고, 입력한 검색어는 그대로 남는다. */
export const Failed: Story = {
  beforeEach: story({
    search: () => Promise.reject(new ApiError(503, 'unavailable', '검색을 잠시 쓸 수 없습니다.')),
  }),
  play: async () => {
    const input = await openAndType('회의');
    await expect(await screen.findByText(/검색하지 못했습니다/)).toBeInTheDocument();
    await expect(input).toHaveValue('회의');
  },
};

/** 날짜 범위가 거꾸로면 검색하지 않은 것처럼 두지 않고 그렇다고 알린다. */
export const ReversedDates: Story = {
  play: async () => {
    await openAndType('회의');
    const from = screen.getByLabelText('날짜부터');
    const to = screen.getByLabelText('까지');
    await userEvent.type(from, '2026-10-25');
    await userEvent.type(to, '2026-10-01');
    await waitFor(() =>
      expect(screen.getByText('시작 날짜가 끝 날짜보다 늦습니다.')).toBeInTheDocument(),
    );
  },
};

/** Escape로 닫으면 초점이 연 단추로 돌아오고, 그 단추에서 Enter로 다시 열면 검색어가 남아 있다. */
export const EscapeReturnsFocus: Story = {
  play: async () => {
    const input = await openAndType('세미나');
    await pressEscape();
    await waitFor(() => expect(input).not.toBeInTheDocument());
    await waitFor(() => expect(screen.getByRole('button', { name: '검색 열기' })).toHaveFocus());
    await userEvent.keyboard('{Enter}');
    await expect(await screen.findByRole('searchbox', { name: '검색어' })).toHaveValue('세미나');
  },
};

export const NarrowDarkLargeText: Story = {
  ...Opened,
  globals: { theme: 'dark', fontScale: '200', viewport: { value: 'narrow375' } },
};
