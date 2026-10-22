import { fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { WebsocketProvider } from 'y-websocket';
import * as Y from 'yjs';

import type { DocumentView } from '$lib/api/client';
import FeedbackHost from '$lib/features/feedback/FeedbackHost.svelte';
import type { LiveSession } from '$lib/features/documents/live';
import LiveDocumentEditor from '$lib/features/documents/LiveDocumentEditor.svelte';
import { me } from '$lib/me.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

// #60(사용자 결정 2026-10-22): 다른 탭에서 계정을 바꾼 뒤 y-websocket이 새 쿠키로 다시 붙으면 앞 계정 화면의
// 미동기화 편집이 새 계정 이름으로 합쳐질 수 있었다. 실시간 연결도 편집기를 열 때의 사용자 id를 `expected_user`로
// 싣고(다시 붙을 때도 같은 값), 서버가 세션의 사용자와 다르면 방에 들어가기 전에 4412로 닫는다. 화면은 다시 붙지
// 않고 확인받지 못한 편집을 앞 계정의 초안으로 남긴 채 계정 전환 처리(다시 읽기)로 넘긴다.

/** 편집기가 연 연결(가장 최근 것이 끝)과 그때 넘긴 기대 사용자. 서버 대신 이 시험이 닫기 코드를 보낸다. */
const opened: { session: LiveSession; expectedUser: unknown }[] = [];

vi.mock('$lib/features/documents/TiptapEditor.svelte', async () => ({
  default: (await import('./stubs/EditorStub.svelte')).default,
}));

vi.mock('$lib/features/documents/live', async (original) => {
  const real = await original<typeof import('$lib/features/documents/live')>();
  return {
    ...real,
    openLive: (
      _id: string,
      user: LiveSession['user'],
      _write: boolean,
      expectedUser?: unknown,
    ): LiveSession => {
      const document = new Y.Doc();
      const session: LiveSession = {
        document,
        // 소켓을 열지 않는다(이 시험이 이벤트를 보낸다).
        provider: new WebsocketProvider('ws://localhost', 'collab', document, { connect: false }),
        user,
        // 서버의 저장 확인은 오지 않는다(확인받지 못한 편집).
        persisted: { on: () => undefined, off: () => undefined },
      };
      opened.push({ session, expectedUser });
      return session;
    },
  };
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

const drafts = (user: string): string[] =>
  Object.keys(window.localStorage).filter((k) => k.startsWith(`fvoci:draft:${user}:d1`));

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  vi.stubGlobal('matchMedia', (query: string) => ({
    matches: false,
    media: query,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
  }));
});
afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  opened.length = 0;
  window.localStorage.clear();
  me.userId = '';
});

describe('the live connection carries the user the editor was opened with', () => {
  it('puts it in the connection address, the same on every reconnect', async () => {
    // 이 파일은 편집기 시험을 위해 openLive를 바꿔 끼웠다. 여기서는 실제 것을 쓴다.
    const { openLive } = await vi.importActual<typeof import('$lib/features/documents/live')>(
      '$lib/features/documents/live',
    );
    // 소켓을 열지 않는 WebSocket 대신(주소만 본다).
    class FakeSocket {
      binaryType = 'arraybuffer';
      close(): void {
        // 열지 않았다.
      }
      send(): void {
        // 보내지 않는다.
      }
    }
    vi.stubGlobal('WebSocket', FakeSocket);
    const live = openLive('d1', { name: '나', color: '#1d4ed8' }, true, 'user-a');
    expect(new URL(live.provider.url).searchParams.get('expected_user')).toBe('user-a');
    expect(new URL(live.provider.url).searchParams.get('write')).toBe('1');
    // 다른 탭에서 계정을 바꿔도(이 창의 사용자 표시가 바뀌어도) 다시 붙는 주소는 그대로다.
    me.userId = 'user-b';
    live.provider.disconnect();
    expect(new URL(live.provider.url).searchParams.get('expected_user')).toBe('user-a');
    live.provider.destroy();
    live.document.destroy();
  });
});

describe('a live connection refused because the account changed', () => {
  it('stops reconnecting, keeps unconfirmed edits for the first account, says so and reloads', async () => {
    me.userId = 'user-a';
    const reload = vi.fn();
    render(FeedbackHost, { props: { reload } });
    render(LiveDocumentEditor, { props: { initial: doc } });
    expect(opened.map((o) => o.expectedUser)).toEqual(['user-a']);

    // 서버 확인을 받지 못한 내 편집.
    await fireEvent.click(screen.getByRole('button', { name: 'stub-type' }));
    const last = opened.at(-1);
    if (last === undefined) throw new Error('no connection');
    last.session.document.getText('t').insert(0, '확인받지 못한 문장');
    const disconnect = vi.spyOn(last.session.provider, 'disconnect');

    // 다른 탭에서 u2로 로그인한 뒤 다시 붙었고, 서버가 방에 들어가기 전에 닫았다.
    last.session.provider.emit('connection-close', [
      new CloseEvent('close', { code: 4412 }),
      last.session.provider,
    ]);

    expect(disconnect).toHaveBeenCalled();
    expect(last.session.provider.shouldConnect).toBe(false);
    expect(opened).toHaveLength(1);
    expect(reload).toHaveBeenCalledOnce();
    expect(drafts('user-a').length).toBeGreaterThan(0);
    expect(drafts('user-b')).toEqual([]);
    expect(await screen.findByText(/다른 계정으로 로그인돼 실시간 연결을 멈췄습니다/)).toBeTruthy();
    // 같은 값으로 다시 붙어도 다시 거절되므로 다시 연결 단추를 두지 않는다.
    expect(screen.queryByRole('button', { name: '다시 연결' })).toBeNull();
  });
});
