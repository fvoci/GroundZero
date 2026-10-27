import type { Workspace } from '@fvoci/api-client';
import { fireEvent, render, screen, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { DocumentView } from '$lib/api/client';
import { api } from '$lib/api/client';
import DocumentActions from '$lib/features/documents/DocumentActions.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

// ⑯(D-113 개정 2026-10-26): 보관된 항목에서 워크스페이스 관리자·소유자는 공유·공개 링크·설정을 보관 해제 없이 바꿀 수
// 있다. 서버는 보관 항목 문서의 등급을 read로 낮추지만 `app_document_can_manage`가 워크스페이스 관리자에게 참을 주고,
// 공유·설정·공개 링크 SQL은 그 판단(`app_require_document_manager`)만 본다(노출을 넓히는 변경도 막지 않는다). 화면은
// 노출을 줄이는 변경만 허용하던 제한을 없애고 `can_manage`를 따른다. 템플릿 지정은 SQL이 owner 등급만 허용해 그대로다.

const workspaces: Workspace[] = [
  {
    access: 'owner',
    id: 'ws-1',
    name: '공간',
    organization_id: 'org-1',
    role: 'admin',
    is_owner: false,
    realtime_enabled: false,
  },
];

const archivedForAdmin: DocumentView = {
  id: 'doc-1',
  workspace_id: 'ws-1',
  placement: 'item',
  item_id: 'item-1',
  parent_id: null,
  reference: 'DEV-1',
  title: '보관된 항목의 문서',
  body: { type: 'doc', content: [{ type: 'paragraph' }] },
  schema_version: 1,
  revision: 1,
  updated_at: '2026-10-21T00:00:00Z',
  access: 'read',
  can_edit: false,
  visibility: 'private',
  allow_copy: false,
  can_manage: true,
  can_delete: false,
  is_template: false,
  realtime: false,
  can_export: true,
};

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  vi.spyOn(api, 'shares').mockResolvedValue([]);
  vi.spyOn(api, 'publicLinks').mockResolvedValue([]);
  vi.spyOn(api, 'document').mockResolvedValue(archivedForAdmin);
});
afterEach(() => {
  vi.restoreAllMocks();
  window.localStorage.clear();
});

describe('sharing settings of a document under an archived item', () => {
  it('lets a workspace admin widen sharing, settings and public links without unarchiving', async () => {
    const settings = vi.spyOn(api, 'setSettings').mockResolvedValue(archivedForAdmin);
    render(DocumentActions, {
      props: { doc: archivedForAdmin, workspaces, onchanged: () => undefined },
    });
    await fireEvent.click(screen.getByRole('button', { name: '공유·설정' }));
    const dialog = await screen.findByRole('dialog');
    expect(within(dialog).getByLabelText('이메일')).toBeInTheDocument();
    expect(within(dialog).getByRole('button', { name: '공개 링크 만들기' })).toBeInTheDocument();
    const privateBox = within(dialog).getByRole('checkbox', { name: /비공개/ });
    const copyBox = within(dialog).getByRole('checkbox', { name: '볼 수 있는 사람의 복제 허용' });
    expect(privateBox).toBeEnabled();
    expect(copyBox).toBeEnabled();
    // 템플릿 지정은 owner 등급만(SQL `app_set_document_template`).
    expect(within(dialog).queryByRole('checkbox', { name: /템플릿/ })).toBeNull();

    // 비공개를 풀어 공간에 공개한다(노출을 넓히는 변경).
    await fireEvent.click(privateBox);
    expect(settings).toHaveBeenCalledWith('doc-1', { visibility: 'workspace' });
  });
});
