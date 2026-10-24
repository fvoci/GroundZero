import type { Workspace } from '@fvoci/api-client';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { DocumentView } from '$lib/api/client';
import { api } from '$lib/api/client';
import DocumentScreen from '$lib/features/documents/DocumentScreen.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

// 375px에서 문서 동작 단추 여덟 개(공유·설정, 복제, 이동, 기록, 내보내기, 홈에 고정, 즐겨찾기, 문서 지우기)가 두 줄을
// 차지했다. 공유·설정과 기록만 두고 나머지는 "더 보기" 메뉴로 옮긴다. 권한이 없는 동작은 메뉴에도 없다. 메뉴에서 연
// 일이 끝나면 초점은 "더 보기"로 돌아온다(대화상자의 초점 복귀는 실제 브라우저에서도 확인했다).
const workspaces: Workspace[] = [
  { id: 'ws-1', name: '공간', role: 'writer', is_owner: false, realtime_enabled: false },
];

function documentView(over: Partial<DocumentView>): DocumentView {
  return {
    id: 'doc-1',
    workspace_id: 'ws-1',
    placement: 'wiki',
    item_id: null,
    parent_id: null,
    reference: 'WIKI-1',
    title: '운영 안내',
    body: { type: 'doc', content: [{ type: 'paragraph' }] },
    schema_version: 1,
    revision: 1,
    updated_at: '2026-10-21T00:00:00Z',
    access: 'owner',
    can_edit: true,
    visibility: 'workspace',
    allow_copy: true,
    can_manage: true,
    can_delete: true,
    is_template: false,
    realtime: false,
    can_export: true,
    ...over,
  };
}

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  Range.prototype.getBoundingClientRect = (): DOMRect => new DOMRect(0, 0, 0, 0);
  const none: DOMRectList = Object.assign([], { item: (): DOMRect | null => null });
  Range.prototype.getClientRects = (): DOMRectList => none;
  Element.prototype.scrollIntoView = (): void => undefined;
  vi.spyOn(api, 'mark').mockResolvedValue({ pin: true, favorite: false });
  vi.spyOn(api, 'comments').mockResolvedValue([]);
  vi.spyOn(api, 'documentFiles').mockResolvedValue([]);
  vi.spyOn(api, 'documentBacklinks').mockResolvedValue([]);
  vi.spyOn(api, 'wikiDocuments').mockResolvedValue([]);
  vi.spyOn(api, 'revisions').mockResolvedValue([]);
});
afterEach(() => {
  vi.restoreAllMocks();
  window.localStorage.clear();
});

async function openMore(): Promise<HTMLElement> {
  const more = await screen.findByRole('button', { name: '더 보기' });
  await fireEvent.click(more);
  return screen.findByRole('menu');
}

describe('document actions menu', () => {
  it('keeps share and history in the toolbar and moves the rest into the menu', async () => {
    vi.spyOn(api, 'document').mockResolvedValue(documentView({}));
    render(DocumentScreen, { props: { documentId: 'doc-1', workspaces } });
    const toolbar = await screen.findByRole('toolbar', { name: '문서 동작' });
    expect(
      within(toolbar)
        .getAllByRole('button')
        .map((b) => b.textContent.trim()),
    ).toEqual(['공유·설정', '기록', '더 보기']);
    for (const name of ['복제', '이동', '내보내기', '홈에 고정', '☆ 즐겨찾기', '문서 지우기'])
      expect(screen.queryByRole('button', { name })).toBeNull();

    const menu = await openMore();
    expect(
      within(menu)
        .getAllByRole('menuitem')
        .map((i) => i.textContent.trim()),
    ).toEqual(['복제', '이동', '내보내기', '문서 지우기']);
    // 홈 표시는 서버가 돌려준 표시대로 체크된다.
    await waitFor(() => {
      expect(within(menu).getByRole('menuitemcheckbox', { name: '홈에 고정' })).toHaveAttribute(
        'aria-checked',
        'true',
      );
    });
    expect(within(menu).getByRole('menuitemcheckbox', { name: '즐겨찾기' })).toHaveAttribute(
      'aria-checked',
      'false',
    );
  });

  it('offers only the home marks to a reader who may not copy, export or delete', async () => {
    vi.spyOn(api, 'document').mockResolvedValue(
      documentView({
        access: 'read',
        can_edit: false,
        can_manage: false,
        allow_copy: false,
        can_export: false,
        can_delete: false,
      }),
    );
    render(DocumentScreen, { props: { documentId: 'doc-1', workspaces } });
    const menu = await openMore();
    expect(within(menu).queryAllByRole('menuitem')).toHaveLength(0);
    expect(within(menu).getAllByRole('menuitemcheckbox')).toHaveLength(2);
    expect(screen.queryByRole('button', { name: '공유·설정' })).toBeNull();
  });

  it('opens the export dialog from the menu', async () => {
    vi.spyOn(api, 'document').mockResolvedValue(documentView({}));
    render(DocumentScreen, { props: { documentId: 'doc-1', workspaces } });
    const menu = await openMore();
    await fireEvent.click(within(menu).getByRole('menuitem', { name: '내보내기' }));
    const dialog = await screen.findByRole('dialog');
    expect(within(dialog).getByRole('heading', { name: '내보내기' })).toBeInTheDocument();
  });

  it('asks before moving to trash and gives focus back to the menu button on cancel', async () => {
    vi.spyOn(api, 'document').mockResolvedValue(documentView({}));
    const trash = vi.spyOn(api, 'trashDocument').mockResolvedValue('batch-1');
    render(DocumentScreen, { props: { documentId: 'doc-1', workspaces } });
    const menu = await openMore();
    await fireEvent.click(within(menu).getByRole('menuitem', { name: '문서 지우기' }));

    expect(await screen.findByText('문서: 휴지통으로 옮길까요?')).toBeInTheDocument();
    await waitFor(() => {
      expect(screen.getByRole('button', { name: '휴지통으로' })).toHaveFocus();
    });
    await fireEvent.click(screen.getByRole('button', { name: '취소' }));
    await waitFor(() => {
      expect(screen.getByRole('button', { name: '더 보기' })).toHaveFocus();
    });
    expect(screen.queryByText('문서: 휴지통으로 옮길까요?')).toBeNull();
    expect(trash).not.toHaveBeenCalled();
  });
});

// 리뷰: 같은 화면에서 다른 문서로 옮기면 앞 문서에서 연 지우기 확인이 새 문서에 남아, 확인을 누르면 새 문서가 휴지통으로
// 갔다. 문서가 바뀌면 확인과 내보내기 창을 닫는다.
describe('document actions after switching documents', () => {
  it('does not carry an open delete question or export dialog over to the next document', async () => {
    vi.spyOn(api, 'document').mockImplementation((id: string) =>
      Promise.resolve(documentView({ id, title: id === 'doc-1' ? '운영 안내' : '설치' })),
    );
    const trash = vi.spyOn(api, 'trashDocument').mockResolvedValue('batch-1');
    const view = render(DocumentScreen, { props: { documentId: 'doc-1', workspaces } });
    const menu = await openMore();
    await fireEvent.click(within(menu).getByRole('menuitem', { name: '문서 지우기' }));
    expect(await screen.findByText('문서: 휴지통으로 옮길까요?')).toBeInTheDocument();

    await view.rerender({ documentId: 'doc-2', workspaces });
    await screen.findByRole('button', { name: '더 보기' });
    expect(screen.queryByText('문서: 휴지통으로 옮길까요?')).toBeNull();
    expect(screen.queryByRole('button', { name: '휴지통으로' })).toBeNull();
    expect(trash).not.toHaveBeenCalled();

    const again = await openMore();
    await fireEvent.click(within(again).getByRole('menuitem', { name: '내보내기' }));
    await screen.findByRole('dialog');
    await view.rerender({ documentId: 'doc-1', workspaces });
    await screen.findByRole('button', { name: '더 보기' });
    expect(screen.queryByRole('dialog')).toBeNull();
  });
});
