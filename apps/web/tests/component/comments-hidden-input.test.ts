import { activityApi, type CommentView } from '$lib/features/activity/api';
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import CommentsPanel from '$lib/features/comments/CommentsPanel.svelte';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';

// #289: 쓸 수 없게 되면(`writable`·`can_edit` 거짓) 댓글·답글·수정 입력이 숨는다. 숨은 입력은 부모의 이탈 경고에
// 남지 않아야 한다. 긴 입력이므로 영구 초안은 지우지 않고 되살리기 제안으로 남긴다(D-27, R5 F-11). 같은 패널에서
// 계속 보이는 입력은 그대로 보호한다.

const target = { kind: 'document' as const, id: 'doc-hidden' };
const comment = (id: string, canEdit = true): CommentView => ({
  id,
  parent_id: null,
  body: `원댓글 ${id}`,
  author_id: 'user-a',
  author_name: '작성자',
  created_at: '2026-10-26T00:00:00Z',
  edited: false,
  deleted: false,
  resolved: false,
  can_edit: canEdit,
  can_delete: false,
  can_resolve: true,
});

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

it('moves a hidden new comment into a kept offer when writing ends', async () => {
  const add = vi.spyOn(activityApi, 'addComment');
  const unsaved = vi.fn();
  const view = render(CommentsPanel, { props: { target, canWrite: true, onunsaved: unsaved } });
  await fireEvent.input(screen.getByLabelText(/새 댓글/), { target: { value: '숨을 긴 댓글' } });
  expect(unsaved).toHaveBeenLastCalledWith(true);
  await view.rerender({ target, canWrite: false, onunsaved: unsaved });
  expect(screen.queryByLabelText(/새 댓글/)).toBeNull();
  await waitFor(() => {
    expect(unsaved).toHaveBeenLastCalledWith(false);
  });
  // 남긴 입력은 지우지 않는다: 쓸 수 없는 동안 안내와 버리기만 보인다.
  const offer = await screen.findByTestId('comment-draft-offer');
  expect(offer).toHaveTextContent('숨을 긴 댓글');
  expect(offer).toHaveTextContent(m.common_draft_read_only());
  await view.rerender({ target, canWrite: true, onunsaved: unsaved });
  expect(screen.getByLabelText(/새 댓글/)).toHaveValue('');
  await fireEvent.click(
    within(screen.getByTestId('comment-draft-offer')).getByRole('button', {
      name: m.common_draft_restore(),
    }),
  );
  expect(screen.getByLabelText(/새 댓글/)).toHaveValue('숨을 긴 댓글');
  expect(add).not.toHaveBeenCalled();
});

it('keeps a sent comment locked until its response, then keeps the failed input as an offer', async () => {
  let reject: (e: Error) => void = () => undefined;
  vi.spyOn(activityApi, 'addComment').mockReturnValue(
    new Promise((_, fail) => {
      reject = fail;
    }),
  );
  const unsaved = vi.fn();
  const view = render(CommentsPanel, { props: { target, canWrite: true, onunsaved: unsaved } });
  await fireEvent.input(screen.getByLabelText(/새 댓글/), { target: { value: '보내는 댓글' } });
  await fireEvent.click(screen.getByRole('button', { name: m.comments_post() }));
  await view.rerender({ target, canWrite: false, onunsaved: unsaved });
  expect(screen.queryByLabelText(/새 댓글/)).toBeNull();
  // 응답 전에는 옮기지 않는다.
  expect(unsaved).toHaveBeenLastCalledWith(true);
  expect(screen.queryByTestId('comment-draft-offer')).toBeNull();
  reject(new Error('forbidden'));
  await waitFor(() => {
    expect(unsaved).toHaveBeenLastCalledWith(false);
  });
  expect(await screen.findByTestId('comment-draft-offer')).toHaveTextContent('보내는 댓글');
});

it('moves a hidden reply into a kept offer when writing ends', async () => {
  vi.mocked(activityApi.comments).mockResolvedValue([comment('c1')]);
  const unsaved = vi.fn();
  const view = render(CommentsPanel, { props: { target, canWrite: true, onunsaved: unsaved } });
  const root = (await screen.findByText('원댓글 c1')).closest('li');
  if (root === null) throw new Error('comment root');
  await fireEvent.click(within(root).getByRole('button', { name: m.comments_reply() }));
  await fireEvent.input(within(root).getByLabelText(m.comments_reply()), {
    target: { value: '숨을 답글' },
  });
  expect(unsaved).toHaveBeenLastCalledWith(true);
  await view.rerender({ target, canWrite: false, onunsaved: unsaved });
  await waitFor(() => {
    expect(unsaved).toHaveBeenLastCalledWith(false);
  });
  expect(await screen.findByTestId('comment-reply-draft-offer')).toHaveTextContent('숨을 답글');
});

it('moves a hidden comment edit into a kept offer while guarding the visible new comment', async () => {
  vi.mocked(activityApi.comments).mockResolvedValue([comment('c1')]);
  const resolve = vi.spyOn(activityApi, 'resolveComment').mockResolvedValue(undefined);
  const edit = vi.spyOn(activityApi, 'editComment');
  const unsaved = vi.fn();
  render(CommentsPanel, { props: { target, canWrite: true, onunsaved: unsaved } });
  const root = (await screen.findByText('원댓글 c1')).closest('li');
  if (root === null) throw new Error('comment root');
  await fireEvent.click(within(root).getByRole('button', { name: m.comments_edit() }));
  await fireEvent.input(screen.getByLabelText(m.comments_edit_label()), {
    target: { value: '숨을 수정 본문' },
  });
  await fireEvent.input(screen.getByLabelText(/새 댓글/), {
    target: { value: '보이는 새 댓글' },
  });
  // 다시 읽은 목록에서 이 댓글을 더는 고칠 수 없다(작성자 권한 상실 등).
  vi.mocked(activityApi.comments).mockResolvedValue([comment('c1', false)]);
  await fireEvent.click(within(root).getByRole('button', { name: m.comments_resolve() }));
  await waitFor(() => {
    expect(screen.queryByLabelText(m.comments_edit_label())).toBeNull();
  });
  expect(resolve).toHaveBeenCalledOnce();
  // 새 댓글은 계속 보이므로 경고가 남는다.
  expect(unsaved).toHaveBeenLastCalledWith(true);
  await fireEvent.input(screen.getByLabelText(/새 댓글/), { target: { value: '' } });
  await waitFor(() => {
    expect(unsaved).toHaveBeenLastCalledWith(false);
  });
  expect(await screen.findByTestId('comment-edit-draft-offer')).toHaveTextContent(
    '숨을 수정 본문',
  );
  expect(edit).not.toHaveBeenCalled();
});
