import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { DocNode, DocumentView } from '$lib/api/client';
import { api } from '$lib/api/client';
import DocumentEditor from '$lib/features/documents/DocumentEditor.svelte';
import { loadDraft, loadOffers, queueDraft, storeDraft } from '$lib/features/documents/drafts';
import { me } from '$lib/me.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';
import { router } from '$lib/router.svelte';

function paragraph(text: string): DocNode {
  return { type: 'doc', content: [{ type: 'paragraph', content: [{ type: 'text', text }] }] };
}

function documentAt(revision: number, title: string, body: DocNode): DocumentView {
  return {
    id: 'doc-1',
    workspace_id: 'ws-1',
    placement: 'wiki',
    item_id: null,
    parent_id: null,
    reference: 'WIKI-1',
    title,
    body,
    schema_version: 1,
    revision,
    updated_at: '2026-10-19T00:00:00Z',
    access: 'owner',
    can_edit: true,
    visibility: 'workspace',
    allow_copy: true,
    can_manage: true,
    can_delete: true,
    is_template: false,
    realtime: false,
    can_export: true,
  };
}

function titleInput(): HTMLInputElement {
  const input = document.querySelector('#doc-title');
  if (!(input instanceof HTMLInputElement)) throw new Error('title input');
  return input;
}

/** 이 문서에 대해 이 브라우저에 남은 초안들의 제목(지금 편집의 초안과 고르게 할 줄 모두). */
function keptTitles(): string[] {
  return [loadDraft('doc-1'), ...loadOffers('doc-1')].flatMap((d) =>
    d === undefined ? [] : [d.title],
  );
}

const serverBody = paragraph('서버 본문');

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  // jsdom에는 배치 계산이 없어 편집기 메뉴 위치 계산이 쓰는 값을 빈 사각형으로 채운다.
  Range.prototype.getBoundingClientRect = (): DOMRect => new DOMRect(0, 0, 0, 0);
  const none: DOMRectList = Object.assign([], { item: (): DOMRect | null => null });
  Range.prototype.getClientRects = (): DOMRectList => none;
  Element.prototype.scrollIntoView = (): void => undefined;
  me.userId = 'user-a';
  // 같은 탭에서 다시 열었다(새로고침·앱 안 이동): 지난번 편집이 이 탭의 초안으로 남아 있다.
  storeDraft('doc-1', {
    baseRevision: 5,
    title: '지난번 초안',
    body: paragraph('지난번 본문'),
    savedAt: '2026-10-19T01:00:00Z',
  });
});

afterEach(() => {
  vi.restoreAllMocks();
  window.localStorage.clear();
  me.userId = '';
  router.guard = undefined;
});

// 감사 W7: "저장하지 않은 초안이 있습니다"에 답하기 전에 입력해도 그 초안을 덮거나 지우지 않는다. 이 탭의 지난 초안은
// 입력이 쓰는 키와 같은 키에 있어, 한 글자 입력이 그것을 덮고 이어진 자동 저장이 지웠다.
describe('typing before answering the draft offer', () => {
  it('keeps the offered draft through a successful save and offers it again on reopen', async () => {
    const save = vi
      .spyOn(api, 'saveDocument')
      .mockImplementation((_id, input) =>
        Promise.resolve({ kind: 'saved', document: documentAt(6, input.title, input.body) }),
      );
    const first = render(DocumentEditor, {
      props: { initial: documentAt(5, '제목', serverBody) },
    });
    expect(screen.getByRole('region', { name: '보관된 초안' })).toBeInTheDocument();

    await fireEvent.input(titleInput(), { target: { value: '제목!' } });
    await fireEvent.click(screen.getByRole('button', { name: '저장' }));
    await waitFor(() => {
      expect(screen.getByTestId('save-status').textContent).toBe('저장됨');
    });
    expect(save).toHaveBeenCalledTimes(1);
    // 아직 고르지 않았다: 제안은 화면에 있고, 초안도 이 브라우저에 그대로 있어야 한다.
    expect(screen.getByRole('region', { name: '보관된 초안' })).toBeInTheDocument();
    expect(keptTitles()).toContain('지난번 초안');
    first.unmount();

    // 고르지 않고 떠났다가 다시 열어도 그 초안을 다시 고르게 하고, 되살릴 수 있다.
    render(DocumentEditor, { props: { initial: documentAt(6, '제목!', serverBody) } });
    await fireEvent.click(screen.getByRole('button', { name: '초안 복원' }));
    expect(titleInput().value).toBe('지난번 초안');
  });

  it('discarding the offer does not drop the edits typed meanwhile', async () => {
    // 저장은 끝나지 않는다(느린 네트워크): 새 입력은 이 브라우저의 초안에만 있다.
    vi.spyOn(api, 'saveDocument').mockReturnValue(new Promise(() => undefined));
    render(DocumentEditor, { props: { initial: documentAt(5, '제목', serverBody) } });

    await fireEvent.input(titleInput(), { target: { value: '새로 고친 제목' } });
    await fireEvent.click(screen.getByRole('button', { name: '초안 버리기' }));

    expect(keptTitles()).toEqual(['새로 고친 제목']);
  });

  it('leaves the draft in place when it cannot be queued (storage full)', async () => {
    // 줄로 옮길 자리가 없다: 지난 초안은 이 탭의 키에 둔 채 고르게 하고, 그동안 그 키를 덮거나 지우지 않는다.
    const setItem = window.localStorage.setItem.bind(window.localStorage);
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation((key: string, value: string) => {
      if (key.includes(':offer:')) throw new DOMException('full', 'QuotaExceededError');
      setItem(key, value);
    });
    vi.spyOn(api, 'saveDocument').mockImplementation((_id, input) =>
      Promise.resolve({ kind: 'saved', document: documentAt(6, input.title, input.body) }),
    );
    render(DocumentEditor, { props: { initial: documentAt(5, '제목', serverBody) } });
    expect(screen.getByRole('region', { name: '보관된 초안' })).toBeInTheDocument();

    await fireEvent.input(titleInput(), { target: { value: '제목!' } });
    await fireEvent.click(screen.getByRole('button', { name: '저장' }));
    await waitFor(() => {
      expect(screen.getByTestId('save-status').textContent).toBe('저장됨');
    });
    expect(keptTitles()).toEqual(['지난번 초안']);

    // 버리면 그 자리에 지금 편집이 남는다(여기서는 이미 저장되어 남길 것이 없다).
    await fireEvent.click(screen.getByRole('button', { name: '초안 버리기' }));
    expect(keptTitles()).toEqual([]);
  });
});

// 고르게 할 초안이 둘 이상이면 하나를 되살리거나 버린 뒤 다음 것을 바로 고르게 한다(다시 열 때까지 숨지 않는다).
describe('more than one draft to offer', () => {
  beforeEach(() => {
    // 고르게 할 줄에 초안이 둘 있다(지난번 초안, 그 뒤의 두 번째 초안). 이 탭의 키에는 없다.
    queueDraft('doc-1');
    storeDraft('doc-1', {
      baseRevision: 5,
      title: '두 번째 초안',
      body: paragraph('두 번째 본문'),
      savedAt: '2026-10-19T02:00:00Z',
    });
    queueDraft('doc-1');
    vi.spyOn(api, 'saveDocument').mockReturnValue(new Promise(() => undefined));
  });

  it('offers the next draft right after discarding one', async () => {
    render(DocumentEditor, { props: { initial: documentAt(5, '제목', serverBody) } });
    await fireEvent.click(screen.getByRole('button', { name: '초안 버리기' }));

    expect(screen.getByRole('region', { name: '보관된 초안' })).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: '초안 복원' }));
    expect(titleInput().value).toBe('두 번째 초안');
    expect(screen.queryByRole('region', { name: '보관된 초안' })).toBeNull();
  });

  it('offers the next draft right after restoring one', async () => {
    render(DocumentEditor, { props: { initial: documentAt(5, '제목', serverBody) } });
    await fireEvent.click(screen.getByRole('button', { name: '초안 복원' }));
    expect(titleInput().value).toBe('지난번 초안');

    expect(screen.getByRole('region', { name: '보관된 초안' })).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: '초안 버리기' }));
    expect(screen.queryByRole('region', { name: '보관된 초안' })).toBeNull();
    // 되살린 초안은 지금 편집으로 남아 있다.
    expect(keptTitles()).toEqual(['지난번 초안']);
  });

  it('offers the next draft, not the restored one again, when it could not leave the queue', async () => {
    // 지금 편집의 키에 쓸 자리가 없다: 되살린 초안은 줄에 남지만(사라지지 않게) 곧바로 다시 고르게 하지 않고 다음 것을
    // 고르게 한다.
    const setItem = window.localStorage.setItem.bind(window.localStorage);
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation((key: string, value: string) => {
      if (key.includes(':tab:')) throw new DOMException('full', 'QuotaExceededError');
      setItem(key, value);
    });
    render(DocumentEditor, { props: { initial: documentAt(5, '제목', serverBody) } });
    await fireEvent.click(screen.getByRole('button', { name: '초안 복원' }));
    expect(titleInput().value).toBe('지난번 초안');

    await fireEvent.click(screen.getByRole('button', { name: '초안 복원' }));
    expect(titleInput().value).toBe('두 번째 초안');
  });
});
