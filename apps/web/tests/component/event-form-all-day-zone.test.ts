import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { EventView } from '$lib/api/client';
import EventForm from '$lib/features/events/EventForm.svelte';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';

// R5 delta2 D2-F-2: 종일 일정도 서버가 시간대를 검증·저장하고 정렬에 쓴다(`EventInput.timezone`은 필수). 칸별 비교가
// 종일일 때 시간대를 보지 않아, 종일을 끄고 시간대를 고친 뒤 다시 켜면 저장될 칸이 바뀌었는데도 고친 것이 없다고 했다.

const allDay: EventView = {
  id: 'ev-1',
  item_id: 'i1',
  workspace_id: 'w1',
  kind: 'meeting',
  title: '회의',
  description: '',
  all_day: true,
  start_date: '2026-10-20',
  end_date: '2026-10-20',
  starts_at: null,
  ends_at: null,
  timezone: 'Asia/Seoul',
  recurrence: null,
  excluded_dates: [],
  version: 1,
  access: 'write',
};

beforeEach(async () => {
  localStorage.clear();
  me.userId = 'zone-user';
  await setLocale('ko', { reload: false });
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  me.userId = '';
});

describe('an all-day event form', () => {
  it('counts a changed time zone as an edit even after switching back to all day', async () => {
    const onunsaved = vi.fn();
    render(EventForm, {
      props: {
        idPrefix: 'zone',
        initial: allDay,
        submitLabel: m.common_save(),
        onsubmit: () => Promise.resolve(),
        attachGuard: false,
        onunsaved,
      },
    });
    const allDayBox = screen.getByRole('checkbox', { name: m.events_all_day() });
    await fireEvent.click(allDayBox);
    await fireEvent.input(screen.getByLabelText(m.events_time_zone()), {
      target: { value: 'Europe/Paris' },
    });
    await fireEvent.click(allDayBox);
    expect(screen.queryByLabelText(m.events_time_zone())).toBeNull();
    expect(onunsaved).toHaveBeenLastCalledWith(true);
  });
});
