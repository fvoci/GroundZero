import type { Workspace } from '@fvoci/api-client';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { api, ApiError } from '$lib/api/client';
import WorkspaceScreen from '$lib/features/workspaces/WorkspaceScreen.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

const LABEL = '실시간 공동 편집(문서를 여럿이 동시에 고침)';

function workspaceWith(realtime: boolean): Workspace {
  return { id: 'ws-1', name: '공간', role: 'admin', is_owner: true, realtime_enabled: realtime };
}

function checkbox(): HTMLInputElement {
  const box = screen.getByRole('checkbox', { name: LABEL });
  if (!(box instanceof HTMLInputElement)) throw new Error('checkbox');
  return box;
}

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  vi.spyOn(api, 'wikiDocuments').mockResolvedValue([]);
  vi.spyOn(api, 'modules').mockResolvedValue([]);
  vi.spyOn(api, 'documentTemplates').mockResolvedValue([]);
});

afterEach(() => {
  vi.restoreAllMocks();
});

// 감사 W10: 워크스페이스의 "실시간 공동 편집" 칸이 서버 상태와 어긋나지 않는다.
describe('workspace realtime checkbox', () => {
  it('goes back to the server state when turning it off fails', async () => {
    const set = vi
      .spyOn(api, 'setWorkspaceRealtime')
      .mockRejectedValue(
        new ApiError(409, 'conflict', 'some live documents could not be saved as revisions'),
      );
    render(WorkspaceScreen, { props: { workspace: workspaceWith(true) } });
    expect(checkbox().checked).toBe(true);

    await fireEvent.click(checkbox());
    await waitFor(() => {
      expect(set).toHaveBeenCalledWith('ws-1', false);
    });
    await screen.findByText(/^바꾸지 못했습니다\./);
    // 실패했으니 실시간은 여전히 켜져 있다. 칸도 켜진 채로 보인다.
    await waitFor(() => {
      expect(checkbox().checked).toBe(true);
    });
  });

  it('goes back to off when turning it on fails', async () => {
    vi.spyOn(api, 'setWorkspaceRealtime').mockRejectedValue(
      new ApiError(403, 'forbidden', 'forbidden'),
    );
    render(WorkspaceScreen, { props: { workspace: workspaceWith(false) } });

    await fireEvent.click(checkbox());
    await waitFor(() => {
      expect(checkbox().checked).toBe(false);
    });
  });

  it('reports a successful change so the shared workspace list (and the next visit) shows it', async () => {
    vi.spyOn(api, 'setWorkspaceRealtime').mockResolvedValue(undefined);
    const changed = vi.fn<(workspaceId: string, enabled: boolean) => void>();
    const view = render(WorkspaceScreen, {
      props: { workspace: workspaceWith(false), onrealtimechanged: changed },
    });

    await fireEvent.click(checkbox());
    await waitFor(() => {
      expect(changed).toHaveBeenCalledWith('ws-1', true);
    });
    expect(
      screen.getByText('실시간 공동 편집을 켰습니다. 문서를 다시 열면 함께 고칠 수 있습니다.'),
    ).toBeInTheDocument();

    // 앱이 목록을 고쳐 새 값을 넘겨도 방금 보인 안내와 칸은 그대로다.
    await view.rerender({ workspace: workspaceWith(true), onrealtimechanged: changed });
    expect(checkbox().checked).toBe(true);
    expect(
      screen.getByText('실시간 공동 편집을 켰습니다. 문서를 다시 열면 함께 고칠 수 있습니다.'),
    ).toBeInTheDocument();
  });
});
