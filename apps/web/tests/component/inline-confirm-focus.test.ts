import type { TimeEntryView } from '$lib/features/time/model';
import { timeData } from '$lib/features/time/data';
import type { Workspace } from '@fvoci/api-client';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { TrashEntryView } from '$lib/api/client';
import { api } from '$lib/api/client';
import TimeEntryDialog from '$lib/features/time/TimeEntryDialog.svelte';
import TrashScreen from '$lib/features/trash/TrashScreen.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

// 같은 줄에서 묻는 지우기 확인: 누른 단추가 사라져도 초점이 빠지지 않고 확인 단추로 가며(질문은 그 단추의 설명),
// 취소하면 처음 단추로 돌아온다. 휴지통은 목록을 받기 전 "비어 있습니다" 대신 불러오는 중, 첫 읽기 실패에 다시 시도.
const workspace: Workspace = {
  access: 'owner',
  id: 'ws-1',
  organization_id: 'org-1',
  name: '연구실',
  role: 'admin',
  is_owner: true,
};

const entry: TrashEntryView = {
  id: 't-1',
  kind: 'document',
  root_id: 'd-1',
  title: '세미나 발표 순서',
  member_count: 0,
  deleted_by: 'u1',
  deleted_by_name: '관리자',
  deleted_at: '2026-10-20T01:00:00Z',
  expires_at: '2026-11-19T01:00:00Z',
  expiry: 'active',
  can_purge: true,
};

beforeEach(async () => {
  await setLocale('ko', { reload: false });
});
afterEach(() => {
  vi.restoreAllMocks();
});

describe('inline delete confirmation focus', () => {
  it('trash: moves focus to the confirm button, describes it, and returns focus on cancel', async () => {
    vi.spyOn(api, 'trash').mockResolvedValue({
      retention_days: 30,
      version_retention_days: null,
      entries: [entry],
    });
    render(TrashScreen, { props: { workspace } });

    await fireEvent.click(await screen.findByRole('button', { name: '영구 삭제…' }));
    const go = screen.getByRole('button', { name: '영구 삭제' });
    await waitFor(() => {
      expect(go).toHaveFocus();
    });
    expect(go).toHaveAccessibleDescription('되돌릴 수 없습니다. 영구 삭제할까요?');

    await fireEvent.click(screen.getByRole('button', { name: '취소' }));
    await waitFor(() => {
      expect(screen.getByRole('button', { name: '영구 삭제…' })).toHaveFocus();
    });
  });

  it('time entry: moves focus to the confirm button and back on cancel', async () => {
    const time: TimeEntryView = {
      id: 'e-1',
      started_at: '2026-10-20T00:00:00Z',
      ended_at: '2026-10-20T01:00:00Z',
      paused_at: null,
      paused_seconds: 0,
      seconds: 3600,
      kind: 'work',
      memo: '',
      target: null,
      version: 1,
    };
    vi.spyOn(timeData, 'timeEntryRevisions').mockResolvedValue([]);
    render(TimeEntryDialog, {
      props: {
        entry: time,
        zone: 'Asia/Seoul',
        onclose: () => undefined,
        onchanged: () => undefined,
      },
    });

    await fireEvent.click(await screen.findByRole('button', { name: '기록 지우기' }));
    const go = screen.getByRole('button', { name: '지우기' });
    await waitFor(() => {
      expect(go).toHaveFocus();
    });
    expect(go.getAttribute('aria-describedby')).not.toBeNull();
    await fireEvent.click(screen.getByRole('button', { name: '취소' }));
    await waitFor(() => {
      expect(screen.getByRole('button', { name: '기록 지우기' })).toHaveFocus();
    });
  });
});

describe('trash list states', () => {
  it('says loading instead of empty before the list arrives', () => {
    vi.spyOn(api, 'trash').mockReturnValue(new Promise(() => undefined));
    render(TrashScreen, { props: { workspace } });
    expect(screen.queryByText('휴지통이 비어 있습니다.')).toBeNull();
    expect(screen.getByText('불러오는 중…')).toBeInTheDocument();
  });

  it('offers a retry when the first load fails', async () => {
    const load = vi
      .spyOn(api, 'trash')
      .mockRejectedValueOnce(new Error('offline'))
      .mockResolvedValue({ retention_days: 30, version_retention_days: null, entries: [] });
    render(TrashScreen, { props: { workspace } });

    await fireEvent.click(await screen.findByRole('button', { name: '다시 시도' }));
    await waitFor(() => {
      expect(load).toHaveBeenCalledTimes(2);
    });
    expect(await screen.findByText('휴지통이 비어 있습니다.')).toBeInTheDocument();
  });
});
