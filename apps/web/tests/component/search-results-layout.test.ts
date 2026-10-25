import { fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { api } from '$lib/api/client';
import SearchDialog from '$lib/features/search/SearchDialog.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

// 실제 브라우저(Storybook Search/SearchDialog)에서 확인한 넘침의 회귀 시험. 대화상자 본문은 격자라 칸 폭이 내용 폭을
// 따른다. 줄바꿈하지 않는 긴 제목(truncate)이 칸을 대화상자보다 넓혀 설명·확인란·요약이 오른쪽에서 잘리고 가로 스크롤이
// 생겼다. jsdom은 배치를 하지 않으므로 그 배치를 정하는 클래스를 확인한다.
describe('search result layout', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
  });
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('lets the result list shrink so long titles truncate inside the dialog', async () => {
    const title =
      '2026학년도 2학기 대학원 연구실 공동 세미나 발표 자료와 회의록을 한곳에 모아 정리하는 작업 공간';
    vi.spyOn(api, 'search').mockResolvedValue([
      {
        kind: 'document',
        id: 'd1',
        title,
        reference: 'WIKI-8',
        workspace_id: 'ws-1',
        item_id: null,
        document_id: 'd1',
        snippet:
          'https://example.test/workspaces/aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee/documents/ffff',
      },
    ]);
    render(SearchDialog, { props: { open: true, memberOf: () => true } });
    await fireEvent.input(screen.getByTestId('search-input'), { target: { value: '세미나' } });
    const list = await screen.findByTestId('search-results');
    expect(list).toHaveClass('min-w-0');
    expect(screen.getByText(title)).toHaveClass('truncate');
    expect(screen.getByText(title)).toHaveAttribute('title', title);
  });
});
