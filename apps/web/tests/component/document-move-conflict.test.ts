import type { Workspace } from '@fvoci/api-client';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { DocumentView, MovePreviewView } from '$lib/api/client';
import { api, ApiError } from '$lib/api/client';
import DocumentActions from '$lib/features/documents/DocumentActions.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';
import { router, workspacePath } from '$lib/router.svelte';

// 이동의 409는 두 모양이다(계약 `MoveConflict`): 확인한 접근 변화가 그사이 바뀌면 MovePreview(새 미리보기),
// 실시간 방의 편집을 먼저 리비전으로 남기지 못하면 ErrorBody(`conflict`, 옮기지 않음). 클라이언트는 앞의 것만
// 다시 확인으로 돌리고 뒤의 것은 일반 오류로 알린다. 화면은 미리보기와 창을 그대로 두어 다시 누르면 된다(감사 C26).
const preview: MovePreviewView = {
  changes: [
    { change: 'gain', user_id: 'u-new', display_name: '새 팀', from_grade: null, to_grade: 'read' },
    { change: 'lose', user_id: 'u-old', display_name: '옛 팀', from_grade: 'read', to_grade: null },
  ],
  history_moves_with_document: true,
  reference_changes: true,
  active_public_links: 0,
  children_left_behind: 0,
};
const busy = {
  error: 'conflict',
  message: 'the live editing room is busy; nothing was moved, try again',
  request_id: 'r1',
};

function response(status: number, json: object): Response {
  return new Response(JSON.stringify(json), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });
}

function reply(status: number, json: object): void {
  vi.stubGlobal('fetch', () => Promise.resolve(response(status, json)));
}

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe('move conflict responses', () => {
  it('turns changed access (MovePreview) into a changed result with the new preview', async () => {
    const more = [
      ...preview.changes,
      {
        change: 'gain',
        user_id: 'u-more',
        display_name: '다른 팀',
        from_grade: null,
        to_grade: 'read',
      },
    ];
    reply(409, { ...preview, changes: more });
    const result = await documentsApi.moveDocument(
      'd1',
      { workspaceId: 'ws-2', itemId: null },
      preview,
    );
    expect(result.kind).toBe('changed');
    expect(result.kind === 'changed' ? result.preview : undefined).toEqual({
      ...preview,
      changes: more,
    });
  });

  it('reports a busy live room (ErrorBody) as an ordinary 409 error, not a changed preview', async () => {
    reply(409, busy);
    const failed = await documentsApi
      .moveDocument('d1', { workspaceId: 'ws-2', itemId: null }, preview)
      .then(
        (r: unknown) => r,
        (e: unknown) => e,
      );
    expect(failed).toBeInstanceOf(ApiError);
    expect(
      failed instanceof ApiError ? [failed.status, failed.code, failed.requestId] : [],
    ).toEqual([409, 'conflict', 'r1']);
  });
});

const doc: DocumentView = {
  id: 'd1',
  workspace_id: 'ws-1',
  placement: 'wiki',
  item_id: null,
  parent_id: null,
  reference: 'WIKI-1',
  title: '운영 안내',
  body: { type: 'doc', content: [] },
  schema_version: 1,
  revision: 3,
  updated_at: '2026-10-21T00:00:00Z',
  access: 'owner',
  can_edit: true,
  visibility: 'workspace',
  allow_copy: false,
  can_manage: true,
  can_delete: true,
  is_template: false,
  can_export: true,
};
// 문서가 있는 ws-1의 위키는 대상에서 빠지므로 옮길 곳은 ws-2 위키 하나다.
const workspaces: Workspace[] = [
  {
    access: 'write',
    id: 'ws-1',
    name: '운영',
    organization_id: 'org-1',
    role: 'member',
    is_owner: false,
  },
  {
    access: 'write',
    id: 'ws-2',
    name: '개발',
    organization_id: 'org-1',
    role: 'member',
    is_owner: false,
  },
];

describe('move preview history notice', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
  });

  // SR1-15(D-22·D-24): 옮기면 리비전뿐 아니라 댓글·첨부(이전 버전 포함)도 새로 볼 수 있게 되는 사람에게 보인다.
  it('tells that revisions, comments and attachments move with the document', async () => {
    vi.spyOn(api, 'modules').mockResolvedValue([]);
    vi.spyOn(documentsApi, 'movePreview').mockResolvedValue(preview);
    render(DocumentActions, { props: { doc, workspaces, onchanged: () => undefined } });
    await fireEvent.click(screen.getByRole('button', { name: '더 보기' }));
    await fireEvent.click(await screen.findByRole('menuitem', { name: '이동' }));
    const dialog = await screen.findByRole('dialog');
    await within(dialog).findByTestId('move-preview');
    const notice = within(dialog).getByText(/리비전/);
    for (const part of ['댓글', '첨부', '이전 버전']) {
      expect(notice.textContent).toContain(part);
    }
  });
});

describe('move panel when the live room is busy', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
  });

  // R5 P2 F-4(D-113): 옮길 곳은 서버 접근 등급(`Workspace.access`)으로 고른다. 역할 이름이 관리자여도 읽기 등급이면 뺀다.
  it('offers only workspaces whose server access is not read as move targets', async () => {
    const modules = vi.spyOn(api, 'modules').mockResolvedValue([]);
    vi.spyOn(documentsApi, 'movePreview').mockResolvedValue(preview);
    const readOnlyAdmin: Workspace = {
      access: 'read',
      id: 'ws-3',
      name: '보관 운영',
      organization_id: 'org-1',
      role: 'admin',
      is_owner: false,
    };
    render(DocumentActions, {
      props: { doc, workspaces: [...workspaces, readOnlyAdmin], onchanged: () => undefined },
    });
    await fireEvent.click(screen.getByRole('button', { name: '더 보기' }));
    await fireEvent.click(await screen.findByRole('menuitem', { name: '이동' }));
    const dialog = await screen.findByRole('dialog');
    await within(dialog).findByTestId('move-preview');
    expect(modules.mock.calls.map((call) => call[0]).sort()).toEqual(['ws-1', 'ws-2']);
  });

  it('shows the error, stays on the page and keeps the preview so the move can be retried', async () => {
    vi.spyOn(api, 'modules').mockResolvedValue([]);
    vi.spyOn(documentsApi, 'movePreview').mockResolvedValue(preview);
    const navigate = vi.spyOn(router, 'navigate').mockImplementation(() => undefined);
    const bodies: unknown[] = [];
    const answers = [
      response(409, busy),
      response(200, { workspace_id: 'ws-2', reference: 'WIKI-9', document: null }),
    ];
    vi.stubGlobal('fetch', (_path: string, init: RequestInit) => {
      bodies.push(typeof init.body === 'string' ? JSON.parse(init.body) : undefined);
      const next = answers.shift();
      return next === undefined
        ? Promise.reject(new Error('no more answers'))
        : Promise.resolve(next);
    });
    render(DocumentActions, { props: { doc, workspaces, onchanged: () => undefined } });

    // 이동은 "더 보기" 메뉴 안에 있다.
    await fireEvent.click(screen.getByRole('button', { name: '더 보기' }));
    await fireEvent.click(await screen.findByRole('menuitem', { name: '이동' }));
    const dialog = await screen.findByRole('dialog');
    await within(dialog).findByTestId('move-preview');
    const move = within(dialog).getByRole('button', { name: '이동' });
    await fireEvent.click(move);

    // 바쁜 방은 접근 변화가 바뀐 것이 아니다. 다시 확인하라는 안내가 아니라 실패를 알린다.
    const alert = await within(dialog).findByRole('alert');
    expect(alert.textContent.trim()).toBe('요청을 처리하지 못했습니다.');
    expect(navigate).not.toHaveBeenCalled();
    expect(screen.getByRole('dialog')).toBe(dialog);
    expect(within(dialog).getByTestId('move-preview').textContent).toContain('새 팀');
    expect(move).toBeEnabled();

    // 같은 미리보기로 다시 누르면 옮긴다(더 이상 볼 수 없는 자리라 그 워크스페이스로 간다).
    await fireEvent.click(move);
    await waitFor(() => {
      expect(navigate).toHaveBeenCalledWith(workspacePath('ws-2'));
    });
    const acknowledged = { changes: preview.changes };
    expect(bodies).toEqual([
      { target_workspace_id: 'ws-2', target_item_id: null, acknowledged },
      { target_workspace_id: 'ws-2', target_item_id: null, acknowledged },
    ]);
  });
});
import { documentsApi } from '$lib/features/documents/api';
