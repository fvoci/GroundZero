import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { api, type CommentView } from '$lib/api/client';
import ChecklistForm from '$lib/features/checklists/ChecklistForm.svelte';
import CommentsPanel from '$lib/features/comments/CommentsPanel.svelte';
import { me } from '$lib/me.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';
import { router } from '$lib/router.svelte';

const comment: CommentView = {
  id: 'c1',
  parent_id: null,
  body: '원댓글',
  author_id: 'user-a',
  author_name: '작성자',
  created_at: '2026-10-26T00:00:00Z',
  edited: false,
  deleted: false,
  resolved: false,
  can_edit: true,
  can_delete: true,
  can_resolve: true,
};

function checklist() {
  const handlers = {
    onadd: vi.fn().mockResolvedValue(undefined),
    ontoggle: vi.fn(),
    onrename: vi.fn().mockResolvedValue(undefined),
    onremove: vi.fn(),
  };
  render(ChecklistForm, {
    idPrefix: 'task-local',
    canEdit: true,
    items: [
      { id: 'a', text: '항목 A', done: false },
      { id: 'b', text: '항목 B', done: false },
    ],
    ...handlers,
  });
  return handlers;
}

async function keep(): Promise<void> {
  const confirmation = await screen.findByRole('alertdialog');
  await fireEvent.click(within(confirmation).getByRole('button', { name: '계속 편집' }));
}

async function discard(): Promise<void> {
  const confirmation = await screen.findByRole('alertdialog');
  await fireEvent.click(within(confirmation).getByRole('button', { name: '변경 버리고 닫기' }));
}

describe('confirm only the local input being discarded', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
    me.userId = 'user-a';
    localStorage.clear();
  });
  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
    localStorage.clear();
    me.userId = '';
  });

  it('asks before cancelling a reply or edited comment, retaining the other comment input', async () => {
    vi.spyOn(api, 'comments').mockResolvedValue([comment]);
    const add = vi.spyOn(api, 'addComment');
    const edit = vi.spyOn(api, 'editComment');
    const guard = router.guard;
    render(CommentsPanel, { props: { target: { kind: 'document', id: 'd1' }, canWrite: true } });
    await screen.findByText('원댓글');
    await fireEvent.input(screen.getByLabelText(/새 댓글/), { target: { value: '새 댓글 유지' } });
    await fireEvent.click(screen.getByRole('button', { name: '답글' }));
    await fireEvent.input(screen.getByRole('textbox', { name: '답글' }), {
      target: { value: '답글 유지' },
    });
    await fireEvent.click(screen.getByRole('button', { name: '취소' }));
    await keep();
    expect(screen.getByRole('textbox', { name: '답글' })).toHaveValue('답글 유지');
    await fireEvent.click(screen.getByRole('button', { name: '취소' }));
    await discard();
    expect(screen.queryByRole('textbox', { name: '답글' })).toBeNull();
    expect(screen.getByLabelText(/새 댓글/)).toHaveValue('새 댓글 유지');
    await fireEvent.click(screen.getByRole('button', { name: '고치기' }));
    await fireEvent.input(screen.getByLabelText('댓글 고치기'), { target: { value: '수정 유지' } });
    await fireEvent.click(screen.getByRole('button', { name: '취소' }));
    await keep();
    expect(screen.getByLabelText('댓글 고치기')).toHaveValue('수정 유지');
    await fireEvent.click(screen.getByRole('button', { name: '취소' }));
    await discard();
    expect(screen.queryByLabelText('댓글 고치기')).toBeNull();
    expect(screen.getByLabelText(/새 댓글/)).toHaveValue('새 댓글 유지');
    expect(add).not.toHaveBeenCalled();
    expect(edit).not.toHaveBeenCalled();
    expect(router.guard).toBe(guard);
  });

  it('guards rename Escape and switching rows while preserving new and pasted inputs', async () => {
    checklist();
    const guard = router.guard;
    await fireEvent.input(screen.getByLabelText('새 항목'), { target: { value: '신규 유지' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Markdown 붙여넣기' }));
    await fireEvent.input(screen.getByLabelText(/붙여넣을 체크리스트/), {
      target: { value: '- [ ] 원문 유지' },
    });
    await fireEvent.click(screen.getByRole('button', { name: '항목 A 고치기' }));
    await fireEvent.input(screen.getByLabelText('항목 A의 새 문구'), {
      target: { value: '수정 A' },
    });
    await fireEvent.keyDown(screen.getByLabelText('항목 A의 새 문구'), { key: 'Escape' });
    await keep();
    expect(screen.getByLabelText('항목 A의 새 문구')).toHaveValue('수정 A');
    await fireEvent.click(screen.getByRole('button', { name: '항목 B 고치기' }));
    await keep();
    expect(screen.getByLabelText('항목 A의 새 문구')).toHaveValue('수정 A');
    await fireEvent.click(screen.getByRole('button', { name: '항목 B 고치기' }));
    await discard();
    expect(screen.getByLabelText('항목 B의 새 문구')).toHaveValue('항목 B');
    // B는 바꾸지 않았으므로 다른 초안이 있어도 즉시 취소한다.
    await fireEvent.click(screen.getByRole('button', { name: '취소' }));
    expect(screen.queryByRole('alertdialog')).toBeNull();
    expect(screen.queryByLabelText('항목 B의 새 문구')).toBeNull();
    expect(screen.getByLabelText('새 항목')).toHaveValue('신규 유지');
    expect(screen.getByLabelText(/붙여넣을 체크리스트/)).toHaveValue('- [ ] 원문 유지');
    expect(router.guard).toBe(guard);
    await waitFor(() => {
      expect(Object.values(localStorage).join('')).toContain('신규 유지');
      expect(Object.values(localStorage).join('')).not.toContain('수정 A');
    });
  });

  it('asks before replacing an edited preview but lets paste close and reopen without loss', async () => {
    checklist();
    await fireEvent.input(screen.getByLabelText('새 항목'), { target: { value: '신규 유지' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Markdown 붙여넣기' }));
    await fireEvent.input(screen.getByLabelText(/붙여넣을 체크리스트/), {
      target: { value: '- [ ] 원문' },
    });
    await fireEvent.click(screen.getByRole('button', { name: '항목으로 펼치기' }));
    await fireEvent.input(screen.getByLabelText('항목 1 내용'), {
      target: { value: '편집된 미리보기' },
    });
    await fireEvent.click(screen.getByRole('checkbox', { name: '편집된 미리보기 완료 여부' }));
    await fireEvent.click(screen.getByRole('button', { name: '항목으로 펼치기' }));
    await keep();
    expect(screen.getByLabelText('항목 1 내용')).toHaveValue('편집된 미리보기');
    expect(screen.getByRole('checkbox', { name: '편집된 미리보기 완료 여부' })).toBeChecked();
    await fireEvent.click(screen.getByRole('button', { name: 'Markdown 붙여넣기' }));
    expect(screen.queryByRole('alertdialog')).toBeNull();
    await fireEvent.click(screen.getByRole('button', { name: 'Markdown 붙여넣기' }));
    expect(screen.getByLabelText('항목 1 내용')).toHaveValue('편집된 미리보기');
    await fireEvent.click(screen.getByRole('button', { name: '항목으로 펼치기' }));
    await discard();
    expect(screen.getByLabelText('항목 1 내용')).toHaveValue('원문');
    expect(screen.getByRole('checkbox', { name: '원문 완료 여부' })).not.toBeChecked();
    expect(screen.getByLabelText('새 항목')).toHaveValue('신규 유지');
  });

  it('closes unchanged comment editors without asking despite other dirty input', async () => {
    vi.spyOn(api, 'comments').mockResolvedValue([comment]);
    render(CommentsPanel, { props: { target: { kind: 'document', id: 'd1' }, canWrite: true } });
    await screen.findByText('원댓글');
    await fireEvent.input(screen.getByLabelText(/새 댓글/), { target: { value: '다른 입력' } });
    await fireEvent.click(screen.getByRole('button', { name: '답글' }));
    await fireEvent.click(screen.getByRole('button', { name: '취소' }));
    expect(screen.queryByRole('alertdialog')).toBeNull();
    await fireEvent.click(screen.getByRole('button', { name: '고치기' }));
    await fireEvent.click(screen.getByRole('button', { name: '취소' }));
    expect(screen.queryByRole('alertdialog')).toBeNull();
    expect(screen.getByLabelText(/새 댓글/)).toHaveValue('다른 입력');
  });

  it('keeps a blank rename on save and asks on cancel even without storage', async () => {
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new Error('full');
    });
    const h = checklist();
    await fireEvent.click(screen.getByRole('button', { name: '항목 A 고치기' }));
    await fireEvent.input(screen.getByLabelText('항목 A의 새 문구'), { target: { value: '  ' } });
    await fireEvent.click(screen.getByRole('button', { name: '저장' }));
    expect(screen.getByLabelText('항목 A의 새 문구')).toHaveValue('  ');
    expect(h.onrename).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByRole('button', { name: '취소' }));
    await keep();
    expect(screen.getByLabelText('항목 A의 새 문구')).toHaveValue('  ');
  });

  it('does not carry a pending discard into the next account', async () => {
    checklist();
    await fireEvent.click(screen.getByRole('button', { name: '항목 A 고치기' }));
    await fireEvent.input(screen.getByLabelText('항목 A의 새 문구'), {
      target: { value: 'A의 수정' },
    });
    await fireEvent.click(screen.getByRole('button', { name: '취소' }));
    await screen.findByRole('alertdialog');
    me.userId = 'user-b';
    await waitFor(() => {
      expect(screen.queryByRole('alertdialog')).toBeNull();
    });
    expect(screen.queryByLabelText('항목 A의 새 문구')).toBeNull();
    me.userId = 'user-a';
    expect(await screen.findByTestId('checklist-new-item-draft-offer')).toHaveTextContent(
      'A의 수정',
    );
    await fireEvent.click(screen.getByRole('button', { name: '입력 되살리기' }));
    expect(screen.getByLabelText('항목 A의 새 문구')).toHaveValue('A의 수정');
    expect(screen.queryByRole('alertdialog')).toBeNull();
  });
});
