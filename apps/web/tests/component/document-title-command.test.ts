import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { WebsocketProvider } from 'y-websocket';
import * as Y from 'yjs';

import type { DocumentView } from '$lib/api/client';
import { loadDraft } from '$lib/features/documents/drafts';
import type { LiveSession } from '$lib/features/documents/live';
import LiveDocumentEditor from '$lib/features/documents/LiveDocumentEditor.svelte';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';

vi.mock('$lib/features/documents/TiptapEditor.svelte', async () => ({
  default: (await import('./stubs/SilentEditorStub.svelte')).default,
}));
vi.mock('$lib/features/documents/live', async (original) => ({
  ...(await original<typeof import('$lib/features/documents/live')>()),
  openLive: (_id: string, user: LiveSession['user']): LiveSession => {
    const document = new Y.Doc();
    return {
      document,
      provider: new WebsocketProvider('ws://localhost', 'collab', document, { connect: false }),
      user,
      persisted: { on: () => undefined, off: () => undefined },
    };
  },
}));

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
  access: 'write',
  can_edit: true,
  visibility: 'workspace',
  allow_copy: false,
  can_manage: false,
  can_delete: false,
  is_template: false,
  realtime: true,
  can_export: true,
};
beforeEach(async () => {
  await setLocale('ko', { reload: false });
  me.userId = 'user-a';
});
afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  window.localStorage.clear();
  me.userId = '';
});

describe('title command and screen-only protection (D-27, #178)', () => {
  it('reuses the title request id after a lost response and sends only one request while saving', async () => {
    let finish: ((response: Response) => void) | undefined;
    const fetch = vi
      .fn<(_url: string, init?: RequestInit) => Promise<Response>>()
      .mockRejectedValueOnce(new TypeError('connection lost'))
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            finish = resolve;
          }),
      );
    vi.stubGlobal('fetch', fetch);
    const saved = vi.fn();
    render(LiveDocumentEditor, { props: { initial: doc, onsaved: saved } });
    const title = screen.getByLabelText(m.documents_title_label());
    const button = screen.getByRole('button', { name: m.documents_title_save() });
    const form = button.closest('form');
    if (form === null) throw new Error('missing title form');
    await fireEvent.input(title, { target: { value: '내 제목' } });
    await fireEvent.click(button);
    await screen.findByRole('alert');
    await fireEvent.click(button);
    await fireEvent.blur(title);
    await fireEvent.submit(form);
    expect(fetch).toHaveBeenCalledTimes(2);
    expect(fetch.mock.calls[1]?.[1]?.body).toBe(fetch.mock.calls[0]?.[1]?.body);
    finish?.(
      new Response(JSON.stringify({ ...doc, title: '내 제목', revision: 4 }), { status: 200 }),
    );
    await waitFor(() => expect(saved).toHaveBeenCalledOnce());
  });

  it('sends one title PATCH, shows the stale title and revision, and retries only on explicit save', async () => {
    const current = { ...doc, title: '다른 사람 제목', revision: 4 };
    let writes = 0;
    const fetch = vi.fn((_url: string, init?: RequestInit) => {
      if (init?.method !== 'GET') writes += 1;
      return Promise.resolve(
        new Response(
          JSON.stringify(
            init?.method === 'GET'
              ? doc
              : writes === 1
                ? {
                    error: 'conflict',
                    message: 'stale',
                    current,
                    changed: ['title'],
                    request_id: 'server-request',
                  }
                : { ...current, title: '내 제목', revision: 5 },
          ),
          { status: init?.method !== 'GET' && writes === 1 ? 409 : 200 },
        ),
      );
    });
    vi.stubGlobal('fetch', fetch);
    const saved = vi.fn();
    render(LiveDocumentEditor, { props: { initial: doc, onsaved: saved } });
    await fireEvent.input(screen.getByLabelText(m.documents_title_label()), {
      target: { value: '내 제목' },
    });
    await fireEvent.click(screen.getByRole('button', { name: m.documents_title_save() }));
    const conflict = await screen.findByRole('region', { name: m.documents_conflict_heading() });
    expect(conflict.textContent).toContain('다른 사람 제목');
    expect(conflict.textContent).toContain('4');
    expect(fetch).toHaveBeenCalledTimes(1);
    const first = fetch.mock.calls[0];
    expect(first?.[0]).toBe('/api/documents/d1');
    expect(first?.[1]?.method).toBe('PATCH');
    expect(JSON.parse(String(first?.[1]?.body))).toEqual({
      request_id: expect.any(String),
      base_revision: 3,
      title: '내 제목',
    });
    expect(new Headers(first?.[1]?.headers).get('x-fvoci-expected-user')).toBe('user-a');
    expect(saved).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByRole('button', { name: m.documents_title_save() }));
    await waitFor(() => expect(saved).toHaveBeenCalledOnce());
    expect(fetch).toHaveBeenCalledTimes(2);
    const second = JSON.parse(String(fetch.mock.calls[1]?.[1]?.body)) as {
      base_revision: number;
      request_id: string;
    };
    expect(second.base_revision).toBe(4);
    expect(second.request_id).not.toBe(
      (JSON.parse(String(first?.[1]?.body)) as { request_id: string }).request_id,
    );
  });

  it('warns on leaving a title-only edit and offers no permanent draft after remount', async () => {
    const view = render(LiveDocumentEditor, { props: { initial: doc } });
    await fireEvent.input(screen.getByLabelText(m.documents_title_label()), {
      target: { value: '화면에만 둔 제목' },
    });
    const unload = new Event('beforeunload', { cancelable: true });
    window.dispatchEvent(unload);
    expect(unload.defaultPrevented).toBe(true);
    expect(loadDraft('d1')).toBeUndefined();
    view.unmount();
    render(LiveDocumentEditor, { props: { initial: doc } });
    expect(screen.getByLabelText(m.documents_title_label())).toHaveValue('문서');
    expect(screen.queryByRole('region', { name: m.documents_draft_region() })).toBeNull();
  });
});
