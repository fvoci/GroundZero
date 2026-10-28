import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
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
// 실패, 고를 때까지 자리를 비워 둔 지난 초안) 편집기를 바꿨다. 남기지 못하면 몇 번을 눌러도 진행하지 않고 알린다(알린 뒤
// 다시 누른 것을 버리기로 보지 않는다, 사용자 결정 2026-10-22). 버리려면 따로 "편집 버리고 다시 연결"을 골라 확인한다.
// 그때도 초안 저장소의 다른 초안(다른 문서·다른 계정·고를 때까지 둔 지난 초안)은 그대로 두고, 버린 편집은 남기지 않는다.

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

const reconnectButton = (): HTMLElement => screen.getByRole('button', { name: '다시 연결' });
const discardButton = (): HTMLElement | null =>
  screen.queryByRole('button', { name: m.documents_live_discard_reconnect() });
const confirmBox = (): HTMLElement | null =>
  screen.queryByRole('alertdialog', { name: m.common_discard_title() });

const EARLIER = {
  baseRevision: 2,
  title: '지난 초안',
  body: EDITED_BODY,
  savedAt: '2026-10-19T01:00:00Z',
};

/** 이 브라우저 저장소의 모든 키와 값. */
function storage(): Record<string, string | null> {
  const all: Record<string, string | null> = {};
  for (let i = 0; i < window.localStorage.length; i += 1) {
    const k = window.localStorage.key(i);
    if (k !== null) all[k] = window.localStorage.getItem(k);
  }
  return all;
}

/** 버리기와 상관없는 초안: 같은 사용자의 다른 문서, 같은 문서의 다른 계정. */
function storeOthers(): void {
  storeDraft('d2', { ...EARLIER, title: '다른 문서 초안' });
  me.userId = 'user-b';
  storeDraft('d1', { ...EARLIER, title: '다른 계정 초안' });
  me.userId = 'user-a';
}

/** 본문을 고친 뒤 권한이 바뀌었고, 저장소 쓰기가 실패해 다시 연결이 남기지 못했다. */
async function lostAfterDenied(reload: () => void): Promise<void> {
  render(LiveDocumentEditor, { props: { initial: doc, onreload: reload } });
  await editBody();
  closeWith(4403);
  await screen.findByRole('button', { name: '다시 연결' });
  failDraftWrites();
  await fireEvent.click(reconnectButton());
  expect(lostAlert()).not.toBeNull();
}

describe('handing the draft over to the parent', () => {
  it('locks the editor while the parent reopens with a body draft (4410)', async () => {
    const off = vi.fn();
    render(LiveDocumentEditor, { props: { initial: doc, onrealtimeoff: off } });
    await editBody();
    closeWith(4410);
    await screen.findByText(/서버에 저장되지 않은 편집이 있어/);
    expect(off).toHaveBeenCalledOnce();
    expectLocked();
    expect(loadDraft('d1')).toMatchObject({ title: doc.title, body: EDITED_BODY });
  });

  // 부모가 다시 읽지 못하면 이 편집기를 오류 화면으로 바꾼다(`DocumentScreen`의 reopen). 맡긴 초안은 남아 다음에 열 때 고른다.
  it('leaves the handed-over draft to be offered when the parent fails and replaces the editor (4403)', async () => {
    const reload = vi.fn();
    const live = render(LiveDocumentEditor, { props: { initial: doc, onreload: reload } });
    await editBody();
    closeWith(4403);
    await screen.findByText(/서버에 저장되지 않은 편집이 있어/);

    await fireEvent.click(screen.getByRole('button', { name: '다시 연결' }));
    expect(reload).toHaveBeenCalledOnce();
    expectLocked();
    expect(screen.getByRole('button', { name: '다시 연결' })).toHaveProperty('disabled', true);

    live.unmount();
    expect(loadDraft('d1')).toMatchObject({ title: doc.title, body: EDITED_BODY });
    render(DocumentEditor, { props: { initial: { ...doc, realtime: false } } });
    expect(await screen.findByRole('region', { name: m.documents_draft_region() })).toBeTruthy();
  });
});

describe('reconnecting when the edits cannot be kept', () => {
  it('does not reopen on a failed write, then retries and reopens once it is kept', async () => {
    const reload = vi.fn();
    render(LiveDocumentEditor, { props: { initial: doc, onreload: reload } });
    // 본문을 고친 뒤 권한이 바뀌었다.
    await editBody();
    closeWith(4403);
    await screen.findByRole('button', { name: '다시 연결' });
    expect(discardButton()).toBeNull();
    failDraftWrites();

    await fireEvent.click(reconnectButton());

    expect(reload).not.toHaveBeenCalled();
    expect(lostAlert()).not.toBeNull();
    expect(discardButton()).not.toBeNull();
    expect(screen.queryByText(/초안으로 남겼습니다/)).toBeNull();
    expect(opened.at(-1)?.document.getText('t').toString()).toContain('확인받지 못한 문장');
    expect(titleInput().readOnly).toBe(false);

    // 저장소가 다시 쓸 수 있게 됐다. 다시 누르면 남기고 다시 연다.
    vi.restoreAllMocks();
    await fireEvent.click(reconnectButton());
    expect(reload).toHaveBeenCalledOnce();
    expect(loadDraft('d1')).toMatchObject({ title: doc.title, body: EDITED_BODY });
  });

  // #91은 알린 뒤 다시 누르면 남기지 못해도 진행했다(알림을 본 것을 버리기로 봤다). 사용자 결정(2026-10-22)으로 바뀌었다:
  // 남기지 못하면 몇 번을 눌러도 진행하지 않는다.
  it('never reopens while the edits still cannot be kept, however many times I press Reconnect', async () => {
    const reload = vi.fn();
    await lostAfterDenied(reload);

    await fireEvent.click(reconnectButton());
    await fireEvent.click(reconnectButton());

    expect(reload).not.toHaveBeenCalled();
    expect(lostAlert()).not.toBeNull();
    expect(opened.at(-1)?.document.getText('t').toString()).toContain('확인받지 못한 문장');
    expect(titleInput().readOnly).toBe(false);
    expect(reconnectButton()).toHaveProperty('disabled', false);
  });

  it('does not reopen over an earlier draft waiting for my choice, and leaves that draft alone', async () => {
    // 이 탭에 지난 초안이 있는데 열 때 줄로 옮기지 못했다(저장소 가득 참): 고를 때까지 그 자리를 비워 둔다.
    storeDraft('d1', EARLIER);
    failDraftWrites();
    const reload = vi.fn();
    render(LiveDocumentEditor, { props: { initial: doc, onreload: reload } });
    vi.restoreAllMocks();
    await editBody();
    closeWith(4403);
    await screen.findByRole('button', { name: '다시 연결' });

    for (let press = 0; press < 3; press += 1) {
      await fireEvent.click(reconnectButton());
      expect(reload).not.toHaveBeenCalled();
      expect(lostAlert()).not.toBeNull();
      expect(opened.at(-1)?.document.getText('t').toString()).toContain('확인받지 못한 문장');
      expect(loadDraft('d1')).toEqual(EARLIER);
    }
  });
});

describe('discarding the edits that cannot be kept', () => {
  for (const way of ['keep editing', 'Escape'] as const) {
    it(`keeps my edits, returns focus and does not reopen when I back out (${way})`, async () => {
      const reload = vi.fn();
      await lostAfterDenied(reload);
      const before = storage();
      const discard = discardButton();
      if (discard === null) throw new Error('no discard button');
      discard.focus();

      await fireEvent.click(discard);
      const box = await screen.findByRole('alertdialog', { name: m.common_discard_title() });
      expect(box.textContent).toContain(m.documents_live_discard_description());
      if (way === 'Escape')
        await fireEvent.keyDown(document.activeElement ?? box, { key: 'Escape' });
      else
        await fireEvent.click(within(box).getByRole('button', { name: m.common_discard_keep() }));

      await waitFor(() => {
        expect(confirmBox()).toBeNull();
      });
      await waitFor(() => {
        expect(document.activeElement).toBe(discardButton());
      });
      expect(reload).not.toHaveBeenCalled();
      expect(lostAlert()).not.toBeNull();
      expect(opened.at(-1)?.document.getText('t').toString()).toContain('확인받지 못한 문장');
      expect(titleInput().readOnly).toBe(false);
      expect(storage()).toEqual(before);
    });
  }

  it('reopens without storing the discarded edits and leaves every other draft alone (4403)', async () => {
    storeOthers();
    const reload = vi.fn();
    const live = render(LiveDocumentEditor, { props: { initial: doc, onreload: reload } });
    failDraftWrites();
    // 본문을 고쳤는데 권한이 바뀌어 닫혔고, 이 브라우저에 남기지 못했다.
    await editBody();
    closeWith(4403);
    await screen.findByRole('button', { name: '다시 연결' });
    expect(lostAlert()).not.toBeNull();
    const before = storage();

    const discard = discardButton();
    if (discard === null) throw new Error('no discard button');
    await fireEvent.click(discard);
    const box = await screen.findByRole('alertdialog', { name: m.common_discard_title() });
    await fireEvent.click(
      within(box).getByRole('button', { name: m.documents_live_discard_reconnect() }),
    );

    expect(reload).toHaveBeenCalledOnce();
    expectLocked();
    // 저장소가 다시 쓸 수 있게 돼도 떠나거나 없어질 때 버린 편집을 남기지 않는다.
    vi.restoreAllMocks();
    const unload = new Event('beforeunload', { cancelable: true });
    window.dispatchEvent(unload);
    expect(unload.defaultPrevented).toBe(false);
    live.unmount();
    expect(storage()).toEqual(before);
    expect(loadDraft('d1')).toBeUndefined();
  });

  it('reopens in place without the discarded edits and still keeps what I type next (4409)', async () => {
    storeOthers();
    const live = render(LiveDocumentEditor, { props: { initial: doc } });
    failDraftWrites();
    await editBody();
    closeWith(4409);
    await screen.findByRole('button', { name: '다시 연결' });
    expect(lostAlert()).not.toBeNull();
    const before = storage();

    const discard = discardButton();
    if (discard === null) throw new Error('no discard button');
    await fireEvent.click(discard);
    const box = await screen.findByRole('alertdialog', { name: m.common_discard_title() });
    await fireEvent.click(
      within(box).getByRole('button', { name: m.documents_live_discard_reconnect() }),
    );

    expect(opened).toHaveLength(2);
    expect(titleInput().value).toBe(doc.title);
    expect(titleInput().readOnly).toBe(false);
    expect(lostAlert()).toBeNull();
    expect(discardButton()).toBeNull();
    expect(screen.getByRole('button', { name: 'stub-type' }).dataset.editable).toBe('true');
    vi.restoreAllMocks();
    const unload = new Event('beforeunload', { cancelable: true });
    window.dispatchEvent(unload);
    expect(unload.defaultPrevented).toBe(false);
    expect(storage()).toEqual(before);

    // D-27: 다시 연 뒤 제목만 고쳐도 영구 초안을 만들지 않는다(다른 본문 초안은 그대로 둔다).
    await editTitle('다시 연 뒤 고친 제목');
    expect(titleInput().value).toBe('다시 연 뒤 고친 제목');
    live.unmount();
    expect(loadDraft('d1')).toBeUndefined();
    expect(loadDraft('d2')?.title).toBe('다른 문서 초안');
  });

  it('keeps the snapshot this tab stored earlier and offers it after reopening in place (4409)', async () => {
    render(LiveDocumentEditor, { props: { initial: doc } });
    // 저장소에 쓸 수 있을 때 고친 본문은 이 탭의 초안으로 남았다. 그 뒤 쓰기가 실패하기 시작했다.
    await editBody();
    await waitFor(() => {
      expect(loadDraft('d1')?.body).toEqual(EDITED_BODY);
    });
    const snapshot = loadDraft('d1');
    failDraftWrites();
    await editBody();
    closeWith(4409);
    await screen.findByRole('button', { name: '다시 연결' });
    expect(lostAlert()).not.toBeNull();

    const discard = discardButton();
    if (discard === null) throw new Error('no discard button');
    await fireEvent.click(discard);
    const box = await screen.findByRole('alertdialog', { name: m.common_discard_title() });
    await fireEvent.click(
      within(box).getByRole('button', { name: m.documents_live_discard_reconnect() }),
    );

    expect(opened).toHaveLength(2);
    expect(loadDraft('d1')).toEqual(snapshot);
    // 다시 맞춰지면 그 초안을 고르게 한다(버리지 않음).
    opened.at(-1)?.provider.emit('sync', [true]);
    expect(await screen.findByRole('region', { name: m.documents_draft_region() })).toBeTruthy();
    expect(loadDraft('d1')).toEqual(snapshot);
  });
});
