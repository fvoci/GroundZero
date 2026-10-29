import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { DocumentView, SummaryView } from '$lib/api/client';
import { api, ApiError } from '$lib/api/client';
import DocumentTree from '$lib/features/documents/DocumentTree.svelte';
import NotesScreen from '$lib/features/notes/NotesScreen.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

afterEach(() => {
  vi.restoreAllMocks();
});

function summary(id: string, title: string): SummaryView {
  return {
    id,
    title,
    parent_id: null,
    reference: null,
    revision: 1,
    updated_at: '2026-10-19T00:00:00Z',
  };
}

// 위키·문서 트리: 옮기기가 실패하면(권한 철회·충돌·네트워크) 조용히 끝나지 않고 알린다. 트리는 서버가 받은 뒤에만
// 바뀌므로 그대로 남는다(부르는 쪽이 최신 목록을 다시 읽는다).
describe('document tree move failure', () => {
  it('tells the person the move failed and keeps the order', async () => {
    await setLocale('ko', { reload: false });
    const onmove = vi.fn(() => Promise.reject(new ApiError(403, 'forbidden', 'no')));
    render(DocumentTree, {
      props: {
        docs: [summary('a', '첫 문서'), summary('b', '둘째 문서')],
        label: '위키',
        empty: '문서가 없습니다.',
        hrefFor: (id: string) => `/d/${id}`,
        navigate: () => undefined,
        canWrite: true,
        onmove,
        oncreatechild: () => Promise.resolve(),
      },
    });
    const first = screen.getAllByRole('treeitem')[0];
    if (first === undefined) throw new Error('no tree item');
    await fireEvent.keyDown(first, { key: 'ArrowDown', altKey: true });
    await waitFor(() => {
      expect(screen.getByRole('alert').textContent).toContain('옮기지 못했습니다');
    });
    expect(onmove).toHaveBeenCalledTimes(1);
    expect(screen.getAllByRole('treeitem').map((e) => e.textContent.includes('첫 문서'))).toEqual([
      true,
      false,
    ]);
  });
});

// 부르는 화면(개인 노트)까지: 옮기기 실패 뒤 다시 읽은 목록이 비어도 실패 알림이 남고, 다시 읽기마저 실패하면 "다시
// 불러왔다"고 하지 않으며, 옮기기는 됐는데 다시 읽기만 실패하면 "옮기지 못했다"고 하지 않는다.
describe('document tree move failure on a screen', () => {
  const two = (): SummaryView[] => [summary('a', '첫 문서'), summary('b', '둘째 문서')];
  async function moveSecondUp(): Promise<void> {
    const rows = await screen.findAllByRole('treeitem');
    const second = rows[1];
    if (second === undefined) throw new Error('no second tree item');
    await fireEvent.keyDown(second, { key: 'ArrowUp', altKey: true });
  }

  it('keeps the failure when the reloaded list is empty', async () => {
    await setLocale('ko', { reload: false });
    vi.spyOn(api, 'documentTemplates').mockResolvedValue([]);
    vi.spyOn(api, 'myNotes').mockResolvedValueOnce(two()).mockResolvedValue([]);
    vi.spyOn(api, 'setParent').mockRejectedValue(new ApiError(404, 'not_found', 'gone'));
    render(NotesScreen);
    await moveSecondUp();
    await waitFor(() => {
      expect(screen.getByRole('alert').textContent).toContain('옮기지 못했습니다');
    });
    expect(screen.getByText('노트가 없습니다.')).toBeInTheDocument();
  });

  it('does not say the list was reloaded when reading it failed too', async () => {
    await setLocale('ko', { reload: false });
    vi.spyOn(api, 'documentTemplates').mockResolvedValue([]);
    vi.spyOn(api, 'myNotes')
      .mockResolvedValueOnce(two())
      .mockRejectedValue(new ApiError(500, 'internal', '서버 오류'));
    vi.spyOn(api, 'setParent').mockRejectedValue(new ApiError(409, 'invalid_state', 'moved'));
    render(NotesScreen);
    await moveSecondUp();
    const alert = await screen.findByRole('alert');
    expect(alert.textContent).toContain('옮기지 못했');
    expect(alert.textContent).not.toContain('다시 불러왔습니다');
  });

  it('does not say the move failed when only the reload failed', async () => {
    await setLocale('ko', { reload: false });
    vi.spyOn(api, 'documentTemplates').mockResolvedValue([]);
    vi.spyOn(api, 'myNotes')
      .mockResolvedValueOnce(two())
      .mockRejectedValue(new ApiError(500, 'internal', '서버 오류'));
    vi.spyOn(api, 'setParent').mockResolvedValue(movedNote());
    render(NotesScreen);
    await moveSecondUp();
    const alert = await screen.findByRole('alert');
    expect(alert.textContent).toContain('옮겼지만');
    expect(alert.textContent).not.toContain('옮기지 못했');
  });
});

function movedNote(): DocumentView {
  return {
    id: 'b',
    workspace_id: null,
    placement: 'personal',
    item_id: null,
    parent_id: null,
    reference: null,
    title: '둘째 문서',
    body: { type: 'doc', content: [] },
    schema_version: 1,
    revision: 1,
    updated_at: '2026-10-19T00:00:00Z',
    access: 'owner',
    can_edit: true,
    visibility: 'private',
    allow_copy: false,
    can_manage: true,
    can_delete: true,
    is_template: false,
    can_export: true,
  };
}
