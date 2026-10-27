import { fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { WebsocketProvider } from 'y-websocket';
import * as Y from 'yjs';

import type { DocumentView } from '$lib/api/client';
import FeedbackHost from '$lib/features/feedback/FeedbackHost.svelte';
import type { LiveSession } from '$lib/features/documents/live';
import LiveDocumentEditor from '$lib/features/documents/LiveDocumentEditor.svelte';
import { me } from '$lib/me.svelte';
import { scopedKey } from '$lib/storage';
import { setLocale } from '$lib/paraglide/runtime.js';

// CodeRabbit(#60 8e42036): 첫 동기화 전이라 편집기가 본문을 넘기지 않았을 때 제목만 고치고 계정이 바뀌어도(4412) 그
// 제목을 앞 계정의 초안으로 남기고 다시 읽는다(남기지 못한다고 멈추거나 떠나기를 막지 않는다).

const opened: LiveSession[] = [];

vi.mock('$lib/features/documents/TiptapEditor.svelte', async () => ({
  default: (await import('./stubs/SilentEditorStub.svelte')).default,
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

afterEach(() => {
  vi.unstubAllGlobals();
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

describe('a title edited before the first live sync', () => {
  it('is kept for the first account on a 4412 and on leaving', async () => {
    await setLocale('ko', { reload: false });
    vi.stubGlobal('matchMedia', (query: string) => ({
      matches: false,
      media: query,
      addEventListener: () => undefined,
      removeEventListener: () => undefined,
    }));
    me.userId = 'user-a';
    const reload = vi.fn();
    render(FeedbackHost, { props: { reload } });
    render(LiveDocumentEditor, { props: { initial: doc } });
    expect(screen.getByTestId('silent-editor')).toBeTruthy();
    const title = document.getElementById('live-title');
    if (!(title instanceof HTMLInputElement)) throw new Error('live-title');
    await fireEvent.input(title, { target: { value: '동기화 전에 고친 제목' } });

    const unload = new Event('beforeunload', { cancelable: true });
    window.dispatchEvent(unload);
    expect(unload.defaultPrevented).toBe(false);

    const last = opened.at(-1);
    last?.provider.emit('connection-close', [
      new CloseEvent('close', { code: 4412 }),
      last.provider,
    ]);
    expect(reload).toHaveBeenCalledOnce();
    const kept = Object.keys(window.localStorage)
      .filter((k) => k.startsWith(scopedKey('draft:d1', 'user-a', undefined)))
      .map((k) => window.localStorage.getItem(k) ?? '');
    expect(kept.some((v) => v.includes('동기화 전에 고친 제목'))).toBe(true);
  });
});
