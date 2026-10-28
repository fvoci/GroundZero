import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { flushSync } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import type { DocumentView } from '$lib/api/client';
import { api } from '$lib/api/client';
import ExportDialog from '$lib/features/documents/ExportDialog.svelte';
import * as exporting from '$lib/features/documents/export';
import { printDocumentHtml, printToPdf } from '$lib/features/documents/export-print';
import { accountSafety, hideSession } from '$lib/features/feedback/feedback.svelte';
import { me } from '$lib/me.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

vi.mock('$lib/features/documents/export', async (original) => ({
  ...(await original<typeof exporting>()),
  saveBlob: vi.fn(),
  toMarkdown: vi.fn(() => ({ markdown: '이전 사용자 본문', lossy: [] })),
  toZip: vi.fn(async () => new Blob(['attachment'])),
}));
vi.mock('$lib/features/documents/export-print', () => ({
  printDocumentHtml: vi.fn(async () => '<p>이전 사용자 본문</p>'),
  printToPdf: vi.fn(async () => undefined),
}));

const doc: DocumentView = {
  id: 'doc',
  workspace_id: 'ws',
  placement: 'wiki',
  item_id: null,
  parent_id: null,
  reference: 'WIKI-1',
  title: '민감 문서',
  body: {
    type: 'doc',
    content: [
      {
        type: 'image',
        attrs: {
          src: '/api/files/0198f1f2-0000-7000-8000-0000000000aa/content',
        },
      },
    ],
  },
  schema_version: 1,
  revision: 1,
  updated_at: '2026-10-29T00:00:00Z',
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
const props = {
  doc,
  unsaved: false,
  open: true,
  references: {
    workspaceId: 'ws',
    describeWorkspace: () => undefined,
    hrefFor: () => null,
    navigate: () => undefined,
  },
};

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  me.userId = 'user-a';
  vi.spyOn(api, 'revisions').mockResolvedValue([]);
  vi.spyOn(api, 'exportRevision').mockResolvedValue({
    revision: 1,
    title: doc.title,
    body: doc.body,
    schema_version: 1,
    created_at: doc.updated_at,
    author_name: 'A',
    restored_from: null,
    edited_by_names: null,
  });
  vi.spyOn(api, 'file').mockRejectedValue(new Error('no name'));
});
afterEach(() => {
  cleanup();
  hideSession('account');
  accountSafety.switching = false;
  me.userId = '';
  vi.restoreAllMocks();
  vi.clearAllMocks();
});

async function start(format: 'zip' | 'pdf') {
  const view = render(ExportDialog, props);
  await screen.findByRole('dialog');
  await fireEvent.click(screen.getByRole('radio', { name: format === 'zip' ? /\.zip/ : /PDF/ }));
  await fireEvent.click(screen.getByRole('button', { name: '내보내기', exact: true }));
  return view;
}

// SR-221-2 / D-96: 공통 request 밖 첨부도 로그아웃·다른 탭 신호·닫기에서 중단한다.
it.each([
  ['logout', 'zip'],
  ['other-tab', 'zip'],
  ['close', 'zip'],
  ['unmount', 'zip'],
  ['logout', 'pdf'],
  ['other-tab', 'pdf'],
  ['close', 'pdf'],
  ['unmount', 'pdf'],
] as const)(
  'aborts a held attachment and never saves or prints after %s during %s',
  async (change, format) => {
    let finish!: (response: Response) => void;
    let signal: AbortSignal | null | undefined;
    vi.spyOn(globalThis, 'fetch').mockImplementation((_input, init) => {
      signal = init?.signal;
      return new Promise<Response>((resolve, reject) => {
        finish = resolve;
        signal?.addEventListener('abort', () => reject(new DOMException('Aborted', 'AbortError')));
      });
    });
    const prepare = vi.spyOn(exporting, 'prepareExport');
    const view = await start(format);
    await waitFor(() => expect(fetch).toHaveBeenCalledOnce());
    if (change === 'unmount') view.unmount();
    else if (change === 'close')
      await fireEvent.click(screen.getAllByRole('button', { name: '닫기' })[0]!);
    else hideSession(change === 'logout' ? 'logout' : 'restore');
    flushSync();
    expect.soft(signal?.aborted).toBe(true);
    finish(new Response('old bytes'));
    await act(async () => {
      await prepare.mock.results[0]?.value;
    });
    expect(exporting.saveBlob).not.toHaveBeenCalled();
    expect(printToPdf).not.toHaveBeenCalled();
  },
);

it.each(['zip', 'pdf'] as const)(
  'rechecks the user immediately before the final %s side effect',
  async (format) => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(new Response('old bytes'));
    let finish!: () => void;
    const held = new Promise<void>((resolve) => {
      finish = resolve;
    });
    if (format === 'zip')
      vi.mocked(exporting.toZip).mockImplementationOnce(async () => {
        await held;
        return new Blob(['old bytes']);
      });
    else
      vi.mocked(printDocumentHtml).mockImplementationOnce(async () => {
        await held;
        return '<p>old bytes</p>';
      });
    await start(format);
    await waitFor(() =>
      expect(format === 'zip' ? exporting.toZip : printDocumentHtml).toHaveBeenCalled(),
    );
    me.userId = 'user-b';
    await act(async () => {
      finish();
      await held;
    });
    expect(exporting.saveBlob).not.toHaveBeenCalled();
    expect(printToPdf).not.toHaveBeenCalled();
  },
);

it('saves an attachment export for the unchanged user', async () => {
  vi.spyOn(globalThis, 'fetch').mockResolvedValue(new Response('current bytes'));
  await start('zip');
  await waitFor(() => expect(exporting.saveBlob).toHaveBeenCalledOnce());
  expect(printToPdf).not.toHaveBeenCalled();
});
