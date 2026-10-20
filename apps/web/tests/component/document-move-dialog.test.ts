import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { SummaryView } from '$lib/api/client';
import { ApiError } from '$lib/api/client';
import DocumentTree from '$lib/features/documents/DocumentTree.svelte';
import type { TreeMove } from '$lib/features/documents/tree';
import { parentCandidates, selfAndDescendants, TreeMoveError } from '$lib/features/documents/tree';
import { setLocale } from '$lib/paraglide/runtime.js';

function doc(id: string, title: string, parent: string | null = null): SummaryView {
  return {
    id,
    title,
    parent_id: parent,
    reference: null,
    revision: 1,
    updated_at: '2026-10-21T00:00:00Z',
  };
}

// 운영 안내 › 설치 › 백업, 점검 기록(최상위).
const docs = [
  doc('guide', '운영 안내'),
  doc('install', '설치', 'guide'),
  doc('backup', '백업', 'install'),
  doc('log', '점검 기록'),
];

describe('parent candidates', () => {
  it('leave out the document and everything under it, keep list order and give each path', () => {
    expect([...selfAndDescendants(docs, 'install')].sort()).toEqual(['backup', 'install']);
    const all = parentCandidates(docs, 'install', '');
    expect(all.map((c) => c.doc.id)).toEqual(['guide', 'log']);
    const deep = parentCandidates(docs, 'log', '백');
    expect(deep.map((c) => [c.doc.id, c.path.join(' › ')])).toEqual([
      ['backup', '운영 안내 › 설치'],
    ]);
  });
});

describe('move under another document (search)', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
  });
  afterEach(() => {
    vi.restoreAllMocks();
  });

  function mount(onmove: (id: string, move: TreeMove) => Promise<void>): void {
    render(DocumentTree, {
      props: {
        docs,
        label: '위키',
        empty: '문서가 없습니다.',
        hrefFor: (id: string) => `/d/${id}`,
        navigate: () => undefined,
        canWrite: true,
        onmove,
        oncreatechild: () => Promise.resolve(),
      },
    });
  }
  const row = (title: string): HTMLElement => {
    const found = screen
      .getAllByRole('treeitem')
      .find((e) => e.getAttribute('aria-label') === title);
    if (found === undefined) throw new Error(title);
    return found;
  };

  it('opens with Alt+M, finds by title, leaves out itself and its subdocuments, and moves with Enter', async () => {
    const onmove = vi.fn<(id: string, move: TreeMove) => Promise<void>>(() => Promise.resolve());
    mount(onmove);
    await fireEvent.keyDown(row('설치'), { key: 'm', altKey: true });
    const dialog = await screen.findByRole('dialog');
    const options = within(dialog).getByRole('listbox');
    // 맨 위, 지금 상위(운영 안내, 고를 수 없음), 점검 기록. 설치·백업은 없다.
    expect(
      within(options)
        .getAllByRole('option')
        .map((o) => o.textContent.trim()),
    ).toEqual(['맨 위(상위 문서 없음)', '운영 안내 (지금 상위 문서)', '점검 기록']);
    const input = within(dialog).getByRole('combobox');
    await fireEvent.input(input, { target: { value: '점검' } });
    await fireEvent.keyDown(input, { key: 'Enter' });
    await waitFor(() => {
      expect(onmove).toHaveBeenCalledWith('install', { parentId: 'log', beforeId: null });
    });
    await waitFor(() => {
      expect(screen.queryByRole('dialog')).toBeNull();
    });
    expect(document.activeElement).toBe(row('설치'));
  });

  it('can move to the top level and does not offer the current parent', async () => {
    const onmove = vi.fn<(id: string, move: TreeMove) => Promise<void>>(() => Promise.resolve());
    mount(onmove);
    await fireEvent.keyDown(row('설치'), { key: 'm', altKey: true });
    const dialog = await screen.findByRole('dialog');
    const input = within(dialog).getByRole('combobox');
    // 처음 고른 것은 맨 위. 아래로 한 번(지금 상위)은 보내지 않는다.
    await fireEvent.keyDown(input, { key: 'ArrowDown' });
    await fireEvent.keyDown(input, { key: 'Enter' });
    expect(onmove).not.toHaveBeenCalled();
    await fireEvent.keyDown(input, { key: 'ArrowUp' });
    await fireEvent.keyDown(input, { key: 'Enter' });
    await waitFor(() => {
      expect(onmove).toHaveBeenCalledWith('install', { parentId: null, beforeId: null });
    });
  });

  it('keeps the dialog open with the reason when the server refuses (e.g. too deep)', async () => {
    const onmove = vi.fn<(id: string, move: TreeMove) => Promise<void>>(() =>
      Promise.reject(
        new TreeMoveError(
          new ApiError(400, 'invalid', '트리가 10단계보다 깊어집니다.'),
          false,
          true,
        ),
      ),
    );
    mount(onmove);
    await fireEvent.keyDown(row('점검 기록'), { key: 'm', altKey: true });
    const dialog = await screen.findByRole('dialog');
    const input = within(dialog).getByRole('combobox');
    await fireEvent.input(input, { target: { value: '백업' } });
    await fireEvent.keyDown(input, { key: 'Enter' });
    const alert = await within(dialog).findByRole('alert');
    expect(alert.textContent).toContain('문서를 옮기지 못했습니다');
    expect(screen.getByRole('dialog')).toBeTruthy();
  });

  it('says when nothing matches', async () => {
    mount(() => Promise.resolve());
    await fireEvent.keyDown(row('점검 기록'), { key: 'm', altKey: true });
    const dialog = await screen.findByRole('dialog');
    await fireEvent.input(within(dialog).getByRole('combobox'), { target: { value: '없는 제목' } });
    expect(within(dialog).getByText('맞는 문서가 없습니다.')).toBeTruthy();
  });
});
