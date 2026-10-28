import { homeApi } from '$lib/features/home/api';
import type { Workspace } from '@fvoci/api-client';
import { render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { api, NetworkError } from '$lib/api/client';
import HomeDashboard from '$lib/features/home/HomeDashboard.svelte';
import { homeExtras } from './fixtures/home';
import { setLocale } from '$lib/paraglide/runtime.js';

beforeEach(homeExtras);

// 홈을 받기 전(불러오는 중)이나 받지 못했을 때(실패) 고정·즐겨찾기·진행 중인 항목·알림 칸이 "없습니다"라고 말해,
// 데이터를 받아 비어 있는 것처럼 읽혔다. 빈 칸 안내는 홈을 받은 뒤에만 보인다(Storybook Screens/Home에서 찾음).
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
  vi.spyOn(api, 'notifications').mockResolvedValue({ notifications: [], next: null });
});

const workspace: Workspace = {
  access: 'owner',
  id: 'ws-1',
  organization_id: 'org-1',
  name: '공간',
  role: 'admin',
  is_owner: true,
  realtime_enabled: false,
};

const EMPTY_TEXTS = [
  '항목·문서·업무 화면의 “홈에 고정”으로 더할 수 있습니다.',
  '항목·문서·업무 화면의 “즐겨찾기”로 더할 수 있습니다.',
  '진행 중인 항목이 없습니다.',
  '읽지 않은 알림이 없습니다.',
];

function expectNoEmptyClaims(): void {
  for (const text of EMPTY_TEXTS) expect(screen.queryByText(text)).toBeNull();
}

describe('home empty texts before the first load', () => {
  it('does not say a section is empty while the home is loading', async () => {
    vi.spyOn(homeApi, 'home').mockImplementation(() => new Promise(() => undefined));
    render(HomeDashboard, { props: { workspaces: [workspace] } });
    await screen.findByRole('heading', { level: 1, name: '오늘' });
    expectNoEmptyClaims();
  });

  it('does not say a section is empty after the home failed to load', async () => {
    vi.spyOn(homeApi, 'home').mockRejectedValue(new NetworkError('offline'));
    render(HomeDashboard, { props: { workspaces: [workspace] } });
    await screen.findByText(/홈을 불러오지 못했습니다\./);
    expectNoEmptyClaims();
  });
});
