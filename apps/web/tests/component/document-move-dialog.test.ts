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
    // 처음 고른 것은 맨 위. 아래로 한 번(지금 상위)은 보내지 않는다. 고른 곳은 단추 위에 글로 보인다.
    const target = (): string => within(dialog).getByTestId('move-doc-target').textContent.trim();
    expect(target()).toBe('옮길 곳: 맨 위(상위 문서 없음)');
    await fireEvent.keyDown(input, { key: 'ArrowDown' });
    expect(target()).toBe('이미 이 문서 아래에 있습니다. 다른 곳을 고르세요.');
    await fireEvent.keyDown(input, { key: 'Enter' });
    expect(onmove).not.toHaveBeenCalled();
    await fireEvent.keyDown(input, { key: 'ArrowDown' });
    expect(target()).toBe('옮길 곳: 점검 기록 아래');
    await fireEvent.keyDown(input, { key: 'ArrowUp' });
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
    input.focus();
    await fireEvent.input(input, { target: { value: '백업' } });
    await fireEvent.keyDown(input, { key: 'Enter' });
    const alert = await within(dialog).findByRole('alert');
    expect(alert.textContent).toContain('문서를 옮기지 못했습니다');
    expect(screen.getByRole('dialog')).toBeTruthy();
    // 거절된 뒤에도 검색 입력에 초점이 남아 다른 곳을 고를 수 있다.
    expect(document.activeElement).toBe(input);
  });

  it('keeps the search input usable (read-only, not disabled) while a move is being sent', async () => {
    let finish: () => void = () => undefined;
    const onmove = vi.fn<(id: string, move: TreeMove) => Promise<void>>(
      () =>
        new Promise<void>((resolve) => {
          finish = resolve;
        }),
    );
    mount(onmove);
    await fireEvent.keyDown(row('점검 기록'), { key: 'm', altKey: true });
    const dialog = await screen.findByRole('dialog');
    const input = within(dialog).getByRole('combobox');
    input.focus();
    await fireEvent.keyDown(input, { key: 'Enter' });
    // 비활성화하면 브라우저가 초점을 빼므로 읽기 전용과 aria-busy로 둔다.
    expect(input).toHaveProperty('disabled', false);
    expect(input).toHaveProperty('readOnly', true);
    expect(input.getAttribute('aria-busy')).toBe('true');
    finish();
  });

  it('does not close as if moved when another move is still running', async () => {
    const onmove = vi.fn<(id: string, move: TreeMove) => Promise<void>>(
      () => new Promise<void>(() => undefined),
    );
    mount(onmove);
    // 트리의 다른 옮기기(Alt+아래)가 아직 끝나지 않았다.
    await fireEvent.keyDown(row('운영 안내'), { key: 'ArrowDown', altKey: true });
    await fireEvent.keyDown(row('점검 기록'), { key: 'm', altKey: true });
    const dialog = await screen.findByRole('dialog');
    await fireEvent.keyDown(within(dialog).getByRole('combobox'), { key: 'Enter' });
    expect((await within(dialog).findByRole('alert')).textContent).toContain(
      '문서를 옮기지 못했습니다',
    );
    expect(screen.getByRole('dialog')).toBeTruthy();
    expect(onmove).toHaveBeenCalledTimes(1);
  });

  it('says when nothing matches', async () => {
    mount(() => Promise.resolve());
    await fireEvent.keyDown(row('점검 기록'), { key: 'm', altKey: true });
    const dialog = await screen.findByRole('dialog');
    await fireEvent.input(within(dialog).getByRole('combobox'), { target: { value: '없는 제목' } });
    expect(within(dialog).getByText('맞는 문서가 없습니다.')).toBeTruthy();
  });
});

describe('move dialog when only the reload fails', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
  });
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('closes (the move went through) and says the list could not be reloaded', async () => {
    const onmove = vi.fn<(id: string, move: TreeMove) => Promise<void>>(() =>
      Promise.reject(new TreeMoveError(new Error('offline'), true, false)),
    );
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
    const row = screen
      .getAllByRole('treeitem')
      .find((e) => e.getAttribute('aria-label') === '점검 기록');
    if (row === undefined) throw new Error('row');
    await fireEvent.keyDown(row, { key: 'm', altKey: true });
    const dialog = await screen.findByRole('dialog');
    await fireEvent.keyDown(within(dialog).getByRole('combobox'), { key: 'Enter' });
    await waitFor(() => {
      expect(screen.queryByRole('dialog')).toBeNull();
    });
    expect(screen.getByRole('alert').textContent).toContain(
      '옮겼지만 목록을 다시 불러오지 못했습니다',
    );
    expect(onmove).toHaveBeenCalledTimes(1);
  });
});
