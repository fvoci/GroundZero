import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { api } from '$lib/api/client';
import TimeScreen from '$lib/features/time/TimeScreen.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

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
    render(TimeScreen);

    expect(screen.queryByText('이 주에는 기록이 없습니다.')).toBeNull();
    expect(screen.getAllByText('불러오는 중…').length).toBeGreaterThan(0);
  });

  it('offers a retry when the first load fails, and shows the list after it', async () => {
    const entries = vi
      .spyOn(api, 'timeEntries')
      .mockRejectedValueOnce(new Error('offline'))
      .mockResolvedValue([]);
    vi.spyOn(api, 'timeSummary').mockResolvedValue([]);
    render(TimeScreen);

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
});
