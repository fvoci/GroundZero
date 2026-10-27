import './fixtures/task-statuses';
import type { Module, Workspace } from '@fvoci/api-client';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { EventView, ItemView } from '$lib/api/client';
import { api } from '$lib/api/client';
import ItemScreen from '$lib/features/modules/ItemScreen.svelte';
import ModuleScreen from '$lib/features/modules/ModuleScreen.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';
import { openItemGroup, openModuleView, resetRoute } from './fixtures/item-groups';

// F-1(R5 B): 항목 등급(app_grade_item)은 보관을 반영하지 않으므로 보관된 모듈의 항목도 owner·write를 받는다(D-113).
// 일정 등급(app_grade_under_item)은 보관이면 owner·write를 read로 내린다(R5 B 최종 FC-6). 항목 화면은 정보·기록·업무판만
// 잠그고 일정 목록·달력에는 보관을 넘기지 않아, 서버가 55000으로 거부할 일정 추가와 날짜 칸의 업무 만들기가 열렸다.
afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  resetRoute();
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

function module(archived: boolean): Module {
  return {
    access: 'owner',
    id: 'm1',
    workspace_id: 'ws-1',
    kind: 'project',
    name: '개발',
    key: 'DEV',
    previous_keys: [],
    archived,
    icon: '',
    description: '',
  };
}

const item: ItemView = {
  id: 'i1',
  module_id: 'm1',
  workspace_id: 'ws-1',
  reference: 'DEV-1',
  title: '항목',
  description: '설명',
  visibility: 'workspace',
  access: 'owner',
  archived: false,
  updated_at: '2026-10-19T00:00:00Z',
  status: 'active',
  is_template: false,
  version: 1,
  starts_on: null,
  ends_on: null,
};

const event: EventView = {
  id: 'e1',
  item_id: 'i1',
  workspace_id: 'ws-1',
  kind: 'meeting',
  title: '주간 회의',
  description: '',
  all_day: true,
  start_date: '2026-10-21',
  end_date: '2026-10-21',
  starts_at: null,
  ends_at: null,
  timezone: 'Asia/Seoul',
  recurrence: null,
  excluded_dates: [],
  version: 1,
  access: 'write',
};

async function mountItem(archived: boolean): Promise<void> {
  await setLocale('ko', { reload: false });
  vi.stubGlobal('fetch', () => new Promise<Response>(() => undefined));
  vi.spyOn(api, 'item').mockResolvedValue(item);
  vi.spyOn(api, 'module').mockResolvedValue(module(archived));
  vi.spyOn(api, 'itemDocuments').mockResolvedValue([]);
  // 서버가 주는 등급: 보관이면 일정은 read다.
  const shown: EventView = archived ? { ...event, access: 'read' } : event;
  vi.spyOn(api, 'itemEvents').mockResolvedValue([shown]);
  vi.spyOn(api, 'event').mockResolvedValue(shown);
  render(ItemScreen, { props: { workspace, itemId: 'i1', workspaces: [workspace] } });
  await screen.findByRole('heading', { name: '항목' });
  // 일정 목록과 항목 달력은 일정 묶음에 있다.
  await openItemGroup('일정');
}

describe('events of an archived module', () => {
  it('hides event and task creation and opens events read-only on the item screen', async () => {
    await mountItem(true);
    await fireEvent.click(await screen.findByRole('button', { name: /주간 회의/ }));
    const dialog = await screen.findByRole('dialog');
    // 대화상자는 일정을 읽은 뒤에 본문을 그린다. 내용이 보인 뒤에야 부재 단언이 뜻이 있다(FC-7).
    expect(await within(dialog).findByText('주간 회의')).toBeInTheDocument();
    expect(document.getElementById('event-edit-title')).toBeNull();
    expect(screen.queryByRole('button', { name: m.events_delete_event() })).toBeNull();
    expect(screen.queryByRole('button', { name: m.events_add() })).toBeNull();
    expect(document.querySelector('[data-create-top]')).toBeNull();
    expect(document.querySelector('[data-create-day]')).toBeNull();
  });

  it('keeps event and task creation for an item of an active module', async () => {
    await mountItem(false);
    expect(await screen.findByRole('button', { name: m.events_add() })).toBeInTheDocument();
    expect(document.querySelector('[data-create-top]')).not.toBeNull();
    await fireEvent.click(await screen.findByRole('button', { name: /주간 회의/ }));
    await screen.findByRole('dialog');
    await waitFor(() => {
      expect(document.getElementById('event-edit-title')).not.toBeNull();
    });
  });

  it('hides task creation on the calendar of an archived module screen', async () => {
    await setLocale('ko', { reload: false });
    vi.stubGlobal('fetch', () => new Promise<Response>(() => undefined));
    vi.spyOn(api, 'module').mockResolvedValue(module(true));
    vi.spyOn(api, 'items').mockResolvedValue([]);
    render(ModuleScreen, { workspace, moduleId: 'm1' });
    await screen.findByRole('heading', { name: '개발' });
    await openModuleView('달력');
    expect(document.getElementById('calendar-heading')).not.toBeNull();
    expect(document.querySelector('[data-create-top]')).toBeNull();
    expect(document.querySelector('[data-create-day]')).toBeNull();
  });
});
import './fixtures/authenticated';
