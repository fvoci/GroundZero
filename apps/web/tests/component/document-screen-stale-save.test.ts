import type { Workspace } from '@fvoci/api-client';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { tick } from 'svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { DocNode, DocumentView } from '$lib/api/client';
import { api } from '$lib/api/client';
import DocumentScreen from '$lib/features/documents/DocumentScreen.svelte';
import { me } from '$lib/me.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

// 실시간 연결은 열지 않는다: 접속하지 않는 provider로 같은 모양의 세션을 돌려준다.
vi.mock(import('$lib/features/documents/live'), async (importOriginal) => {
  const actual = await importOriginal();
  const Y = await import('yjs');
  const { WebsocketProvider } = await import('y-websocket');
  return {
    ...actual,
    openLive: (documentId: string, user: { name: string; color: string }) => {
      const document = new Y.Doc();
      const provider = new WebsocketProvider('ws://127.0.0.1:9', documentId, document, {
        connect: false,
        disableBc: true,
      });
      return {
        document,
        provider,
        user,
        persisted: { on: () => undefined, off: () => undefined },
      };
    },
  };
});

const workspaces: Workspace[] = [
  { id: 'ws-1', name: '공간', role: 'writer', is_owner: false, realtime_enabled: true },
];

function paragraph(text: string): DocNode {
  return { type: 'doc', content: [{ type: 'paragraph', content: [{ type: 'text', text }] }] };
}

function liveDocument(id: string, title: string, revision = 3): DocumentView {
  return {
    id,
    workspace_id: 'ws-1',
    placement: 'wiki',
    item_id: null,
    parent_id: null,
    reference: `WIKI-${id}`,
    title,
    body: paragraph(`${title} 본문`),
    schema_version: 1,
    revision,
    updated_at: '2026-10-19T00:00:00Z',
    access: 'write',
    can_edit: true,
    visibility: 'workspace',
    allow_copy: true,
    can_manage: false,
    can_delete: false,
    is_template: false,
    realtime: true,
    can_export: false,
  };
}

function liveTitle(): HTMLInputElement {
  const input = document.querySelector('#live-title');
  if (!(input instanceof HTMLInputElement)) throw new Error('live title');
  return input;
}

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  Range.prototype.getBoundingClientRect = (): DOMRect => new DOMRect(0, 0, 0, 0);
  const none: DOMRectList = Object.assign([], { item: (): DOMRect | null => null });
  Range.prototype.getClientRects = (): DOMRectList => none;
  Element.prototype.scrollIntoView = (): void => undefined;
  me.userId = 'user-a';
  vi.spyOn(api, 'mark').mockResolvedValue({ pin: false, favorite: false });
  vi.spyOn(api, 'comments').mockResolvedValue([]);
  vi.spyOn(api, 'documentFiles').mockResolvedValue([]);
  vi.spyOn(api, 'documentBacklinks').mockResolvedValue([]);
  vi.spyOn(api, 'wikiDocuments').mockResolvedValue([]);
});

afterEach(() => {
  vi.restoreAllMocks();
  window.localStorage.clear();
  me.userId = '';
});

// 감사 W8: 실시간 문서 A의 제목을 고치다 사이드바에서 B를 누르면(제목 칸을 떠나며 A의 제목 저장이 시작됨), 늦게 온
// A의 저장 결과가 B의 주소 아래에 A를 다시 보여 주면 안 된다.
describe('a late save for the previous document', () => {
  it('does not put document A back on screen after switching to B', async () => {
    vi.spyOn(api, 'document').mockImplementation((id: string) =>
      Promise.resolve(id === 'A' ? liveDocument('A', 'A 제목') : liveDocument('B', 'B 제목')),
    );
    let finishSave: (doc: DocumentView) => void = () => undefined;
    const save = vi.spyOn(api, 'saveDocument').mockImplementation(
      () =>
        new Promise((resolve) => {
          finishSave = (doc: DocumentView) => {
            resolve({ kind: 'saved', document: doc });
          };
        }),
    );
    const view = render(DocumentScreen, { props: { documentId: 'A', workspaces } });
    await waitFor(() => {
      expect(liveTitle().value).toBe('A 제목');
    });

    // 제목을 고친 채 사이드바의 B를 누른다: 먼저 제목 칸을 떠나 A의 제목 저장이 시작된다.
    await fireEvent.input(liveTitle(), { target: { value: 'A 새 제목' } });
    await fireEvent.blur(liveTitle());
    await waitFor(() => {
      expect(save).toHaveBeenCalledTimes(1);
    });
    await view.rerender({ documentId: 'B', workspaces });
    await waitFor(() => {
      expect(liveTitle().value).toBe('B 제목');
    });

    // 그 뒤에 A의 저장이 끝난다.
    finishSave(liveDocument('A', 'A 새 제목', 4));
    await tick();
    await new Promise((r) => setTimeout(r, 0));
    await tick();

    expect(liveTitle().value).toBe('B 제목');
    expect(screen.queryByText('WIKI-A')).toBeNull();
  });
});
