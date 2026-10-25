import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { api } from '$lib/api/client';
import ItemTimePanel from '$lib/features/time/ItemTimePanel.svelte';
import TaskTimePanel from '$lib/features/time/TaskTimePanel.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

beforeEach(async () => {
  await setLocale('ko', { reload: false });
});
afterEach(() => {
  vi.restoreAllMocks();
});

// 업무·항목의 시간 합계를 못 읽으면 조용히 비우지 않고 알리고 다시 시도할 수 있다. 타이머 상태 글은 처음부터 있는
// 알림 영역 안에서 바뀐다.
describe('time panels', () => {
  it('task: says the total failed to load and retries', async () => {
    const load = vi
      .spyOn(api, 'taskTime')
      .mockRejectedValueOnce(new Error('offline'))
      .mockResolvedValue({
        estimate_minutes: 60,
        mine_seconds: 1800,
      });
    render(TaskTimePanel, { props: { taskId: 't-1' } });

    expect(screen.getByRole('status')).toBeInTheDocument();
    const alert = await screen.findByRole('alert');
    expect(alert.textContent).toContain('시간 기록을 불러오지 못했습니다.');
    await fireEvent.click(screen.getByRole('button', { name: '다시 시도' }));
    await waitFor(() => {
      expect(load).toHaveBeenCalledTimes(2);
    });
    await waitFor(() => {
      expect(screen.queryByRole('alert')).toBeNull();
    });
  });

  it('item: says the total failed to load and retries', async () => {
    const load = vi
      .spyOn(api, 'itemTime')
      .mockRejectedValueOnce(new Error('offline'))
      .mockResolvedValue({ estimate_minutes: 0, kinds: [] });
    render(ItemTimePanel, { props: { itemId: 'i-1' } });

    expect(screen.getByRole('status')).toBeInTheDocument();
    expect(await screen.findByRole('alert')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: '다시 시도' }));
    await waitFor(() => {
      expect(load).toHaveBeenCalledTimes(2);
    });
    expect(await screen.findByText('아직 이 항목의 시간 기록이 없습니다.')).toBeInTheDocument();
    expect(screen.queryByRole('alert')).toBeNull();
  });
});
