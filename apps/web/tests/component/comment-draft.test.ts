import { activityApi, type CommentView } from '$lib/features/activity/api';
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { NetworkError } from '$lib/api/client';
import CommentsPanel from '$lib/features/comments/CommentsPanel.svelte';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';

const props = { target: { kind: 'document' as const, id: 'doc-1' }, canWrite: true };
const comment = (id: string): CommentView => ({
  id,
  parent_id: null,
  body: `원댓글 ${id}`,
  author_id: 'user-a',
  author_name: '작성자',
  created_at: '2026-10-26T00:00:00Z',
  edited: false,
  deleted: false,
  resolved: false,
  can_edit: true,
  can_delete: true,
  can_resolve: true,
});

describe('comment drafts', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
    localStorage.clear();
    me.userId = 'user-a';
    vi.spyOn(activityApi, 'comments').mockResolvedValue([]);
  });
  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
    localStorage.clear();
    me.userId = '';
  });

  it('offers a failed new comment after remount and preserves its retry id', async () => {
    const add = vi.spyOn(activityApi, 'addComment').mockRejectedValue(new NetworkError('offline'));
    const view = render(CommentsPanel, { props });
    await fireEvent.input(screen.getByLabelText(/새 댓글/), { target: { value: '남길 댓글' } });
    await fireEvent.click(screen.getByRole('button', { name: '댓글 달기' }));
    await waitFor(() => {
      expect(add).toHaveBeenCalledTimes(1);
    });
    view.unmount();
    render(CommentsPanel, { props });
    expect(screen.getByLabelText(/새 댓글/)).toHaveValue('');
    const offer = await screen.findByTestId('comment-draft-offer');
    expect(offer).toHaveTextContent('남길 댓글');
    await fireEvent.click(screen.getByRole('button', { name: '입력 되살리기' }));
    expect(screen.getByLabelText(/새 댓글/)).toHaveValue('남길 댓글');
    expect(add).toHaveBeenCalledTimes(1);
    add.mockResolvedValueOnce('c-1');
    await fireEvent.click(screen.getByRole('button', { name: '댓글 달기' }));
    await waitFor(() => {
      expect(add).toHaveBeenCalledTimes(2);
    });
    expect(add.mock.calls[1]?.[3]).toBe(add.mock.calls[0]?.[3]);
    await waitFor(() => {
      expect(screen.getByLabelText(/새 댓글/)).toHaveValue('');
    });
    view.unmount();
    cleanup();
    render(CommentsPanel, { props });
    expect(await screen.findByTestId('comment-draft-offer')).toHaveTextContent('남길 댓글');
    expect(screen.getByLabelText(/새 댓글/)).toHaveValue('');
    expect(add).toHaveBeenCalledTimes(2);
  });

  it('hides the original account input on a live switch and offers it only to that account', async () => {
    const unsaved = vi.fn();
    const add = vi.spyOn(activityApi, 'addComment');
    render(CommentsPanel, { props: { ...props, onunsaved: unsaved } });
    await fireEvent.input(screen.getByLabelText(/새 댓글/), {
      target: { value: 'A의 비공개 입력' },
    });
    await waitFor(() => {
      expect(unsaved).toHaveBeenLastCalledWith(true);
    });
    me.userId = 'user-b';
    await waitFor(() => {
      expect(screen.getByLabelText(/새 댓글/)).toHaveValue('');
    });
    expect(screen.queryByTestId('comment-draft-offer')).toBeNull();
    expect(Object.keys(localStorage).filter((key) => key.includes('user-b'))).toEqual([]);
    me.userId = 'user-a';
    expect(await screen.findByTestId('comment-draft-offer')).toHaveTextContent('A의 비공개 입력');
    await fireEvent.click(screen.getByRole('button', { name: '입력 되살리기' }));
    expect(screen.getByLabelText(/새 댓글/)).toHaveValue('A의 비공개 입력');
    expect(add).not.toHaveBeenCalled();
  });

  it('keeps independent replies and a failed existing-comment edit across remount', async () => {
    vi.mocked(activityApi.comments).mockResolvedValue([comment('c1'), comment('c2')]);
    const edit = vi
      .spyOn(activityApi, 'editComment')
      .mockRejectedValue(new NetworkError('offline'));
    const add = vi.spyOn(activityApi, 'addComment');
    const unsaved = vi.fn();
    const view = render(CommentsPanel, { props: { ...props, onunsaved: unsaved } });
    for (const id of ['c1', 'c2']) {
      const root = (await screen.findByText(`원댓글 ${id}`)).closest('li');
      if (root === null) throw new Error('comment root');
      const row = within(root);
      await fireEvent.click(row.getByRole('button', { name: '답글' }));
      await fireEvent.input(row.getByRole('textbox', { name: '답글' }), {
        target: { value: `${id} 답글 초안` },
      });
    }
    const root = screen.getByText('원댓글 c1').closest('li');
    if (root === null) throw new Error('comment root');
    await fireEvent.click(within(root).getByRole('button', { name: '고치기' }));
    await fireEvent.input(screen.getByLabelText('댓글 고치기'), {
      target: { value: '수정 중인 원댓글' },
    });
    await fireEvent.click(screen.getByRole('button', { name: '저장' }));
    await waitFor(() => {
      expect(edit).toHaveBeenCalledTimes(1);
    });
    await waitFor(() => {
      expect(unsaved).toHaveBeenLastCalledWith(true);
    });
    view.unmount();
    render(CommentsPanel, { props });
    expect(await screen.findAllByTestId('comment-reply-draft-offer')).toHaveLength(2);
    expect(await screen.findByTestId('comment-edit-draft-offer')).toHaveTextContent(
      '수정 중인 원댓글',
    );
    for (const button of screen.getAllByRole('button', { name: '입력 되살리기' }))
      await fireEvent.click(button);
    expect(screen.getByLabelText('댓글 고치기')).toHaveValue('수정 중인 원댓글');
    expect(
      screen.getAllByRole('textbox', { name: '답글' }).map((box) => {
        if (!(box instanceof HTMLTextAreaElement)) throw new Error('reply textarea');
        return box.value;
      }),
    ).toEqual(['c1 답글 초안', 'c2 답글 초안']);
    expect(edit).toHaveBeenCalledTimes(1);
    expect(add).not.toHaveBeenCalled();
  });

  it('retains input when access is revoked and when a late success follows more typing', async () => {
    let resolve: (id: string) => void = () => undefined;
    vi.spyOn(activityApi, 'addComment').mockReturnValue(
      new Promise((done) => {
        resolve = done;
      }),
    );
    const view = render(CommentsPanel, { props });
    await fireEvent.input(screen.getByLabelText(/새 댓글/), { target: { value: '전송 중' } });
    await fireEvent.click(screen.getByRole('button', { name: '댓글 달기' }));
    await fireEvent.input(screen.getByLabelText(/새 댓글/), { target: { value: '더 쓴 입력' } });
    resolve('c1');
    await waitFor(() =>
      expect(screen.getByRole('button', { name: '댓글 달기' })).not.toBeDisabled(),
    );
    expect(screen.getByLabelText(/새 댓글/)).toHaveValue('더 쓴 입력');
    await view.rerender({ ...props, canWrite: false });
    expect(screen.queryByLabelText(/새 댓글/)).toBeNull();
    view.unmount();
    render(CommentsPanel, { props });
    expect(await screen.findByTestId('comment-draft-offer')).toHaveTextContent('더 쓴 입력');
  });

  // R5 F-11: 쓸 수 없는 동안은 되살리기 대신 읽기 전용 안내와 버리기만 보인다. 입력은 버리기 전까지 남는다.
  it('offers only discard with a read-only notice while comments cannot be written', async () => {
    vi.mocked(activityApi.comments).mockResolvedValue([comment('c1')]);
    const edit = vi
      .spyOn(activityApi, 'editComment')
      .mockRejectedValue(new NetworkError('offline'));
    const first = render(CommentsPanel, { props });
    await fireEvent.input(screen.getByLabelText(/새 댓글/), {
      target: { value: '읽기 전용 입력' },
    });
    const root = (await screen.findByText('원댓글 c1')).closest('li');
    if (root === null) throw new Error('comment root');
    await fireEvent.click(within(root).getByRole('button', { name: '고치기' }));
    await fireEvent.input(screen.getByLabelText('댓글 고치기'), {
      target: { value: '고친 원댓글' },
    });
    await fireEvent.click(screen.getByRole('button', { name: '저장' }));
    await waitFor(() => {
      expect(edit).toHaveBeenCalledTimes(1);
    });
    first.unmount();
    const view = render(CommentsPanel, { props: { ...props, canWrite: false } });
    const offer = await screen.findByTestId('comment-draft-offer');
    const editOffer = await screen.findByTestId('comment-edit-draft-offer');
    for (const box of [offer, editOffer]) {
      expect(within(box).queryByRole('button', { name: m.common_draft_restore() })).toBeNull();
      expect(box).toHaveTextContent(m.common_draft_read_only());
      expect(within(box).getByRole('button', { name: m.common_draft_discard() })).toBeEnabled();
    }
    await view.rerender({ ...props, canWrite: true });
    expect(
      within(screen.getByTestId('comment-draft-offer')).getByRole('button', {
        name: m.common_draft_restore(),
      }),
    ).toBeInTheDocument();
    await view.rerender({ ...props, canWrite: false });
    await fireEvent.click(
      within(screen.getByTestId('comment-draft-offer')).getByRole('button', {
        name: m.common_draft_discard(),
      }),
    );
    expect(screen.queryByTestId('comment-draft-offer')).toBeNull();
  });

  it('still posts and reports unsaved input when browser storage is unavailable', async () => {
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new Error('unavailable');
    });
    const add = vi.spyOn(activityApi, 'addComment').mockResolvedValue('c1');
    const unsaved = vi.fn();
    render(CommentsPanel, { props: { ...props, onunsaved: unsaved } });
    await fireEvent.input(screen.getByLabelText(/새 댓글/), { target: { value: '메모' } });
    expect(unsaved).toHaveBeenLastCalledWith(true);
    await fireEvent.click(screen.getByRole('button', { name: '댓글 달기' }));
    await waitFor(() => {
      expect(add).toHaveBeenCalledTimes(1);
    });
    await waitFor(() => {
      expect(screen.getByLabelText(/새 댓글/)).toHaveValue('');
    });
    expect(unsaved).toHaveBeenLastCalledWith(false);
  });

  it('does not overwrite or clear another tab when this tab saves', async () => {
    vi.spyOn(activityApi, 'addComment').mockResolvedValue('c1');
    sessionStorage.setItem('fvoci:tab', 'first-tab');
    const first = render(CommentsPanel, { props });
    const a = within(first.container);
    sessionStorage.setItem('fvoci:tab', 'second-tab');
    const second = render(CommentsPanel, { props });
    const b = within(second.container);
    await fireEvent.input(a.getByLabelText(/새 댓글/), { target: { value: '첫 탭' } });
    await fireEvent.input(b.getByLabelText(/새 댓글/), { target: { value: '둘째 탭' } });
    const secondKey = Object.keys(localStorage).find((key) => key.endsWith(':tab:second-tab'));
    if (secondKey === undefined) throw new Error('second tab draft');
    const saved = localStorage.getItem(secondKey);
    await fireEvent.click(a.getByRole('button', { name: '댓글 달기' }));
    await waitFor(() => {
      expect(a.getByLabelText(/새 댓글/)).toHaveValue('');
    });
    expect(b.getByLabelText(/새 댓글/)).toHaveValue('둘째 탭');
    expect(localStorage.getItem(secondKey)).toBe(saved);
  });
});
