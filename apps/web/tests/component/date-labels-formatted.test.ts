import type { Workspace } from '@fvoci/api-client';
import { render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { CalendarView, EventView, ItemView, NotificationView } from '$lib/api/client';
import { api } from '$lib/api/client';
import { formatDay } from '$lib/display';
import CalendarScreen from '$lib/features/events/CalendarScreen.svelte';
import EventDialog from '$lib/features/events/EventDialog.svelte';
import { describeRule, describeWhen } from '$lib/features/events/labels';
import { dayIn, localTimeZone } from '$lib/features/events/time';
import { notificationText } from '$lib/features/references/notification-text';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';

// 감사 W27: 표시 언어 문장 안에 날짜를 `YYYY-MM-DD` 그대로 넣던 곳(달력 날짜 머리말·만들기 단추, 알림, 반복 "까지",
// 한 회차 표시, 종일 일정 표시)이 공통 날짜 서식(D-81)을 거치지 않았다. 영어 화면에서 "2026-10-22"가 아니라
// "Oct 22, 2026"처럼 보여야 한다.
const ISO_DATE = /\d{4}-\d{2}-\d{2}/;

beforeEach(async () => {
  me.locale = 'en';
  await setLocale('en', { reload: false });
});
afterEach(() => {
  me.locale = '';
  vi.restoreAllMocks();
});

function event(over: Partial<EventView> = {}): EventView {
  return {
    id: 'e1',
    item_id: 'i1',
    workspace_id: 'ws-1',
    kind: 'meeting',
    title: 'Standup',
    description: '',
    all_day: true,
    start_date: '2026-11-02',
    end_date: '2026-11-02',
    starts_at: null,
    ends_at: null,
    timezone: localTimeZone(),
    recurrence: null,
    excluded_dates: [],
    version: 1,
    access: 'write',
    ...over,
  };
}

describe('dates inside localized text', () => {
  it('formats the due date of a notification', () => {
    const n: NotificationView = {
      id: 'n1',
      kind: 'due',
      document_id: null,
      task_id: 't1',
      document_title: 'Report',
      document_reference: 'DEV-3',
      workspace_id: 'w1',
      item_id: 'i1',
      comment_id: null,
      excerpt: null,
      due_date: '2026-10-22',
      subject: null,
      actor_name: null,
      created_at: '2026-10-19T00:00:00Z',
      read: false,
    };
    const text = notificationText(n);
    expect(text).not.toMatch(ISO_DATE);
    expect(text).toContain(formatDay('2026-10-22'));
  });

  it('formats the end of a repeat rule and all-day event dates', () => {
    const rule = describeRule({
      freq: 'weekly',
      interval: 1,
      weekdays: [],
      count: null,
      until: '2026-12-31',
    });
    expect(rule).not.toMatch(ISO_DATE);
    expect(rule).toContain(formatDay('2026-12-31'));

    const one = describeWhen(event(), localTimeZone());
    expect(one).not.toMatch(ISO_DATE);
    expect(one).toContain(formatDay('2026-11-02'));
    const range = describeWhen(event({ end_date: '2026-11-04' }), localTimeZone());
    expect(range).not.toMatch(ISO_DATE);
    expect(range).toContain(formatDay('2026-11-04'));
  });

  it('formats the single-occurrence label in the event dialog', async () => {
    vi.spyOn(api, 'event').mockResolvedValue(
      event({
        recurrence: { freq: 'weekly', interval: 1, weekdays: [1], until: null, count: null },
      }),
    );
    render(EventDialog, {
      props: {
        eventId: 'e1',
        occurrence: '2026-11-02',
        onclose: () => undefined,
        onchanged: () => undefined,
      },
    });
    expect(
      await screen.findByText(m.events_occurrence_only({ date: formatDay('2026-11-02') })),
    ).toBeTruthy();
  });

  it('formats calendar day headings and create buttons', async () => {
    const today = dayIn(new Date().toISOString(), localTimeZone());
    const calendar: CalendarView = {
      tasks: [],
      projected_tasks: [],
      events: [],
      occurrences: [],
      dates: [
        {
          kind: 'submission',
          id: 's1',
          item_id: 'i1',
          title: 'Paper',
          date: today,
          personal: false,
          done: false,
        },
      ],
      items: [],
    };
    vi.spyOn(api, 'calendar').mockResolvedValue(calendar);
    const workspace: Workspace = {
      id: 'ws-1',
      name: 'Space',
      role: 'writer',
      is_owner: false,
      realtime_enabled: false,
    };
    const item: ItemView = {
      id: 'i1',
      module_id: 'm1',
      workspace_id: 'ws-1',
      reference: 'KED-1',
      title: 'Item',
      description: '',
      visibility: 'workspace',
      access: 'write',
      archived: false,
      updated_at: '2026-10-21T00:00:00Z',
      status: 'active',
      is_template: false,
      starts_on: null,
      ends_on: null,
    };
    render(CalendarScreen, { props: { workspace, scope: { itemId: 'i1' }, item, embedded: true } });

    const agenda = document.querySelector('[data-testid="calendar-agenda"]');
    if (agenda === null) throw new Error('no agenda');
    await waitFor(() => {
      expect(agenda.querySelector('h2')).not.toBeNull();
    });
    const todayText = m.events_day_today({ day: formatDay(today) });
    expect(agenda.querySelector('h2')?.textContent).toBe(todayText);

    const create = document.querySelector(`[data-create-day="${today}"]`);
    expect(create?.getAttribute('aria-label')).toBe(
      m.events_task_create_on({ day: formatDay(today) }),
    );
    const cell = create?.closest('td');
    expect(cell?.querySelector('.sr-only')?.textContent).toBe(todayText);
  });
});
