import type { HomeView } from '$lib/features/home/api';
import { homeApi } from '$lib/features/home/api';
import { activityApi } from '$lib/features/activity/api';
import type { Workspace } from '@fvoci/api-client';
import { render, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { CalendarView } from '$lib/api/client';
import { api } from '$lib/api/client';
import HomeDashboard from '$lib/features/home/HomeDashboard.svelte';
import { homeExtras } from './fixtures/home';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';

beforeEach(homeExtras);

afterEach(() => {
  vi.restoreAllMocks();
});

const workspace: Workspace = {
  access: 'owner',
  id: 'ws-1',
  organization_id: 'org-1',
  name: '공간',
  role: 'admin',
  is_owner: true,
};

const home: HomeView = {
  // 수요일. 주 시작(기본 일요일) 기준 이번 주는 2026-10-18 ~ 2026-10-24.
  my_tasks: { counts: { overdue: 0, today: 0, upcoming: 0, undated: 0, done: 0 }, workspaces: [] },
  today: '2026-10-21',
  up_next: [],
  up_next_counts: { overdue: 0, today: 0, upcoming: 0 },
  recent_tasks: [],
  recent_documents: [],
  active_items: [],
  pins: [],
  favorites: [],
};

const calendar: CalendarView = {
  tasks: [],
  projected_tasks: [],
  events: [],
  occurrences: [],
  dates: [
    {
      kind: 'episode',
      id: 'ep-1',
      item_id: 'i1',
      title: '3강',
      date: '2026-10-21',
      personal: true,
      done: false,
    },
    {
      kind: 'submission',
      id: 'sub-1',
      item_id: 'i2',
      title: 'Paper',
      date: '2026-10-23',
      personal: false,
      done: false,
    },
  ],
  items: [],
};

// 감사 W22: 홈 "이번 주"가 달력 API의 `dates`(D-80 강의·스터디 계획일, 투고·개정·산출물 날짜)를 빼고 업무·일정만
// 모아, 같은 주의 달력에는 보이는 날짜가 홈에서는 "—"로 보였다.
describe('home this week', () => {
  it('shows the D-80 calendar dates the calendar shows', async () => {
    await setLocale('ko', { reload: false });
    vi.spyOn(homeApi, 'home').mockResolvedValue(home);
    vi.spyOn(api, 'calendar').mockResolvedValue(calendar);
    vi.spyOn(activityApi, 'notifications').mockResolvedValue({ notifications: [], next: null });
    render(HomeDashboard, { props: { workspaces: [workspace] } });

    const list = document.querySelector('[aria-labelledby="week-heading"] ol');
    if (!(list instanceof HTMLOListElement)) throw new Error('no week list');
    // 홈과 달력을 다 읽으면 이번 주 7일이 그려진다.
    await waitFor(() => {
      expect(list.querySelectorAll('li')).toHaveLength(7);
    });
    const rows = [...list.querySelectorAll('li')].map((li) => li.textContent);
    const episode = m.events_entry_date_personal({ label: m.events_date_episode(), title: '3강' });
    const submission = m.events_entry_date({ label: m.events_date_submission(), title: 'Paper' });
    // 수요일(넷째 줄)에 내 강의 계획일, 금요일(여섯째 줄)에 투고 마감.
    expect(rows[3]).toContain(episode);
    expect(rows[5]).toContain(submission);
  });
});
