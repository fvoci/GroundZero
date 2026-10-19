import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';

import type { SummaryView } from '$lib/api/client';
import { ApiError } from '$lib/api/client';
import DocumentTree from '$lib/features/documents/DocumentTree.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

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
