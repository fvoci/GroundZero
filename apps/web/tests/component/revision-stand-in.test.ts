import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { DocumentView, RevisionSummaryView } from '$lib/api/client';
import { api, parseRevisionSummary } from '$lib/api/client';
import ExportDialog from '$lib/features/documents/ExportDialog.svelte';
import RevisionHistory from '$lib/features/documents/RevisionHistory.svelte';
import { standInLabel } from '$lib/features/documents/revision-author';
import { setLocale } from '$lib/paraglide/runtime.js';

// D-95: 실시간 편집에서 고친 사람 대신 다른 사람(옮긴 사람·실시간을 끈 관리자 등)이 저장한 리비전은 실제 편집자와 대신
// 저장한 사람을 함께 보인다. 대신 저장한 사람이 내용을 쓴 것처럼 보이지 않게 한다.

afterEach(() => {
  vi.restoreAllMocks();
});

function summary(
  revision: number,
  author: string | null,
  editedBy: (string | null)[] | null,
): RevisionSummaryView {
  return {
    revision,
    title: '문서',
    author_name: author,
    created_at: '2026-10-22T00:00:00Z',
    restored_from: null,
    edited_by_names: editedBy,
  };
}

const LIST = [
  summary(3, '관리자', ['작성자', null]),
  summary(2, '관리자', []),
  summary(1, '작성자', null),
];

function documentAt(revision: number): DocumentView {
  return {
    id: 'd1',
    workspace_id: 'ws-1',
    placement: 'wiki',
    item_id: null,
    parent_id: null,
    reference: 'WIKI-1',
    title: '문서',
    body: { type: 'doc' },
    schema_version: 1,
    revision,
    updated_at: '2026-10-22T00:00:00Z',
    access: 'owner',
    can_edit: true,
    visibility: 'workspace',
    allow_copy: false,
    can_manage: true,
    can_delete: true,
    is_template: false,
    can_export: true,
  };
}

describe('stand-in revisions (D-95)', () => {
  it('parses the actual editors, keeping unknown names and the ordinary case apart', () => {
    const base = {
      revision: 2,
      title: 't',
      author_name: 'x',
      created_at: '2026-10-22T00:00:00Z',
      restored_from: null,
    };
    expect(parseRevisionSummary(base).edited_by_names).toBeNull();
    expect(parseRevisionSummary({ ...base, edited_by_names: null }).edited_by_names).toBeNull();
    expect(parseRevisionSummary({ ...base, edited_by_names: [] }).edited_by_names).toEqual([]);
    expect(parseRevisionSummary({ ...base, edited_by_names: ['a', null] }).edited_by_names).toEqual(
      ['a', null],
    );
    expect(() => parseRevisionSummary({ ...base, edited_by_names: [1] })).toThrow(TypeError);
    expect(() => parseRevisionSummary({ ...base, edited_by_names: 'a' })).toThrow(TypeError);
  });

  it('names the editors and the person who saved for them', async () => {
    await setLocale('ko', { reload: false });
    expect(standInLabel(summary(1, '작성자', null))).toBeUndefined();
    expect(standInLabel(summary(3, '관리자', ['작성자', null]))).toBe(
      '편집: 작성자, 알 수 없음 · 대신 저장: 관리자',
    );
    expect(standInLabel(summary(2, '관리자', []))).toBe(
      '편집한 사람 기록 없음 · 대신 저장: 관리자',
    );
    expect(standInLabel(summary(2, null, ['작성자']))).toBe('편집: 작성자 · 대신 저장: 알 수 없음');
    await setLocale('en', { reload: false });
    expect(standInLabel(summary(3, 'Admin', ['Writer', 'Reviewer']))).toBe(
      'Edited by Writer, Reviewer · saved for them by Admin',
    );
    expect(standInLabel(summary(2, 'Admin', []))).toBe(
      'Editors not recorded · saved for them by Admin',
    );
    await setLocale('ko', { reload: false });
  });

  it('shows the stand-in in the revision history list', async () => {
    await setLocale('ko', { reload: false });
    vi.spyOn(api, 'revisions').mockResolvedValue(LIST);
    render(RevisionHistory, { props: { doc: documentAt(3), onrestored: () => undefined } });
    await fireEvent.click(screen.getByRole('button', { name: '기록' }));
    const options = await screen.findAllByRole('option');
    expect(options.map((o) => o.textContent)).toEqual([
      expect.stringContaining('편집: 작성자, 알 수 없음 · 대신 저장: 관리자'),
      expect.stringContaining('편집한 사람 기록 없음 · 대신 저장: 관리자'),
      expect.stringContaining('작성자'),
    ]);
    expect(options[2]?.textContent).not.toContain('대신 저장');
  });

  it('shows the stand-in in the export revision picker', async () => {
    await setLocale('ko', { reload: false });
    vi.spyOn(api, 'revisions').mockResolvedValue(LIST);
    render(ExportDialog, {
      props: {
        doc: documentAt(3),
        unsaved: false,
        // 여는 단추는 문서 화면의 "더 보기" 메뉴에 있다.
        open: true,
        references: {
          workspaceId: 'ws-1',
          describeWorkspace: () => undefined,
          hrefFor: () => null,
          navigate: () => undefined,
        },
      },
    });
    await waitFor(() => {
      expect(screen.getAllByRole('option')).toHaveLength(3);
    });
    const texts = screen.getAllByRole('option').map((o) => o.textContent);
    expect(texts[0]).toContain('편집: 작성자, 알 수 없음 · 대신 저장: 관리자');
    expect(texts[1]).toContain('편집한 사람 기록 없음 · 대신 저장: 관리자');
    expect(texts[2]).toContain(' · 작성자');
    expect(texts[2]).not.toContain('대신 저장');
  });
});
