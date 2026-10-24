import { render, screen } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { api } from '$lib/api/client';
import CommentsPanel from '$lib/features/comments/CommentsPanel.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

afterEach(() => {
  vi.restoreAllMocks();
});

// 목록을 받기 전에는 "댓글이 없습니다"가 아니라 불러오는 중이라고 보인다(Storybook에서 발견: 받기 전에도 빈 목록 글이 보였다).
describe('comments loading state', () => {
  it('says loading until the first list arrives, then shows the empty text', async () => {
    await setLocale('ko', { reload: false });
    let resolve: (list: []) => void = () => undefined;
    vi.spyOn(api, 'comments').mockReturnValue(
      new Promise((r) => {
        resolve = r;
      }),
    );
    render(CommentsPanel, { props: { target: { kind: 'document', id: 'doc-1' } } });

    expect(screen.getByText('불러오는 중…')).toBeInTheDocument();
    expect(screen.queryByText('댓글이 없습니다.')).toBeNull();

    resolve([]);
    expect(await screen.findByText('댓글이 없습니다.')).toBeInTheDocument();
    expect(screen.queryByText('불러오는 중…')).toBeNull();
  });
});
