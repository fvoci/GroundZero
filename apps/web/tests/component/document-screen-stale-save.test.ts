import { activityApi } from '$lib/features/activity/api';
import type { Workspace } from '@fvoci/api-client';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { tick } from 'svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { DocNode, DocumentView } from '$lib/api/client';
import { api } from '$lib/api/client';
import { documentsApi } from '$lib/features/documents/api';
import DocumentScreen from '$lib/features/documents/DocumentScreen.svelte';
import { workspaceNav } from '$lib/features/nav/workspace-nav.svelte';
import { me } from '$lib/me.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';
import { m } from '$lib/paraglide/messages.js';
import { router } from '$lib/router.svelte';

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
  {
    access: 'write',
    id: 'ws-1',
    name: '공간',
    organization_id: 'org-1',
    role: 'member',
    is_owner: false,
  },
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
  vi.spyOn(activityApi, 'mark').mockResolvedValue({ pin: false, favorite: false });
  vi.spyOn(activityApi, 'comments').mockResolvedValue([]);
  vi.spyOn(api, 'documentFiles').mockResolvedValue([]);
  vi.spyOn(api, 'documentBacklinks').mockResolvedValue([]);
  vi.spyOn(api, 'wikiDocuments').mockResolvedValue([]);
});

afterEach(() => {
  vi.restoreAllMocks();
  window.localStorage.clear();
  me.userId = '';
});

it('warns and can cancel navigation with only an unsaved document comment', async () => {
  vi.spyOn(api, 'document').mockResolvedValue(liveDocument('A', 'A 제목'));
  render(DocumentScreen, { props: { documentId: 'A', workspaces } });
  const input = await screen.findByLabelText(/새 댓글/);
  const cleanUnload = new Event('beforeunload', { cancelable: true });
  window.dispatchEvent(cleanUnload);
  expect(cleanUnload.defaultPrevented).toBe(false);

  await fireEvent.input(input, { target: { value: '본문과 별개인 댓글 초안' } });
  const unload = new Event('beforeunload', { cancelable: true });
  window.dispatchEvent(unload);
  expect(unload.defaultPrevented).toBe(true);
  const navigate = vi.fn();
  expect(router.guard?.(navigate)).toBe(false);
  await screen.findByRole('alertdialog');
  await fireEvent.click(screen.getByRole('button', { name: m.common_discard_keep() }));
  expect(navigate).not.toHaveBeenCalled();
  expect(input).toHaveValue('본문과 별개인 댓글 초안');
});

// 감사 W8: 실시간 문서 A의 제목을 고치다 사이드바에서 B를 누르면(제목 칸을 떠나며 A의 제목 저장이 시작됨), 늦게 온
// A의 저장 결과가 B의 주소 아래에 A를 다시 보여 주면 안 된다.
describe('a late save for the previous document', () => {
  it('does not put document A back on screen after switching to B', async () => {
    vi.spyOn(api, 'document').mockImplementation((id: string) =>
      Promise.resolve(id === 'A' ? liveDocument('A', 'A 제목') : liveDocument('B', 'B 제목')),
    );
    let finishSave: (doc: DocumentView) => void = () => undefined;
    const save = vi.spyOn(documentsApi, 'patchTitle').mockImplementation(
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

  it('does not overwrite document A reopened meanwhile (A → B → A) with the old save result', async () => {
    // 다시 열 때 A는 그사이 리비전 5가 되었다(다른 사람의 편집이 리비전으로 남음).
    let revisionOfA = 3;
    vi.spyOn(api, 'document').mockImplementation((id: string) =>
      Promise.resolve(
        id === 'A' ? liveDocument('A', 'A 제목', revisionOfA) : liveDocument('B', 'B 제목'),
      ),
    );
    vi.spyOn(api, 'revisions').mockResolvedValue([]);
    let finishSave: (doc: DocumentView) => void = () => undefined;
    const save = vi.spyOn(documentsApi, 'patchTitle').mockImplementation(
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

    await fireEvent.input(liveTitle(), { target: { value: 'A 새 제목' } });
    await fireEvent.blur(liveTitle());
    await waitFor(() => {
      expect(save).toHaveBeenCalledTimes(1);
    });
    await view.rerender({ documentId: 'B', workspaces });
    await waitFor(() => {
      expect(liveTitle().value).toBe('B 제목');
    });
    revisionOfA = 5;
    await view.rerender({ documentId: 'A', workspaces });
    await waitFor(() => {
      expect(liveTitle().value).toBe('A 제목');
    });
    await fireEvent.click(screen.getByRole('button', { name: '기록' }));
    await screen.findByText(/^지금은 리비전 5입니다\./);

    // 처음 연 A의 편집기(이미 사라짐)가 보낸 제목 저장이 이제 끝난다(리비전 4).
    const refresh = vi.spyOn(workspaceNav, 'refreshDocs');
    finishSave(liveDocument('A', 'A 새 제목', 4));
    await tick();
    await new Promise((r) => setTimeout(r, 0));
    await tick();

    // 화면의 A는 다시 읽은 리비전 5 그대로다(기록·복원의 기준이 옛 리비전으로 돌아가지 않는다).
    expect(screen.getByText(/^지금은 리비전 \d+입니다\./).textContent).toMatch(
      /^지금은 리비전 5입니다\./,
    );
    // 사이드바 목록은 새 제목으로 다시 읽는다.
    expect(refresh).toHaveBeenCalled();
  });
});
