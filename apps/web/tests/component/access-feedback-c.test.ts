import { activityApi } from '$lib/features/activity/api';
import type { Workspace } from '@fvoci/api-client';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { api, ApiError } from '$lib/api/client';
import CommentsPanel from '$lib/features/comments/CommentsPanel.svelte';
import DocumentActions from '$lib/features/documents/DocumentActions.svelte';
import DocumentScreen from '$lib/features/documents/DocumentScreen.svelte';
import AttachmentsPanel from '$lib/features/files/AttachmentsPanel.svelte';
import { org } from '$lib/org.svelte';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';
import { ACCESS_DOCUMENT, ACCESS_FILE } from '../../stories/fixtures/access';
import { comment } from '../../stories/fixtures/comments';
import { TASK_ITEM, PROJECT_MODULE } from '../../stories/fixtures/synthetic';
import { TEST_ORGANIZATION } from './fixtures/organization';

const ACCESS_WORKSPACE: Workspace = {
  access: 'owner',
  id: 'ws-1',
  name: '운영',
  organization_id: TEST_ORGANIZATION.id,
  role: 'admin',
  is_owner: true,
  realtime_enabled: false,
};

beforeEach(async () => {
  me.userId = 'u1';
  await setLocale('ko', { reload: false });
});
afterEach(() => {
  vi.restoreAllMocks();
  me.userId = '';
});

describe('server access and archived comments (D-20·D-106·D-113)', () => {
  it.each(['document', 'task'] as const)(
    'shows the archived-parent notice for %s comments',
    async (kind) => {
      vi.spyOn(activityApi, 'comments').mockResolvedValue([]);
      vi.spyOn(api, 'item').mockResolvedValue({ ...TASK_ITEM, access: 'read' });
      vi.spyOn(api, 'module').mockResolvedValue({ ...PROJECT_MODULE, archived: true });
      render(CommentsPanel, {
        props: { target: { kind, id: 'target-1' }, canWrite: false, itemId: TASK_ITEM.id },
      });
      await screen.findByText('보관된 항목이라 쓸 수 없습니다.');
      expect(screen.queryByRole('textbox')).toBeNull();
    },
  );
  it.each([false, true])(
    'hides write controls and preserves own-comment deletion when archived=%s',
    async (archived) => {
      vi.spyOn(activityApi, 'comments').mockResolvedValue([
        comment({ can_edit: true, can_delete: true }),
      ]);
      render(CommentsPanel, {
        props: { target: { kind: 'document', id: 'd1' }, canWrite: false, archived },
      });
      await screen.findByRole('button', { name: '지우기' });
      expect(screen.queryByRole('textbox')).toBeNull();
      expect(screen.queryByRole('button', { name: '답글' })).toBeNull();
      expect(screen.queryByRole('button', { name: '고치기' })).toBeNull();
      if (archived) expect(screen.getByText('보관된 항목이라 쓸 수 없습니다.')).toBeInTheDocument();
      const start = screen.getByRole('button', { name: '지우기' });
      await fireEvent.click(start);
      await waitFor(() => {
        expect(screen.getByRole('button', { name: '지우기' })).toHaveFocus();
      });
      await fireEvent.click(screen.getByRole('button', { name: '취소' }));
      await waitFor(() => {
        expect(screen.getByRole('button', { name: '지우기' })).toHaveFocus();
      });
    },
  );

  it('keeps the input on an archived 409 and explains the archived state', async () => {
    vi.spyOn(activityApi, 'comments').mockResolvedValue([]);
    vi.spyOn(activityApi, 'addComment').mockRejectedValue(
      new ApiError(409, 'invalid_state', 'the item is archived'),
    );
    render(CommentsPanel, { props: { target: { kind: 'document', id: 'd1' }, canWrite: true } });
    const box = screen.getByRole('textbox');
    await fireEvent.input(box, { target: { value: '보존할 입력' } });
    await fireEvent.click(screen.getByRole('button', { name: '댓글 달기' }));
    await screen.findByText('보관된 항목이라 쓸 수 없습니다.');
    expect(box).toHaveValue('보존할 입력');
  });
});

describe('organization public links and allow_copy', () => {
  it.each([
    ['owner', false, true],
    ['write', false, false],
    ['read', false, false],
    ['write', true, true],
    ['read', true, true],
  ] as const)(
    'shows copy and export for access=%s, allow_copy=%s',
    async (access, allowCopy, allowed) => {
      vi.spyOn(api, 'document').mockResolvedValue({
        ...ACCESS_DOCUMENT,
        access,
        can_edit: access !== 'read',
        can_manage: access === 'owner',
        can_delete: access === 'owner',
        allow_copy: allowCopy,
      });
      vi.spyOn(api, 'documentFiles').mockResolvedValue([]);
      vi.spyOn(api, 'documentBacklinks').mockResolvedValue([]);
      vi.spyOn(activityApi, 'comments').mockResolvedValue([]);
      vi.spyOn(activityApi, 'mark').mockResolvedValue({ pin: false, favorite: false });
      render(DocumentScreen, { documentId: ACCESS_DOCUMENT.id, workspaces: [] });
      await fireEvent.click(await screen.findByRole('button', { name: '더 보기' }));
      await screen.findByRole('menu');
      expect(screen.queryByRole('menuitem', { name: '복제' }) !== null).toBe(allowed);
      expect(screen.queryByRole('menuitem', { name: /내보내기/ }) !== null).toBe(allowed);
    },
  );
  it.each([true, false])(
    'shows link creation only while public_links_enabled=%s',
    async (enabled) => {
      org.organizations = org.organizations.map((o) => ({ ...o, public_links_enabled: enabled }));
      vi.spyOn(api, 'shares').mockResolvedValue([]);
      vi.spyOn(api, 'publicLinks').mockResolvedValue([]);
      vi.spyOn(api, 'document').mockResolvedValue(ACCESS_DOCUMENT);
      render(DocumentActions, {
        doc: ACCESS_DOCUMENT,
        workspaces: [ACCESS_WORKSPACE],
        onchanged: () => undefined,
      });
      await fireEvent.click(screen.getByRole('button', { name: '공유·설정' }));
      await screen.findByTestId('public-link-list');
      expect(screen.queryByRole('button', { name: '공개 링크 만들기' }) !== null).toBe(enabled);
      if (!enabled) expect(screen.getByText('조직이 공개 링크를 껐습니다.')).toBeInTheDocument();
    },
  );

  // R5 P2 F-5·C-3: 공개 링크 정책은 지금 고른 조직이 아니라 문서가 속한 워크스페이스의 조직 것이다. 개인 노트에는 조직
  // 정책이 없다(SQL `app_create_public_link`는 workspace_id가 없는 문서에 조직 설정을 보지 않는다).
  it('applies the policy of the document organization and none to a personal note', async () => {
    const other = { ...TEST_ORGANIZATION, id: 'org-2', public_links_enabled: false };
    org.load('u1', [{ ...TEST_ORGANIZATION, public_links_enabled: false }, other], null);
    expect(org.currentId).toBeUndefined();
    vi.spyOn(api, 'shares').mockResolvedValue([]);
    vi.spyOn(api, 'publicLinks').mockResolvedValue([]);
    const personal = { ...ACCESS_DOCUMENT, workspace_id: null };
    const read = vi.spyOn(api, 'document').mockResolvedValue(personal);
    const view = render(DocumentActions, {
      doc: personal,
      workspaces: [ACCESS_WORKSPACE],
      onchanged: () => undefined,
    });
    await fireEvent.click(screen.getByRole('button', { name: '공유·설정' }));
    await screen.findByTestId('public-link-list');
    expect(screen.getByRole('button', { name: '공개 링크 만들기' })).toBeInTheDocument();
    expect(screen.queryByText('조직이 공개 링크를 껐습니다.')).toBeNull();
    view.unmount();

    // 문서의 조직(org-1)은 켰고 지금 고른 조직(org-2)은 껐다: 문서 조직을 따른다.
    org.load('u1', [TEST_ORGANIZATION, other], other.id);
    read.mockResolvedValue(ACCESS_DOCUMENT);
    render(DocumentActions, {
      doc: ACCESS_DOCUMENT,
      workspaces: [ACCESS_WORKSPACE],
      onchanged: () => undefined,
    });
    await fireEvent.click(screen.getByRole('button', { name: '공유·설정' }));
    await screen.findByTestId('public-link-list');
    expect(screen.getByRole('button', { name: '공개 링크 만들기' })).toBeInTheDocument();
    expect(screen.queryByText('조직이 공개 링크를 껐습니다.')).toBeNull();
  });

  // 서버 can_download(app_file_can_download)가 문서의 allow_copy를 이미 적용한다. 웹은 그 값만 따른다(D-113).
  it.each([true, false])(
    'gates downloads, including old versions, with the server can_download=%s',
    async (allowCopy) => {
      vi.spyOn(api, 'documentFiles').mockResolvedValue([
        { ...ACCESS_FILE, can_download: allowCopy },
      ]);
      render(AttachmentsPanel, {
        props: {
          target: { kind: 'document', id: ACCESS_DOCUMENT.id },
          canWrite: true,
        },
      });
      await screen.findByText(ACCESS_FILE.name);
      expect(screen.queryByRole('link', { name: ACCESS_FILE.name }) !== null).toBe(allowCopy);
      // 미리보기 권한과 다운로드 권한을 섞지 않는다.
      expect(screen.getByRole('link', { name: /미리 보기|미리보기/ })).toBeInTheDocument();
      // 옛 판도 같은 규칙이다: 버전 기록을 열어 v1 내려받기 링크를 본다.
      const version = {
        size: 512,
        sha256: '0'.repeat(64),
        content_type: 'application/pdf',
        note: '',
        uploaded_by: 'u1',
        uploaded_by_name: '가람',
        created_at: '2026-10-25T00:00:00Z',
      };
      vi.spyOn(api, 'file').mockResolvedValue({
        file: { ...ACCESS_FILE, current_version: 2, can_download: allowCopy },
        versions: [
          { ...version, version: 2 },
          { ...version, version: 1 },
        ],
      });
      await fireEvent.click(
        screen.getByRole('button', { name: m.files_versions_aria({ name: ACCESS_FILE.name }) }),
      );
      const list = await screen.findByRole('list', {
        name: m.files_version_list_aria({ name: ACCESS_FILE.name }),
      });
      expect(within(list).getByText('v1')).toBeInTheDocument();
      expect(within(list).queryByRole('link', { name: 'v1' }) !== null).toBe(allowCopy);
    },
  );
});
