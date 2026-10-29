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
  toZip: vi.fn(() => Promise.resolve(new Blob(['attachment']))),
}));
vi.mock('$lib/features/documents/export-print', () => ({
  printDocumentHtml: vi.fn(() => Promise.resolve('<p>이전 사용자 본문</p>')),
  printToPdf: vi.fn(() => Promise.resolve()),
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
  await fireEvent.click(screen.getByRole('button', { name: '내보내기' }));
  return view;
}

it.each(['revision', 'references', 'file'])(
  'cancels a held %s preparation request on close and can export again before that response arrives',
  async (stage) => {
    vi.spyOn(api, 'exportRevision').mockRestore();
    vi.spyOn(api, 'file').mockRestore();
    const body =
      stage === 'references'
        ? {
            type: 'doc',
            content: [
              {
                type: 'paragraph',
                content: [
                  {
                    type: 'reference',
                    attrs: { kind: 'item', id: '0198f1f2-0000-7000-8000-000000000001' },
                  },
                ],
              },
            ],
          }
        : doc.body;
    const revision = {
      revision: 1,
      title: doc.title,
      body,
      schema_version: 1,
      created_at: doc.updated_at,
      author_name: 'A',
      restored_from: null,
      edited_by_names: null,
    };
    const target =
      stage === 'revision'
        ? '/api/documents/doc/revisions/1/export'
        : stage === 'references'
          ? '/api/references/resolve'
          : '/api/files/0198f1f2-0000-7000-8000-0000000000aa';
    let signal: AbortSignal | null | undefined;
    let release: (() => void) | undefined;
    let held = false;
    vi.spyOn(globalThis, 'fetch').mockImplementation((input, init) => {
      const path =
        typeof input === 'string' ? input : input instanceof URL ? input.href : input.url;
      const response = () =>
        path.endsWith('/export')
          ? new Response(JSON.stringify(revision))
          : path === '/api/references/resolve'
            ? new Response('[]')
            : path.includes('/content')
              ? new Response('attachment')
              : new Response('{"error":"not_found"}', { status: 404 });
      if (path !== target || held) return Promise.resolve(response());
      held = true;
      signal = init?.signal;
      return new Promise<Response>((resolve, reject) => {
        release = () => {
          resolve(response());
        };
        signal?.addEventListener(
          'abort',
          () => {
            reject(new DOMException('Aborted', 'AbortError'));
          },
          { once: true },
        );
      });
    });
    const view = await start('zip');
    try {
      await waitFor(() => {
        expect(release).toBeDefined();
      });
      await view.rerender({ ...props, open: false });
      expect.soft(signal?.aborted).toBe(true);
      await view.rerender({ ...props, open: true });
      const retry = await screen.findByRole('button', { name: /^내보내기$/ });
      expect(retry).toBeEnabled();
      await fireEvent.click(retry);
      await waitFor(() => {
        expect(exporting.saveBlob).toHaveBeenCalledOnce();
      });
      expect(printToPdf).not.toHaveBeenCalled();
    } finally {
      release?.();
    }
  },
);

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
        signal?.addEventListener('abort', () => {
          reject(new DOMException('Aborted', 'AbortError'));
        });
      });
    });
    let preparing: Promise<exporting.PreparedExport> | undefined;
    const prepare = exporting.prepareExport;
    vi.spyOn(exporting, 'prepareExport').mockImplementation((...args) => {
      preparing = prepare(...args);
      return preparing;
    });
    const view = await start(format);
    await waitFor(() => {
      expect(fetch).toHaveBeenCalledOnce();
    });
    if (change === 'unmount') view.unmount();
    else if (change === 'close') {
      const close = screen.getAllByRole('button', { name: '닫기' })[0];
      if (close === undefined) throw new Error('close button missing');
      await fireEvent.click(close);
    } else hideSession(change === 'logout' ? 'logout' : 'restore');
    flushSync();
    expect.soft(signal?.aborted).toBe(true);
    finish(new Response('old bytes'));
    await act(async () => {
      await preparing?.catch(() => undefined);
    });
    expect(exporting.saveBlob).not.toHaveBeenCalled();
    expect(printToPdf).not.toHaveBeenCalled();
  },
);

it.each([
  ['zip', 'user'],
  ['pdf', 'user'],
  ['zip', 'generation'],
  ['pdf', 'generation'],
] as const)(
  'rechecks context immediately before the final %s side effect after %s changes',
  async (format, change) => {
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
    await waitFor(() => {
      expect(format === 'zip' ? exporting.toZip : printDocumentHtml).toHaveBeenCalled();
    });
    if (change === 'user') me.userId = 'user-b';
    else hideSession('restore');
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
  await waitFor(() => {
    expect(exporting.saveBlob).toHaveBeenCalledOnce();
  });
  expect(printToPdf).not.toHaveBeenCalled();
});

it('prints an attachment export for the unchanged user', async () => {
  vi.spyOn(globalThis, 'fetch').mockResolvedValue(new Response('current bytes'));
  await start('pdf');
  await waitFor(() => {
    expect(printToPdf).toHaveBeenCalledOnce();
  });
  expect(exporting.saveBlob).not.toHaveBeenCalled();
});
