import type { Workspace } from '@fvoci/api-client';
import { fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { HomeView } from '$lib/api/client';
import { api, NetworkError } from '$lib/api/client';
import HomeDashboard from '$lib/features/home/HomeDashboard.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

// 홈을 처음 불러오지 못하면 오류만 남고 다시 받을 길이 없었다(빠른 추가를 해야만 다시 읽었다). 받기 전에는 불러오는
// 중이라고 알리고, 실패하면 오류 옆의 "다시 시도"로 다시 받는다.
afterEach(() => {
  vi.restoreAllMocks();
});

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  vi.spyOn(api, 'calendar').mockResolvedValue({
    tasks: [],
    projected_tasks: [],
    events: [],
    occurrences: [],
    dates: [],
    items: [],
  });
  vi.spyOn(api, 'notifications').mockResolvedValue([]);
});

const workspace: Workspace = {
  id: 'ws-1',
  name: '공간',
  role: 'admin',
  is_owner: true,
  realtime_enabled: false,
};

const home: HomeView = {
  today: '2026-10-20',
  up_next: [],
  up_next_counts: { overdue: 0, today: 0, upcoming: 0 },
  recent_tasks: [],
  recent_documents: [],
  active_items: [],
  pins: [],
  favorites: [],
};

describe('home first load', () => {
  it('says it is loading until the home arrives', async () => {
    vi.spyOn(api, 'home').mockImplementation(() => new Promise(() => undefined));
    render(HomeDashboard, { props: { workspaces: [workspace] } });
    expect(await screen.findByText('불러오는 중…')).toBeInTheDocument();
  });

  it('retries after a failed first load', async () => {
    const load = vi
      .spyOn(api, 'home')
      .mockRejectedValueOnce(new NetworkError('offline'))
      .mockResolvedValue(home);
    render(HomeDashboard, { props: { workspaces: [workspace] } });
    expect(await screen.findByRole('alert')).toHaveTextContent('홈을 불러오지 못했습니다.');

    await fireEvent.click(screen.getByRole('button', { name: '다시 시도' }));
    expect(await screen.findByText('진행 중인 항목이 없습니다.')).toBeInTheDocument();
    expect(screen.queryByRole('alert')).toBeNull();
    expect(screen.queryByRole('button', { name: '다시 시도' })).toBeNull();
    expect(load).toHaveBeenCalledTimes(2);
  });
});
