import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { DocumentView } from '$lib/api/client';
import { api } from '$lib/api/client';
import * as exporter from '$lib/features/documents/export';
import ExportDialog from '$lib/features/documents/ExportDialog.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

function documentView(canExport: boolean): DocumentView {
  return {
    id: 'doc-creator',
    workspace_id: 'ws-1',
    placement: 'wiki',
    item_id: null,
    parent_id: null,
    reference: 'WIKI-1',
    title: '작성자의 문서',
    body: { type: 'doc', content: [{ type: 'paragraph' }] },
    schema_version: 1,
    revision: 3,
    updated_at: '2026-10-28T00:00:00Z',
    access: 'write',
    can_edit: true,
    visibility: 'workspace',
    allow_copy: false,
    can_manage: false,
    can_delete: true,
    is_template: false,
    realtime: false,
    // D-115: 서버는 owner가 아닌 작성자에게도 내보내기를 허용한다.
    can_export: canExport,
  };
}

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  vi.spyOn(api, 'revisions').mockResolvedValue([]);
  vi.spyOn(exporter, 'saveBlob').mockImplementation(() => undefined);
});

afterEach(() => {
  vi.restoreAllMocks();
});

async function clickExport(doc: DocumentView): Promise<void> {
  render(ExportDialog, {
    props: {
      doc,
      unsaved: false,
      open: true,
      references: {
        workspaceId: doc.workspace_id,
        describeWorkspace: () => undefined,
        hrefFor: () => null,
        navigate: () => undefined,
      },
    },
  });
  const dialog = await screen.findByRole('dialog');
  await fireEvent.click(within(dialog).getByRole('button', { name: '내보내기', exact: true }));
}

describe('export dialog server permission (D-115, SR-200-1)', () => {
  it('exports for a non-owner creator with copying disabled when the server permits it', async () => {
    const doc = documentView(true);
    const exportRevision = vi.spyOn(api, 'exportRevision').mockResolvedValue({
      revision: doc.revision,
      title: doc.title,
      author_name: '작성자',
      created_at: doc.updated_at,
      restored_from: null,
      edited_by_names: null,
      body: doc.body,
      schema_version: doc.schema_version,
    });

    await clickExport(doc);

    await waitFor(() => expect(exportRevision).toHaveBeenCalledWith(doc.id, 3, 'markdown'));
    await waitFor(() => expect(exporter.saveBlob).toHaveBeenCalledOnce());
  });

  it('sends no export request when the server denies it even if the dialog is open', async () => {
    const exportRevision = vi.spyOn(api, 'exportRevision');

    await clickExport(documentView(false));

    expect(exportRevision).not.toHaveBeenCalled();
    expect(exporter.saveBlob).not.toHaveBeenCalled();
  });
});
