import type { Module, Workspace } from '@fvoci/api-client';
import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { CalendarView, EventView, ItemView } from '$lib/api/client';
import { api } from '$lib/api/client';
import { displaySettings } from '$lib/display';
import CalendarScreen from '$lib/features/events/CalendarScreen.svelte';
import { dayIn } from '$lib/features/events/time';
import { me } from '$lib/me.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

// R5 P3 F-1 남은 제한(D-113: 보관은 등급 위의 쓰기 금지). 일정 응답(`EventView`)에는 보관 값이 없어 워크스페이스·모듈
// 달력은 보관된 항목·모듈의 일정을 고칠 수 있는 것처럼 보였다. 일정을 열 때 그 항목(과 모듈)의 보관 여부를 읽어
// 쓰기를 감춘다. 서버 계약은 그대로다. 만들기 대상 선택에서 보관된 모듈·항목을 빼는 것은 CalendarTaskDialog가 이미 한다.

const workspace: Workspace = {
  access: 'owner',
  id: 'ws-1',
  organization_id: 'org-1',
  name: '공간',
  role: 'admin',
  is_owner: true,
  realtime_enabled: false,
};

const today = dayIn(new Date().toISOString(), displaySettings().timeZone);

const event: EventView = {
  id: 'ev-1',
  item_id: 'item-1',
  workspace_id: 'ws-1',
  kind: 'meeting',
  title: '주간 회의',
  description: '',
  all_day: true,
  start_date: today,
  end_date: today,
  starts_at: null,
  ends_at: null,
  timezone: 'Asia/Seoul',
  recurrence: null,
  excluded_dates: [],
  version: 1,
  access: 'owner',
};

const calendar: CalendarView = {
  tasks: [],
  projected_tasks: [],
  events: [event],
  occurrences: [
    {
      event_id: 'ev-1',
      occurrence: today,
      start_date: today,
      end_date: today,
      starts_at: null,
      ends_at: null,
      skipped: false,
    },
  ],
  dates: [],
  items: [{ id: 'item-1', reference: 'DEV-1', title: '항목' }],
};

function module(archived: boolean): Module {
  return {
    id: 'mod-1',
    workspace_id: 'ws-1',
    kind: 'project',
    name: '개발',
    key: 'DEV',
    previous_keys: [],
    archived,
    icon: '',
    description: '',
    access: 'owner',
  };
}

function item(archived: boolean): ItemView {
  return {
    id: 'item-1',
    workspace_id: 'ws-1',
    module_id: 'mod-1',
    title: '항목',
    description: '',
    reference: 'DEV-1',
    access: 'owner',
    archived,
    is_template: false,
    status: 'active',
    visibility: 'workspace',
    starts_on: null,
    ends_on: null,
    updated_at: '2026-10-27T00:00:00Z',
    version: 1,
  };
}

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  me.userId = 'user-a';
  vi.spyOn(api, 'calendar').mockResolvedValue(calendar);
  vi.spyOn(api, 'event').mockResolvedValue(event);
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  me.userId = '';
  localStorage.clear();
});

async function openEvent(): Promise<void> {
  const [entry] = await screen.findAllByRole('button', { name: /주간 회의/ });
  if (entry === undefined) throw new Error('event entry');
  await fireEvent.click(entry);
  await screen.findByRole('dialog');
}

describe('workspace calendar events under archived items or modules', () => {
  it('hides editing for an event of an archived item', async () => {
    vi.spyOn(api, 'modules').mockResolvedValue([module(false)]);
    vi.spyOn(api, 'item').mockResolvedValue(item(true));
    render(CalendarScreen, { props: { workspace } });
    await openEvent();
    await vi.waitFor(() => {
      expect(api.event).toHaveBeenCalledWith('ev-1');
    });
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(screen.queryByRole('button', { name: '일정 지우기' })).toBeNull();
  });

  it('hides editing for an event of an item in an archived module', async () => {
    vi.spyOn(api, 'modules').mockResolvedValue([module(true)]);
    vi.spyOn(api, 'item').mockResolvedValue(item(false));
    render(CalendarScreen, { props: { workspace } });
    await openEvent();
    await vi.waitFor(() => {
      expect(api.event).toHaveBeenCalledWith('ev-1');
    });
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(screen.queryByRole('button', { name: '일정 지우기' })).toBeNull();
  });

  it('hides editing in a module calendar for an event of an archived item', async () => {
    vi.spyOn(api, 'item').mockResolvedValue(item(true));
    render(CalendarScreen, { props: { workspace, scope: { moduleId: 'mod-1' } } });
    await openEvent();
    await vi.waitFor(() => {
      expect(api.event).toHaveBeenCalledWith('ev-1');
    });
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(screen.queryByRole('button', { name: '일정 지우기' })).toBeNull();
  });

  it('keeps editing for an event of an active item and module', async () => {
    vi.spyOn(api, 'modules').mockResolvedValue([module(false)]);
    vi.spyOn(api, 'item').mockResolvedValue(item(false));
    render(CalendarScreen, { props: { workspace } });
    await openEvent();
    expect(await screen.findByRole('button', { name: '일정 지우기' })).toBeInTheDocument();
  });
});

describe('when the archive check cannot be read', () => {
  it('shows editing and leaves the refusal to the server', async () => {
    vi.spyOn(api, 'modules').mockResolvedValue([module(false)]);
    vi.spyOn(api, 'item').mockRejectedValue(new Error('offline'));
    render(CalendarScreen, { props: { workspace } });
    await openEvent();
    expect(await screen.findByRole('button', { name: '일정 지우기' })).toBeInTheDocument();
  });
});
