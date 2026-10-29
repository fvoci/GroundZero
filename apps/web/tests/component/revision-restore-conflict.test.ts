import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { DocumentView, RevisionView } from '$lib/api/client';
import { api } from '$lib/api/client';
import RevisionHistory from '$lib/features/documents/RevisionHistory.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

afterEach(() => {
  vi.restoreAllMocks();
});

function paragraph(text: string): DocumentView['body'] {
  return { type: 'doc', content: [{ type: 'paragraph', content: [{ type: 'text', text }] }] };
}

function documentAt(revision: number, text: string): DocumentView {
  return {
    id: 'd1',
    workspace_id: 'ws-1',
    placement: 'wiki',
    item_id: null,
    parent_id: null,
    reference: 'WIKI-1',
    title: '문서',
    body: paragraph(text),
    schema_version: 1,
    revision,
    updated_at: '2026-10-19T00:00:00Z',
    access: 'owner',
    can_edit: true,
    visibility: 'workspace',
    allow_copy: false,
    can_manage: true,
    can_delete: true,
    is_template: false,
    realtime: true,
    can_export: true,
  };
}

function revisionAt(revision: number, text: string): RevisionView {
  return {
    revision,
    title: '문서',
    author_name: '나',
    created_at: '2026-10-19T00:00:00Z',
    restored_from: null,
    edited_by_names: null,
    body: paragraph(text),
    schema_version: 1,
  };
}

// 남은 항목 ①: 복원이 충돌하면(그사이 실시간 편집이 리비전으로 남음 등) 저절로 다시 보내지 않는다. 지금 내용(서버의 최신
// 리비전)을 비교에 보여 주고, 사람이 다시 누를 때만 그 최신 리비전을 기준으로 다시 보낸다.
describe('restore conflict', () => {
  it('shows the latest content and retries only when asked, on the latest base', async () => {
    await setLocale('ko', { reload: false });
    vi.spyOn(api, 'revisions').mockResolvedValue([
      revisionAt(3, '지금 화면'),
      revisionAt(1, '옛 내용'),
    ]);
    vi.spyOn(api, 'revision').mockImplementation((_id: string, n: number) =>
      Promise.resolve(n === 1 ? revisionAt(1, '옛 내용') : revisionAt(n, '지금 화면')),
    );
    const restore = vi
      .spyOn(documentsApi, 'restoreRevision')
      .mockResolvedValueOnce({
        kind: 'conflict',
        current: documentAt(4, '다른 사람의 실시간 문장'),
        changed: ['body'],
        requestId: 'restore-conflict',
      })
      .mockResolvedValueOnce({ kind: 'restored', document: documentAt(5, '옛 내용') });
    const restored = vi.fn();
    render(RevisionHistory, { props: { doc: documentAt(3, '지금 화면'), onrestored: restored } });

    await fireEvent.click(screen.getByRole('button', { name: '기록' }));
    await fireEvent.click(await screen.findByRole('option', { name: /r1/ }));
    const button = await screen.findByRole('button', { name: 'r1(으)로 복원' });
    await fireEvent.click(button);
    await waitFor(() => {
      expect(screen.getByRole('alert').textContent).toContain('4');
    });
    // 저절로 다시 보내지 않는다. 비교는 서버의 최신 내용 기준이다.
    await new Promise((r) => setTimeout(r, 20));
    expect(restore).toHaveBeenCalledTimes(1);
    expect(restore.mock.calls[0]?.[2].baseRevision).toBe(3);
    expect(screen.getByTestId('revision-diff').textContent).toContain('다른 사람의 실시간 문장');

    await fireEvent.click(screen.getByRole('button', { name: 'r1(으)로 복원' }));
    await waitFor(() => {
      expect(restored).toHaveBeenCalled();
    });
    expect(restore).toHaveBeenCalledTimes(2);
    expect(restore.mock.calls[1]?.[2].baseRevision).toBe(4);
    expect(restore.mock.calls[1]?.[2].requestId).not.toBe(restore.mock.calls[0]?.[2].requestId);
  });
});

// 검토(남은 항목 ①): 충돌 뒤 화면의 문서가 더 새 리비전으로 바뀌면(저장 등) 옛 충돌 응답 대신 그 문서를 기준으로 한다.
describe('restore conflict, then the page moves on', () => {
  it('uses the newer page document rather than the older conflict result', async () => {
    await setLocale('ko', { reload: false });
    vi.spyOn(api, 'revisions').mockResolvedValue([revisionAt(1, '옛 내용')]);
    vi.spyOn(api, 'revision').mockResolvedValue(revisionAt(1, '옛 내용'));
    const restore = vi
      .spyOn(documentsApi, 'restoreRevision')
      .mockResolvedValueOnce({
        kind: 'conflict',
        current: documentAt(4, '충돌 때 내용'),
        changed: ['body'],
        requestId: 'restore-conflict',
      })
      .mockResolvedValueOnce({ kind: 'restored', document: documentAt(6, '옛 내용') });
    const view = render(RevisionHistory, {
      props: { doc: documentAt(3, '지금 화면'), onrestored: () => undefined },
    });
    await fireEvent.click(screen.getByRole('button', { name: '기록' }));
    await fireEvent.click(await screen.findByRole('option', { name: /r1/ }));
    await fireEvent.click(await screen.findByRole('button', { name: 'r1(으)로 복원' }));
    await waitFor(() => {
      expect(restore).toHaveBeenCalledTimes(1);
    });
    await view.rerender({ doc: documentAt(5, '더 새 화면'), onrestored: () => undefined });
    await fireEvent.click(await screen.findByRole('button', { name: 'r1(으)로 복원' }));
    await waitFor(() => {
      expect(restore).toHaveBeenCalledTimes(2);
    });
    expect(restore.mock.calls[1]?.[2].baseRevision).toBe(5);
  });
});
import { documentsApi } from '$lib/features/documents/api';
