import { activityApi } from '$lib/features/activity/api';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { NetworkError } from '$lib/api/client';
import CommentsPanel from '$lib/features/comments/CommentsPanel.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';
import { me } from '$lib/me.svelte';

beforeEach(() => {
  me.userId = 'user-a';
  localStorage.clear();
});
afterEach(() => {
  vi.restoreAllMocks();
  me.userId = '';
  localStorage.clear();
});

// 응답을 잃은 뒤 글을 고쳐 다시 달면 새 요청 id여야 한다. 같은 id면 서버가 처음 댓글을 돌려줘 고친 글이 사라진다.
// 고치지 않고 다시 누르면 같은 id(중복 댓글 없음). 제품 로직 점검 2번 불변식.
describe('comment retry', () => {
  it('uses a new request id after the text is edited, the same id when retried as is', async () => {
    await setLocale('ko', { reload: false });
    vi.spyOn(activityApi, 'comments').mockResolvedValue([]);
    const add = vi
      .spyOn(activityApi, 'addComment')
      .mockRejectedValueOnce(new NetworkError('lost'))
      .mockRejectedValueOnce(new NetworkError('lost'))
      .mockResolvedValue('c1');
    render(CommentsPanel, { props: { target: { kind: 'document', id: 'doc-1' }, canWrite: true } });
    const box = screen.getByLabelText(/새 댓글/);
    const submit = screen.getByRole('button', { name: '댓글 달기' });

    await fireEvent.input(box, { target: { value: '처음 글' } });
    await fireEvent.click(submit);
    await waitFor(() => {
      expect(add).toHaveBeenCalledTimes(1);
    });
    await fireEvent.click(submit);
    await waitFor(() => {
      expect(add).toHaveBeenCalledTimes(2);
    });
    await fireEvent.input(box, { target: { value: '고친 글' } });
    await fireEvent.click(submit);
    await waitFor(() => {
      expect(add).toHaveBeenCalledTimes(3);
    });

    const ids = add.mock.calls.map((call) => call[3]);
    expect(ids[1]).toBe(ids[0]);
    expect(ids[2]).not.toBe(ids[0]);
    expect(add.mock.calls[2]?.[1]).toBe('고친 글');
  });
});
