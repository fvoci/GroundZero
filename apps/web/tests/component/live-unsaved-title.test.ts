import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { WebsocketProvider } from 'y-websocket';
import * as Y from 'yjs';

import type { DocumentView } from '$lib/api/client';
import { api } from '$lib/api/client';
import DocumentEditor from '$lib/features/documents/DocumentEditor.svelte';
import { loadDraft, loadOffers } from '$lib/features/documents/drafts';
import type { LiveSession } from '$lib/features/documents/live';
import LiveDocumentEditor from '$lib/features/documents/LiveDocumentEditor.svelte';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';

// CodeRabbit(#64): 서버가 연결을 닫을 때 확인받지 못한 본문만 보고 초안을 남겼다. 제목만 고친 채(본문은 맞춰짐) 실시간이
// 꺼지거나(4410, 다시 붙기 거절 뒤 문서가 실시간 아님) 권한이 바뀌어(4403) 다시 열면 저장하지 않은 제목이 사라졌다. 이제
// 저장하지 않은 제목도 초안으로 남기고 알리며, 일반 편집기가 다시 열 때 그 제목을 되살리게 한다.

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

const keptNotice = (): HTMLElement | null => screen.queryByText(/서버에 저장되지 않은 편집이 있어/);

describe('a title-only edit when the server ends live editing', () => {
  it('is kept as a draft when realtime is turned off (4410), and the plain editor restores it', async () => {
    const off = vi.fn();
    const view = render(LiveDocumentEditor, { props: { initial: doc, onrealtimeoff: off } });
    await editTitleOnly();

    closeWith(4410);

    expect(off).toHaveBeenCalledOnce();
    expect(loadDraft('d1')?.title).toBe(EDITED);
    expect(await screen.findByText(/서버에 저장되지 않은 편집이 있어/)).toBeTruthy();

    // 부모가 문서를 다시 읽어 일반 편집기로 연다(DocumentScreen의 reopen). 남긴 초안의 제목을 되살린다.
    view.unmount();
    render(DocumentEditor, { props: { initial: { ...doc, realtime: false, revision: 4 } } });
    await screen.findByRole('region', { name: m.documents_draft_region() });
    await fireEvent.click(screen.getByRole('button', { name: m.documents_draft_restore() }));
    const title = document.getElementById('doc-title');
    if (!(title instanceof HTMLInputElement)) throw new Error('doc-title');
    expect(title.value).toBe(EDITED);
  });

  it('is kept as a draft when a refused reconnect finds realtime off', async () => {
    vi.spyOn(api, 'document').mockResolvedValue({ ...doc, realtime: false });
    const off = vi.fn();
    render(LiveDocumentEditor, { props: { initial: doc, onrealtimeoff: off } });
    await editTitleOnly();

    closeWith(1006);

    await waitFor(() => {
      expect(off).toHaveBeenCalledOnce();
    });
    expect(loadDraft('d1')?.title).toBe(EDITED);
    expect(await screen.findByText(/서버에 저장되지 않은 편집이 있어/)).toBeTruthy();
  });

  it('is kept as a draft and announced when access changes (4403), before reconnecting reopens', async () => {
    const reload = vi.fn();
    render(LiveDocumentEditor, { props: { initial: doc, onreload: reload } });
    await editTitleOnly();

    closeWith(4403);

    expect(await screen.findByText(/서버에 저장되지 않은 편집이 있어/)).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: '다시 연결' }));
    expect(reload).toHaveBeenCalledOnce();
    expect(loadDraft('d1')?.title).toBe(EDITED);
  });

  // 4409는 같은 편집기에서 본문 편집기만 새로 연다. 제목 칸은 그대로라 저장하지 않은 제목이 화면에 남고, 남긴 초안은 지금
  // 화면과 같으므로 고르라고 내놓지 않는다.
  it('stays on screen when the document changed (4409) and is not offered back', async () => {
    render(LiveDocumentEditor, { props: { initial: doc } });
    await editTitleOnly();

    closeWith(4409);
    expect(opened).toHaveLength(2);
    opened.at(-1)?.provider.emit('sync', [true]);

    await waitFor(() => {
      expect(loadOffers('d1')).toEqual([]);
    });
    await waitFor(() => {
      expect(keptNotice()).toBeNull();
    });
    expect(screen.queryByRole('region', { name: m.documents_draft_region() })).toBeNull();
    const title = document.getElementById('live-title');
    if (!(title instanceof HTMLInputElement)) throw new Error('live-title');
    expect(title.value).toBe(EDITED);
  });
});
