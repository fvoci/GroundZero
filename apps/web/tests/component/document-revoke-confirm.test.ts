import type { Workspace } from '@fvoci/api-client';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { DocumentView } from '$lib/api/client';
import { api } from '$lib/api/client';
import DocumentActions from '$lib/features/documents/DocumentActions.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

// 공유 철회와 공개 링크 철회는 한 번에 실행됐다(UI 감사). 같은 화면의 다른 지우기처럼 한 번 더 묻고, 그만두면 처음 누른
// 단추로 초점을 돌려준다.
const workspaces: Workspace[] = [
  { id: 'ws-1', name: '공간', role: 'admin', is_owner: true, realtime_enabled: false },
];

const doc: DocumentView = {
  id: 'd1',
  workspace_id: 'ws-1',
  placement: 'wiki',
  item_id: null,
  parent_id: null,
  reference: 'WIKI-1',
  title: '문서',
  body: { type: 'doc' },
  schema_version: 1,
  revision: 3,
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

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  vi.spyOn(api, 'document').mockResolvedValue(doc);
  vi.spyOn(api, 'shares').mockResolvedValue([
    { id: 's1', email: 'second@example.test', permission: 'read' },
  ]);
  vi.spyOn(api, 'publicLinks').mockResolvedValue([
    { id: 'l1', created_at: '2026-10-20T00:00:00Z', expires_at: null, active: true },
  ]);
});
afterEach(() => {
  vi.restoreAllMocks();
});

async function openShare(): Promise<HTMLElement> {
  render(DocumentActions, { props: { doc, workspaces, onchanged: () => undefined } });
  await fireEvent.click(screen.getByRole('button', { name: '공유·설정' }));
  return screen.findByRole('dialog');
}

describe('revoking access asks first', () => {
  it('asks before revoking a share and gives focus back on cancel', async () => {
    const unshare = vi.spyOn(api, 'unshare').mockResolvedValue(undefined);
    const dialog = await openShare();
    const list = await within(dialog).findByTestId('share-list');
    const start = await within(list).findByRole('button', { name: '철회' });
    await fireEvent.click(start);

    expect(within(list).getByText('second@example.test의 접근을 철회할까요?')).toBeInTheDocument();
    expect(unshare).not.toHaveBeenCalled();
    const confirm = within(list).getByRole('button', { name: '철회' });
    await waitFor(() => {
      expect(confirm).toHaveFocus();
    });

    await fireEvent.click(within(list).getByRole('button', { name: '취소' }));
    await waitFor(() => {
      expect(within(list).getByRole('button', { name: '철회' })).toHaveFocus();
    });
    expect(unshare).not.toHaveBeenCalled();

    await fireEvent.click(within(list).getByRole('button', { name: '철회' }));
    await fireEvent.click(within(list).getByRole('button', { name: '철회' }));
    await waitFor(() => {
      expect(unshare).toHaveBeenCalledWith('s1');
    });
  });

  it('asks before revoking a public link', async () => {
    const revoke = vi.spyOn(api, 'revokePublicLink').mockResolvedValue(undefined);
    const dialog = await openShare();
    const links = await within(dialog).findByTestId('public-link-list');
    await fireEvent.click(await within(links).findByRole('button', { name: '철회' }));
    expect(within(links).getByText(/이 링크를 철회할까요\?/)).toBeInTheDocument();
    expect(revoke).not.toHaveBeenCalled();
    await fireEvent.click(within(links).getByRole('button', { name: '철회' }));
    await waitFor(() => {
      expect(revoke).toHaveBeenCalledWith('l1');
    });
  });
});
