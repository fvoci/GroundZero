import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { api } from '$lib/api/client';
import { org } from '$lib/org.svelte';
import { m } from '$lib/paraglide/messages.js';
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

  it('asks for an organization before requesting', async () => {
    org.reset();
    const entries = vi.spyOn(api, 'timeEntries').mockResolvedValue([]);
    const summary = vi.spyOn(api, 'timeSummary').mockResolvedValue([]);
    render(TimeScreen);
    expect(await screen.findByText(m.app_organization_required())).toBeInTheDocument();
    expect(entries).not.toHaveBeenCalled();
    expect(summary).not.toHaveBeenCalled();
  });

  it('only shows my records, with no sharing switch, once an organization is chosen', async () => {
    // R5 최종 A FA-6: 조직을 고른 상태(시험 설정의 기본)에서 목록이 그려진 뒤에 본다. 조직이 없으면 본문이 아예 그려지지
    // 않아 이 단언이 늘 참이었다. 시간 기록은 현재 조직 범위의 개인 화면이다(D-111).
    vi.spyOn(api, 'timeEntries').mockResolvedValue([]);
    vi.spyOn(api, 'timeSummary').mockResolvedValue([]);
    render(TimeScreen);
    expect(await screen.findByText('이 주에는 기록이 없습니다.')).toBeInTheDocument();
    expect(screen.getByRole('heading', { level: 1, name: m.time_my_time() })).toBeInTheDocument();
    expect(screen.queryByRole('checkbox')).toBeNull();
    expect(screen.queryByRole('switch')).toBeNull();
    expect(screen.queryByText(/공유/)).toBeNull();
  });
});
