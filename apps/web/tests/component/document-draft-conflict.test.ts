import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { DocNode, DocumentView } from '$lib/api/client';
import { api } from '$lib/api/client';
import DocumentEditor from '$lib/features/documents/DocumentEditor.svelte';
import { loadDraft } from '$lib/features/documents/drafts';
import { me } from '$lib/me.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';
import { router } from '$lib/router.svelte';

function paragraph(text: string): DocNode {
  return { type: 'doc', content: [{ type: 'paragraph', content: [{ type: 'text', text }] }] };
}

function documentAt(revision: number, title: string, text: string): DocumentView {
  return {
    id: 'doc-1',
    workspace_id: 'ws-1',
    placement: 'wiki',
    item_id: null,
    parent_id: null,
    reference: 'WIKI-1',
    title,
    body: paragraph(text),
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

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  // jsdom에는 배치 계산이 없어 편집기 메뉴 위치 계산이 쓰는 값을 빈 사각형으로 채운다.
  Range.prototype.getBoundingClientRect = (): DOMRect => new DOMRect(0, 0, 0, 0);
  const none: DOMRectList = Object.assign([], { item: (): DOMRect | null => null });
  Range.prototype.getClientRects = (): DOMRectList => none;
  Element.prototype.scrollIntoView = (): void => undefined;
  me.userId = 'user-a';
  // 저장은 보내기 전에 이 창의 계정이 아직 로그인돼 있는지 묻는다.
  vi.spyOn(api, 'me').mockResolvedValue({
    id: 'user-a',
    display_name: '나',
    hour_cycle: '',
    locale: 'ko',
    time_zone: '',
    week_start: '',
    theme: 'system',
  });
});

afterEach(() => {
  vi.restoreAllMocks();
  window.localStorage.clear();
  me.userId = '';
  router.guard = undefined;
});

// 감사 W9: 충돌 화면이 열린 동안 고친 내용도 이 브라우저의 초안에 남는다. 떠날 때 "초안은 이 브라우저에 보관됩니다"라고
// 묻는데, 충돌 뒤의 입력이 초안에 쓰이지 않으면 떠나거나 새로고침할 때 그 입력이 사라진다.
describe('editing while the conflict panel is open', () => {
  it('keeps writing the local draft after a 409', async () => {
    const save = vi.spyOn(api, 'saveDocument').mockResolvedValue({
      kind: 'conflict',
      current: documentAt(6, '다른 사람 제목', '다른 사람 본문'),
    });
    render(DocumentEditor, { props: { initial: documentAt(5, '제목', '본문') } });

    await fireEvent.input(titleInput(), { target: { value: '충돌 전 제목' } });
    await fireEvent.click(screen.getByRole('button', { name: '저장' }));
    await screen.findByRole('heading', { name: '다른 곳에서 먼저 저장됐습니다' });
    expect(loadDraft('doc-1')?.title).toBe('충돌 전 제목');

    // 비교를 읽으며 계속 고친다.
    await fireEvent.input(titleInput(), { target: { value: '충돌 뒤에 더 고친 제목' } });
    await waitFor(() => {
      expect(loadDraft('doc-1')?.title).toBe('충돌 뒤에 더 고친 제목');
    });
    // 충돌은 사용자가 고를 때까지 그대로다(저절로 다시 저장하지 않는다).
    expect(screen.getByTestId('save-status').textContent).toBe('충돌 — 선택 필요');
    expect(save).toHaveBeenCalledTimes(1);
  });
});
