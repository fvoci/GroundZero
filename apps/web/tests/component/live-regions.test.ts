import { fireEvent, render, screen, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { DocumentView } from '$lib/api/client';
import { api } from '$lib/api/client';
import ExportDialog from '$lib/features/documents/ExportDialog.svelte';
import DeleteButton from '$lib/features/trash/DeleteButton.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

// 상태 알림(role="status")을 글과 함께 {#if}로 새로 넣으면 화면 낭독기가 알리지 못할 수 있다(UI 감사). 상태 영역은 처음부터
// 두고(비었을 때는 empty:hidden으로 감춘다) 그 안의 글만 바꾼다. 같은 요소가 먼저 있고 나중에 글을 받는지 확인한다.
beforeEach(async () => {
  await setLocale('ko', { reload: false });
});
afterEach(() => {
  vi.restoreAllMocks();
});

describe('status regions stay in place and only their text changes', () => {
  it('keeps the trash question region before asking', async () => {
    render(DeleteButton, {
      props: { what: '문서', remove: () => Promise.resolve(), ondeleted: () => undefined },
    });
    const region = screen.getByRole('status');
    expect(region).toBeEmptyDOMElement();
    expect(region).toHaveClass('empty:hidden');

    await fireEvent.click(screen.getByRole('button', { name: '문서 지우기' }));
    expect(screen.getByRole('status')).toBe(region);
    expect(region).toHaveTextContent('문서: 휴지통으로 옮길까요?');
  });

  it('keeps the export notice region while the unsaved notice comes and goes', async () => {
    vi.spyOn(api, 'revisions').mockResolvedValue([]);
    const doc: DocumentView = {
      id: 'd1',
      workspace_id: 'ws-1',
      placement: 'wiki',
      item_id: null,
      parent_id: null,
      reference: 'WIKI-1',
      title: '문서',
      body: { type: 'doc' },
      schema_version: 1,
      revision: 3,
      updated_at: '2026-10-19T00:00:00Z',
      access: 'owner',
      can_edit: true,
      visibility: 'workspace',
      allow_copy: true,
      can_manage: true,
      can_delete: true,
      is_template: false,
      realtime: false,
      can_export: true,
    };
    const references = {
      workspaceId: 'ws-1',
      describeWorkspace: () => undefined,
      hrefFor: () => null,
      navigate: () => undefined,
    };
    const view = render(ExportDialog, { props: { doc, unsaved: false, references, open: true } });
    const dialog = await screen.findByRole('dialog');
    const before = within(dialog).getAllByRole('status');
    expect(before.length).toBeGreaterThan(0);
    for (const region of before) expect(region).toBeEmptyDOMElement();

    await view.rerender({ doc, unsaved: true, references, open: true });
    const after = within(dialog).getAllByRole('status');
    expect(after).toEqual(before);
    expect(after.some((r) => r.textContent.includes('리비전 3'))).toBe(true);
  });
});
