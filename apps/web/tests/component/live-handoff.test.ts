import { fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { WebsocketProvider } from 'y-websocket';
import * as Y from 'yjs';

import type { DocNode, DocumentView } from '$lib/api/client';
import DocumentEditor from '$lib/features/documents/DocumentEditor.svelte';
import { loadDraft, storeDraft } from '$lib/features/documents/drafts';
import type { LiveSession } from '$lib/features/documents/live';
import LiveDocumentEditor from '$lib/features/documents/LiveDocumentEditor.svelte';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';

// CodeRabbit(#91): (A) 초안을 남기고 부모에게 다시 열기를 맡긴 뒤(`handOff`) 부모가 문서를 다시 읽는 동안 기존 편집기에서
// 더 친 글은 남길 곳이 없었다(없어질 때 다시 남기지 않으므로). 맡긴 뒤로는 제목·본문을 더 고치지 못하게 한다. 부모가 다시
// 읽지 못해 오류 화면으로 바꿔도 맡긴 초안은 남아 다음에 고르게 한다. (B) "다시 연결"이 초안을 남기지 못하고도(저장소 쓰기
// 실패, 고를 때까지 자리를 비워 둔 지난 초안) 편집기를 바꿨다. 처음 실패는 진행하지 않고 알리며, 알린 뒤 다시 누르면 다시
// 남겨 보고 진행한다(옮겨 둔 뒤의 명시적 진행).

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

const EDITED_TITLE = '저장하지 않은 제목';
const EDITED_BODY: DocNode = {
  type: 'doc',
  content: [{ type: 'paragraph', content: [{ type: 'text', text: '확인받지 못한 문장' }] }],
};

function titleInput(): HTMLInputElement {
  const el = document.getElementById('live-title');
  if (!(el instanceof HTMLInputElement)) throw new Error('live-title');
  return el;
}

async function editTitle(value = EDITED_TITLE): Promise<void> {
  await fireEvent.input(titleInput(), { target: { value } });
}

/** 서버 확인을 받지 못한 본문 편집(편집기가 본문을 넘기고 Yjs 문서가 바뀜). */
async function editBody(): Promise<void> {
  await fireEvent.click(screen.getByRole('button', { name: 'stub-type' }));
  const last = opened.at(-1);
  if (last === undefined) throw new Error('no connection');
  last.document.getText('t').insert(0, '확인받지 못한 문장');
}

function closeWith(code: number): void {
  const last = opened.at(-1);
  last?.provider.emit('connection-close', [new CloseEvent('close', { code }), last.provider]);
}

/** 이 편집기의 입력이 모두 잠겼는지(제목 칸·제목 저장·본문·파일 넣기). */
function expectLocked(): void {
  expect(titleInput().readOnly).toBe(true);
  const save = screen.getByRole('button', { name: m.documents_title_save() });
  expect(save).toHaveProperty('disabled', true);
  const body = screen.getByRole('button', { name: 'stub-type' });
  expect(body.dataset.editable).toBe('false');
  expect(body.dataset.insertFile).toBe('false');
}

/** 저장소 쓰기가 실패한다(가득 참). 이 탭의 id는 이미 sessionStorage에 있어 초안 키는 그대로다. */
function failDraftWrites(): void {
  vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
    throw new DOMException('full', 'QuotaExceededError');
  });
}

const lostAlert = (): HTMLElement | null => screen.queryByText(/이 브라우저에도 남기지 못했습니다/);

describe('handing the draft over to the parent', () => {
  const cases: { what: string; edit: () => Promise<void>; title: string; body: DocNode }[] = [
    { what: 'title only', edit: () => editTitle(), title: EDITED_TITLE, body: doc.body },
    { what: 'body only', edit: editBody, title: doc.title, body: EDITED_BODY },
    {
      what: 'title and body',
      edit: async () => {
        await editTitle();
        await editBody();
      },
      title: EDITED_TITLE,
      body: EDITED_BODY,
    },
  ];
  for (const c of cases) {
    it(`locks the editor while the parent reopens (${c.what}, 4410)`, async () => {
      // 부모가 문서를 다시 읽는 중(끝나지 않음).
      const off = vi.fn();
      render(LiveDocumentEditor, { props: { initial: doc, onrealtimeoff: off } });
      await c.edit();

      closeWith(4410);
      await screen.findByText(/서버에 저장되지 않은 편집이 있어/);

      expect(off).toHaveBeenCalledOnce();
      expectLocked();
      expect(loadDraft('d1')).toMatchObject({ title: c.title, body: c.body });
    });
  }

  // 부모가 다시 읽지 못하면 이 편집기를 오류 화면으로 바꾼다(`DocumentScreen`의 reopen). 맡긴 초안은 남아 다음에 열 때 고른다.
  it('leaves the handed-over draft to be offered when the parent fails and replaces the editor (4403)', async () => {
    const reload = vi.fn();
    const live = render(LiveDocumentEditor, { props: { initial: doc, onreload: reload } });
    await editTitle();
    closeWith(4403);
    await screen.findByText(/서버에 저장되지 않은 편집이 있어/);

    await fireEvent.click(screen.getByRole('button', { name: '다시 연결' }));
    expect(reload).toHaveBeenCalledOnce();
    expectLocked();
    expect(screen.getByRole('button', { name: '다시 연결' })).toHaveProperty('disabled', true);

    live.unmount();
    expect(loadDraft('d1')?.title).toBe(EDITED_TITLE);
    render(DocumentEditor, { props: { initial: { ...doc, realtime: false } } });
    expect(await screen.findByRole('region', { name: m.documents_draft_region() })).toBeTruthy();
  });
});

describe('reconnecting when the edits cannot be kept', () => {
  it('does not reopen on a failed write until I was told, then retries and reopens once it is kept', async () => {
    const reload = vi.fn();
    render(LiveDocumentEditor, { props: { initial: doc, onreload: reload } });
    // 고칠 것이 없을 때 권한이 바뀌어 멈췄다. 멈춘 뒤 제목을 더 고쳤다.
    closeWith(4403);
    await screen.findByRole('button', { name: '다시 연결' });
    await editTitle();
    failDraftWrites();

    await fireEvent.click(screen.getByRole('button', { name: '다시 연결' }));

    expect(reload).not.toHaveBeenCalled();
    expect(lostAlert()).not.toBeNull();
    expect(screen.queryByText(/초안으로 남겼습니다/)).toBeNull();
    expect(titleInput().value).toBe(EDITED_TITLE);
    expect(titleInput().readOnly).toBe(false);

    // 저장소가 다시 쓸 수 있게 됐다. 다시 누르면 남기고 다시 연다.
    vi.restoreAllMocks();
    await fireEvent.click(screen.getByRole('button', { name: '다시 연결' }));
    expect(reload).toHaveBeenCalledOnce();
    expect(loadDraft('d1')?.title).toBe(EDITED_TITLE);
  });

  it('reopens on the second press after telling me, even if it still cannot keep them', async () => {
    const reload = vi.fn();
    render(LiveDocumentEditor, { props: { initial: doc, onreload: reload } });
    closeWith(4403);
    await screen.findByRole('button', { name: '다시 연결' });
    await editTitle();
    failDraftWrites();

    await fireEvent.click(screen.getByRole('button', { name: '다시 연결' }));
    expect(reload).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByRole('button', { name: '다시 연결' }));
    expect(reload).toHaveBeenCalledOnce();
  });

  it('does not reopen over an earlier draft waiting for my choice, and leaves that draft alone', async () => {
    // 이 탭에 지난 초안이 있는데 열 때 줄로 옮기지 못했다(저장소 가득 참): 고를 때까지 그 자리를 비워 둔다.
    const earlier = {
      baseRevision: 2,
      title: '지난 초안',
      body: EDITED_BODY,
      savedAt: '2026-10-19T01:00:00Z',
    };
    storeDraft('d1', earlier);
    failDraftWrites();
    const reload = vi.fn();
    render(LiveDocumentEditor, { props: { initial: doc, onreload: reload } });
    vi.restoreAllMocks();
    closeWith(4403);
    await screen.findByRole('button', { name: '다시 연결' });
    await editTitle();

    await fireEvent.click(screen.getByRole('button', { name: '다시 연결' }));

    expect(reload).not.toHaveBeenCalled();
    expect(lostAlert()).not.toBeNull();
    expect(titleInput().value).toBe(EDITED_TITLE);
    expect(loadDraft('d1')).toEqual(earlier);
  });
});
