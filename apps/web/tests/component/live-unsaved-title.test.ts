import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { WebsocketProvider } from 'y-websocket';
import * as Y from 'yjs';

import type { DocumentView } from '$lib/api/client';
import { api } from '$lib/api/client';
import { documentsApi } from '$lib/features/documents/api';
import { loadDraft, loadOffers } from '$lib/features/documents/drafts';
import type { LiveSession } from '$lib/features/documents/live';
import LiveDocumentEditor from '$lib/features/documents/LiveDocumentEditor.svelte';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';

// D-27 (#154): 제목은 화면에서 보호하고, 본문만 영구 초안으로 남긴다.

const opened: LiveSession[] = [];

vi.mock('$lib/features/documents/TiptapEditor.svelte', async () => ({
  default: (await import('./stubs/EditorStub.svelte')).default,
}));

vi.mock('$lib/features/documents/live', async (original) => {
  const real = await original<typeof import('$lib/features/documents/live')>();
  return {
    ...real,
    openLive: (_id: string, user: LiveSession['user']): LiveSession => {
      const document = new Y.Doc();
      const session: LiveSession = {
        document,
        provider: new WebsocketProvider('ws://localhost', 'collab', document, { connect: false }),
        user,
        persisted: { on: () => undefined, off: () => undefined },
      };
      opened.push(session);
      return session;
    },
  };
});

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  me.userId = 'user-a';
});

afterEach(() => {
  vi.restoreAllMocks();
  opened.length = 0;
  window.localStorage.clear();
  me.userId = '';
});

const doc: DocumentView = {
  id: 'd1',
  workspace_id: 'ws-1',
  placement: 'wiki',
  item_id: null,
  parent_id: null,
  reference: 'WIKI-1',
  title: '문서',
  body: { type: 'doc', content: [{ type: 'paragraph' }] },
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

const EDITED = '저장하지 않은 제목';

/** 제목만 고친다(본문은 손대지 않아 서버와 맞춰져 있다). 칸을 떠나지 않아 제목 저장도 보내지 않는다. */
async function editTitleOnly(): Promise<void> {
  const title = document.getElementById('live-title');
  if (!(title instanceof HTMLInputElement)) throw new Error('live-title');
  await fireEvent.input(title, { target: { value: EDITED } });
}

function closeWith(code: number): void {
  const last = opened.at(-1);
  last?.provider.emit('connection-close', [new CloseEvent('close', { code }), last.provider]);
}

describe('a screen-only title when live editing ends (D-27)', () => {
  for (const pendingBody of [false, true]) {
    it(`clears the lost-title warning after saving only when no body is pending (${String(pendingBody)})`, async () => {
      vi.spyOn(api, 'document').mockResolvedValue(doc);
      vi.spyOn(documentsApi, 'patchTitle').mockResolvedValue({
        kind: 'saved',
        document: { ...doc, title: EDITED, revision: 4 },
      });
      const saved = vi.fn();
      render(LiveDocumentEditor, { props: { initial: doc, onsaved: saved } });
      await editTitleOnly();
      if (pendingBody) {
        await fireEvent.click(screen.getByRole('button', { name: 'stub-type' }));
        opened.at(-1)?.document.getText('t').insert(0, '본문');
        vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
          throw new DOMException('full', 'QuotaExceededError');
        });
      }
      closeWith(4410);
      await screen.findByText(m.documents_live_title_pending());
      expect(
        screen.getByRole('button', { name: m.documents_live_discard_reconnect() }),
      ).toBeTruthy();

      await fireEvent.click(screen.getByRole('button', { name: m.documents_title_save() }));
      await waitFor(() => {
        expect(saved).toHaveBeenCalledOnce();
      });
      if (pendingBody) {
        expect(screen.getByRole('alert')).toHaveTextContent(m.documents_live_unsaved_lost());
        expect(
          screen.getByRole('button', { name: m.documents_live_discard_reconnect() }),
        ).toBeTruthy();
      } else {
        expect(screen.queryByRole('alert')).toBeNull();
        expect(
          screen.queryByRole('button', { name: m.documents_live_discard_reconnect() }),
        ).toBeNull();
      }
      const unload = new Event('beforeunload', { cancelable: true });
      window.dispatchEvent(unload);
      expect(unload.defaultPrevented).toBe(pendingBody);
    });
  }

  for (const code of [4410, 4403, 4409]) {
    it(`keeps the title on screen and blocks reopening until it is saved or discarded (${String(code)})`, async () => {
      const reopen = vi.fn();
      render(LiveDocumentEditor, {
        props: { initial: doc, onrealtimeoff: reopen, onreload: reopen },
      });
      await editTitleOnly();
      closeWith(code);
      expect(await screen.findByText(m.documents_live_title_pending())).toBeTruthy();
      expect(reopen).not.toHaveBeenCalled();
      expect(opened).toHaveLength(1);
      expect(loadDraft('d1')).toBeUndefined();
      await fireEvent.click(screen.getByRole('button', { name: '다시 연결' }));
      expect(reopen).not.toHaveBeenCalled();
      expect(opened).toHaveLength(1);
      expect(screen.getByLabelText(m.documents_title_label())).toHaveValue(EDITED);
      const unload = new Event('beforeunload', { cancelable: true });
      window.dispatchEvent(unload);
      expect(unload.defaultPrevented).toBe(true);
    });
  }

  it('keeps the title on screen when a refused reconnect finds realtime off', async () => {
    vi.spyOn(api, 'document').mockResolvedValue({ ...doc, realtime: false });
    const off = vi.fn();
    render(LiveDocumentEditor, { props: { initial: doc, onrealtimeoff: off } });
    await editTitleOnly();
    closeWith(1006);
    await screen.findByText(m.documents_live_title_pending());
    expect(off).not.toHaveBeenCalled();
    expect(loadDraft('d1')).toBeUndefined();
  });

  it('stores an unconfirmed body with the saved title and never offers the short title after remount', async () => {
    const view = render(LiveDocumentEditor, { props: { initial: doc } });
    await editTitleOnly();
    await fireEvent.click(screen.getByRole('button', { name: 'stub-type' }));
    opened.at(-1)?.document.getText('t').insert(0, '본문');
    const unload = new Event('beforeunload', { cancelable: true });
    window.dispatchEvent(unload);
    expect(unload.defaultPrevented).toBe(true);
    expect(loadDraft('d1')?.title).toBe(doc.title);
    expect(JSON.stringify(loadDraft('d1')?.body)).toContain('확인받지 못한 문장');
    view.unmount();
    render(LiveDocumentEditor, { props: { initial: doc } });
    expect(screen.getByLabelText(m.documents_title_label())).toHaveValue(doc.title);
    expect(loadOffers('d1').every((draft) => draft.title === doc.title)).toBe(true);
  });
});
