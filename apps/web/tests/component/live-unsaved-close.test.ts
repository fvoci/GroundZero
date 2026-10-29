import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { WebsocketProvider } from 'y-websocket';
import * as Y from 'yjs';

import type { DocumentView } from '$lib/api/client';
import { api } from '$lib/api/client';
import { documentsApi } from '$lib/features/documents/api';
import { loadOffers } from '$lib/features/documents/drafts';
import type { LiveSession } from '$lib/features/documents/live';
import LiveDocumentEditor from '$lib/features/documents/LiveDocumentEditor.svelte';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';

/** 연 연결들(가장 최근 것이 끝). 서버 대신 이 시험이 닫기 코드와 맞춰짐을 보낸다. */
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
        // 소켓을 열지 않는다(이 시험이 이벤트를 보낸다).
        provider: new WebsocketProvider('ws://localhost', 'collab', document, { connect: false }),
        user,
        // 서버의 저장 확인은 오지 않는다(확인받지 못한 편집).
        persisted: { on: () => undefined, off: () => undefined },
      };
      opened.push(session);
      return session;
    },
  };
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
  can_export: true,
};

/** 서버 확인을 받지 못한 내 편집을 하나 만든다(편집기가 본문을 넘기고 Yjs 문서가 바뀜). */
async function editUnconfirmed(): Promise<void> {
  await fireEvent.click(screen.getByRole('button', { name: 'stub-type' }));
  const last = opened.at(-1);
  if (last === undefined) throw new Error('no connection');
  last.document.getText('t').insert(0, '확인받지 못한 문장');
}

/** 서버가 연결을 닫는다. */
function closeWith(code: number): void {
  const last = opened.at(-1);
  last?.provider.emit('connection-close', [new CloseEvent('close', { code }), last.provider]);
}

// 미확인 편집도 사용자 입력이다: 서버가 4403(권한·세션 끝)·4409(문서가 바뀜)로 닫아 반영하지 않아도 브라우저에서
// 없애지 않는다. 이 사용자의 초안으로 남기고, "저장되지 않았다"고 알리며, 다시 열면 내 노트로 보존하거나 버리게 한다.
describe('live editor closed by the server with unconfirmed edits', () => {
  it('keeps the kept-body notice after a title save', async () => {
    await setLocale('ko', { reload: false });
    me.userId = 'user-a';
    vi.spyOn(api, 'document').mockResolvedValue(doc);
    vi.spyOn(documentsApi, 'patchTitle').mockResolvedValue({
      kind: 'saved',
      document: { ...doc, title: '제목만 저장', revision: 4 },
    });
    const saved = vi.fn();
    render(LiveDocumentEditor, { props: { initial: doc, onsaved: saved } });
    await editUnconfirmed();
    closeWith(4409);
    await screen.findByText(m.documents_live_unsaved_kept());
    expect(opened).toHaveLength(2);

    await fireEvent.input(screen.getByLabelText(m.documents_title_label()), {
      target: { value: '제목만 저장' },
    });
    await fireEvent.click(screen.getByRole('button', { name: m.documents_title_save() }));
    await waitFor(() => {
      expect(saved).toHaveBeenCalledOnce();
    });
    expect(loadOffers('d1')).toHaveLength(1);
    expect(JSON.stringify(loadOffers('d1')[0]?.body)).toContain('확인받지 못한 문장');
    expect(screen.queryByText(m.documents_live_unsaved_kept())).not.toBeNull();

    opened.at(-1)?.provider.emit('sync', [true]);
    await screen.findByRole('region', { name: m.documents_draft_region() });
    expect(screen.queryByText(m.documents_live_unsaved_kept())).toBeNull();
    await fireEvent.click(screen.getByRole('button', { name: m.documents_draft_discard() }));
    expect(loadOffers('d1')).toEqual([]);
  });

  for (const code of [4403, 4409]) {
    it(`keeps them as this user's draft, says so, and offers them again (${String(code)})`, async () => {
      await setLocale('ko', { reload: false });
      me.userId = 'user-a';
      const view = render(LiveDocumentEditor, { props: { initial: doc } });
      await editUnconfirmed();

      closeWith(code);

      const notice = await screen.findByText(/서버에 저장되지 않은 편집이 있어/);
      expect(notice.closest('[aria-live]')).not.toBeNull();
      if (code === 4403) {
        // 권한이 바뀌어 멈춘다(다시 연결은 사람이 누른다). 초안은 이 사용자의 키에만 남는다.
        expect(screen.getByRole('button', { name: '다시 연결' })).toBeTruthy();
        view.unmount();
        me.userId = 'user-b';
        expect(loadOffers('d1')).toEqual([]);
        me.userId = 'user-a';
        render(LiveDocumentEditor, { props: { initial: doc } });
      }
      // 다시 열려 맞춰지면 남긴 초안을 고르게 한다(내 노트로 보존 또는 버림). 알림은 그 선택으로 바뀐다.
      opened.at(-1)?.provider.emit('sync', [true]);
      const offer = await screen.findByRole('region', { name: /초안/ });
      expect(offer.textContent).toMatch(/서버에 전해지지 않은 이 문서의 편집/);
      expect(screen.getByRole('button', { name: /내 노트로 보존/ })).toBeTruthy();
      await waitFor(() => {
        expect(screen.queryByText(/서버에 저장되지 않은 편집이 있어/)).toBeNull();
      });
    });
  }
});

describe('live editor that cannot keep unconfirmed edits', () => {
  it('says they were not kept and keeps them on screen instead of reopening', async () => {
    await setLocale('ko', { reload: false });
    me.userId = 'user-a';
    render(LiveDocumentEditor, { props: { initial: doc } });
    await editUnconfirmed();
    // 저장 공간이 가득 찼다.
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new DOMException('full', 'QuotaExceededError');
    });

    closeWith(4409);

    const alert = await screen.findByRole('alert');
    expect(alert.textContent).toMatch(/이 브라우저에도 남기지 못했습니다/);
    expect(screen.queryByText(/초안으로 남겼습니다/)).toBeNull();
    // 새 내용으로 바꾸지 않는다(다시 열지 않음). 사람이 옮겨 둔 뒤 다시 연결한다.
    expect(opened).toHaveLength(1);
    expect(screen.getByText(/다시 연결하면 새 내용으로 엽니다/)).toBeTruthy();
    expect(screen.getByRole('button', { name: 'stub-type' })).toBeTruthy();
    // 아직 남길 수 없으면 다시 연결해도 바꾸지 않는다(사용자 결정 2026-10-22). 남길 수 있게 되면 남기고 다시 연다.
    await fireEvent.click(screen.getByRole('button', { name: '다시 연결' }));
    expect(opened).toHaveLength(1);
    expect(screen.getByRole('alert').textContent).toMatch(/이 브라우저에도 남기지 못했습니다/);
    vi.restoreAllMocks();
    await fireEvent.click(screen.getByRole('button', { name: '다시 연결' }));
    expect(opened).toHaveLength(2);
    expect(screen.queryByRole('alert')).toBeNull();
  });
});

describe('choosing an earlier draft', () => {
  it('keeps the notice while a draft kept at a later close is still waiting', async () => {
    await setLocale('ko', { reload: false });
    me.userId = 'user-a';
    render(LiveDocumentEditor, { props: { initial: doc } });
    await editUnconfirmed();
    closeWith(4409);
    opened.at(-1)?.provider.emit('sync', [true]);
    await screen.findByRole('region', { name: /초안/ });
    // 지난 초안을 고르기 전에 새 편집이 또 확인받지 못한 채 닫힌다(이 탭의 초안으로 남음).
    await editUnconfirmed();
    closeWith(4403);

    await fireEvent.click(screen.getByRole('button', { name: '초안 버리기' }));

    expect(await screen.findByText(/서버에 저장되지 않은 편집이 있어/)).toBeTruthy();
  });
});
