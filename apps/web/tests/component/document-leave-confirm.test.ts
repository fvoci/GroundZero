import type { Workspace } from '@fvoci/api-client';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { DocumentView } from '$lib/api/client';
import { api } from '$lib/api/client';
import DocumentScreen from '$lib/features/documents/DocumentScreen.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';
import { router } from '$lib/router.svelte';

// 저장하지 않은 문서에서 다른 화면으로 가려 하면 브라우저 기본 확인창(window.confirm)이 떴다(UI 감사). 업무·일정
// 대화상자와 같은 확인(DiscardChangesConfirm)을 쓰고, 계속 편집하면 입력을 그대로 두며, 떠나기로 하면 같은 이동을 한다.
const workspaces: Workspace[] = [
  {
    id: 'ws-1',
    organization_id: 'org-1',
    name: '공간',
    role: 'member',
    access: 'write',
    is_owner: false,
    realtime_enabled: false,
  },
];

const doc: DocumentView = {
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
  access: 'write',
  can_edit: true,
  visibility: 'workspace',
  allow_copy: true,
  can_manage: false,
  can_delete: false,
  is_template: false,
  realtime: false,
  can_export: false,
};

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  Range.prototype.getBoundingClientRect = (): DOMRect => new DOMRect(0, 0, 0, 0);
  const none: DOMRectList = Object.assign([], { item: (): DOMRect | null => null });
  Range.prototype.getClientRects = (): DOMRectList => none;
  Element.prototype.scrollIntoView = (): void => undefined;
  vi.spyOn(api, 'document').mockResolvedValue(doc);
  vi.spyOn(api, 'mark').mockResolvedValue({ pin: false, favorite: false });
  vi.spyOn(api, 'comments').mockResolvedValue([]);
  vi.spyOn(api, 'documentFiles').mockResolvedValue([]);
  vi.spyOn(api, 'documentBacklinks').mockResolvedValue([]);
  vi.spyOn(api, 'wikiDocuments').mockResolvedValue([]);
  vi.stubGlobal('confirm', () => {
    throw new Error('window.confirm must not be used');
  });
  router.navigate('/w/ws-1/wiki/doc-1');
});
afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  router.guard = undefined;
  router.navigate('/');
  window.localStorage.clear();
});

describe('leaving a document with unsaved changes', () => {
  it('asks in an in-app dialog, keeps the input on keep, and goes on leave', async () => {
    render(DocumentScreen, { props: { documentId: 'doc-1', workspaces } });
    const title = await screen.findByRole('textbox', { name: '제목' });
    await fireEvent.input(title, { target: { value: '운영 안내 고침' } });

    router.navigate('/notes');
    const ask = await screen.findByRole('alertdialog');
    expect(router.path).toBe('/w/ws-1/wiki/doc-1');
    expect(
      within(ask).getByText(/저장하지 않은 변경이 있습니다\. 이동할까요\?/),
    ).toBeInTheDocument();

    await fireEvent.click(within(ask).getByRole('button', { name: '계속 편집' }));
    await waitFor(() => {
      expect(screen.queryByRole('alertdialog')).toBeNull();
    });
    expect(screen.getByRole('textbox', { name: '제목' })).toHaveValue('운영 안내 고침');
    expect(router.path).toBe('/w/ws-1/wiki/doc-1');

    router.navigate('/notes');
    const again = await screen.findByRole('alertdialog');
    await fireEvent.click(within(again).getByRole('button', { name: '이동하기' }));
    expect(router.path).toBe('/notes');
  });
});
