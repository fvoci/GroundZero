import type { Workspace } from '@fvoci/api-client';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { api } from '$lib/api/client';
import TimeScreen from '$lib/features/time/TimeScreen.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

const workspaces: Workspace[] = [
  {
    id: 'ws-1',
    organization_id: 'org-1',
    name: '연구실',
    role: 'admin',
    access: 'owner',
    is_owner: true,
    realtime_enabled: false,
  },
];

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  vi.spyOn(api, 'currentTimer').mockResolvedValue(null);
});
afterEach(() => {
  vi.restoreAllMocks();
});

const pending = <T>(): Promise<T> => new Promise<T>(() => undefined);

// 내 시간 화면의 불러오는 중·실패·중복 요청 처리.
describe('time screen states', () => {
  it('says loading instead of "no entries" before the first list arrives', () => {
    vi.spyOn(api, 'timeEntries').mockReturnValue(pending());
    vi.spyOn(api, 'timeSummary').mockReturnValue(pending());
    vi.spyOn(api, 'timeSharing').mockReturnValue(pending());
    render(TimeScreen, { props: { workspaces } });

    expect(screen.queryByText('이 주에는 기록이 없습니다.')).toBeNull();
    expect(screen.getAllByText('불러오는 중…').length).toBeGreaterThan(0);
  });

  it('offers a retry when the first load fails, and shows the list after it', async () => {
    const entries = vi
      .spyOn(api, 'timeEntries')
      .mockRejectedValueOnce(new Error('offline'))
      .mockResolvedValue([]);
    vi.spyOn(api, 'timeSummary').mockResolvedValue([]);
    vi.spyOn(api, 'timeSharing').mockResolvedValue(false);
    render(TimeScreen, { props: { workspaces } });

    const retry = await screen.findByRole('button', { name: '다시 시도' });
    await fireEvent.click(retry);
    await waitFor(() => {
      expect(entries).toHaveBeenCalledTimes(2);
    });
    await waitFor(() => {
      expect(screen.queryByRole('button', { name: '다시 시도' })).toBeNull();
    });
    expect(screen.queryAllByText('불러오는 중…')).toHaveLength(0);
  });

  it('keeps sharing switches disabled until the setting is known, and retries a failed load', async () => {
    vi.spyOn(api, 'timeEntries').mockResolvedValue([]);
    vi.spyOn(api, 'timeSummary').mockResolvedValue([]);
    const sharing = vi
      .spyOn(api, 'timeSharing')
      .mockRejectedValueOnce(new Error('offline'))
      .mockResolvedValue(true);
    const set = vi.spyOn(api, 'setTimeSharing').mockResolvedValue(false);
    render(TimeScreen, { props: { workspaces } });

    const box = screen.getByRole('checkbox', { name: '연구실' });
    expect(box).toBeDisabled();
    expect(await screen.findByText('공유 설정을 불러오지 못했습니다.')).toBeInTheDocument();
    await fireEvent.click(box);
    expect(set).not.toHaveBeenCalled();

    const retry = screen.getAllByRole('button', { name: '다시 시도' }).at(-1);
    if (retry === undefined) throw new Error('retry');
    await fireEvent.click(retry);
    await waitFor(() => {
      expect(sharing).toHaveBeenCalledTimes(2);
    });
    await waitFor(() => {
      expect(box).toBeEnabled();
    });
    expect(box).toHaveAttribute('aria-checked', 'true');
  });

  it('ignores a second toggle while the first is still saving', async () => {
    vi.spyOn(api, 'timeEntries').mockResolvedValue([]);
    vi.spyOn(api, 'timeSummary').mockResolvedValue([]);
    vi.spyOn(api, 'timeSharing').mockResolvedValue(false);
    let finish: (on: boolean) => void = () => undefined;
    const set = vi.spyOn(api, 'setTimeSharing').mockReturnValue(
      new Promise((r) => {
        finish = r;
      }),
    );
    render(TimeScreen, { props: { workspaces } });

    const box = screen.getByRole('checkbox', { name: '연구실' });
    await waitFor(() => {
      expect(box).toBeEnabled();
    });
    await fireEvent.click(box);
    await fireEvent.click(box);
    expect(set).toHaveBeenCalledTimes(1);
    expect(box).toBeDisabled();
    finish(true);
    await waitFor(() => {
      expect(box).toBeEnabled();
    });
  });
});
