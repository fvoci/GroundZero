import { taskData as work } from '$lib/features/tasks/detail-data';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { ItemProfileView, ItemView, TimeEntryView } from '$lib/api/client';
import { api } from '$lib/api/client';
import ItemProfilePanel from '$lib/features/modules/ItemProfilePanel.svelte';
import TimeEntryDialog from '$lib/features/time/TimeEntryDialog.svelte';
import TimeScreen from '$lib/features/time/TimeScreen.svelte';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';

// R5 2차 확인 필요 1: bits-ui Select는 조상 `<fieldset disabled>`가 아니라 자기 `disabled`만 보고 pointerdown으로 열린다.
// 저장 중 잠금(F-16·F-8)이 fieldset뿐이면 상태·리더·종류를 그사이 바꿔 스냅숏이 달라지고 같은 사용자의 성공을 버린다.
// 저장하는 동안 Select가 열리지 않는지 본다.

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  localStorage.clear();
  me.userId = 'user-a';
  Element.prototype.hasPointerCapture = (): boolean => false;
  Element.prototype.setPointerCapture = (): void => undefined;
  Element.prototype.releasePointerCapture = (): void => undefined;
  Element.prototype.scrollIntoView = (): void => undefined;
});
afterEach(() => {
  vi.restoreAllMocks();
  localStorage.clear();
  me.userId = '';
});

function trigger(labelledBy: string): HTMLElement {
  const el = document.querySelector<HTMLElement>(`[aria-labelledby="${labelledBy}"]`);
  if (el === null) throw new Error(labelledBy);
  return el;
}

/**
 * 마우스로 눌러 본다(bits-ui는 pointerdown에서 연다). 열렸는지는 트리거의 `aria-expanded`로 본다(목록은 portal이라
 * `*ByRole('option')`이 숨은 요소로 여겨 찾지 못한다).
 */
async function opens(el: HTMLElement): Promise<boolean> {
  await fireEvent.pointerDown(el, { button: 0, pointerType: 'mouse' });
  await new Promise((r) => setTimeout(r, 30));
  return el.getAttribute('aria-expanded') === 'true';
}

const item: ItemView = {
  id: 'i1',
  module_id: 'm1',
  workspace_id: 'ws-1',
  reference: 'DEV-1',
  title: '항목',
  description: '',
  visibility: 'workspace',
  access: 'owner',
  archived: false,
  updated_at: '',
  status: 'active',
  is_template: false,
  version: 1,
  starts_on: null,
  ends_on: null,
};
const profile: ItemProfileView = {
  leader: null,
  goal: '',
  scope: '',
  starts_on: null,
  ends_on: null,
  status: 'active',
  details: {},
  members: [],
  groups: [],
  can_edit: true,
  can_manage_members: false,
  version: 1,
};
const entry: TimeEntryView = {
  id: 'entry-a',
  started_at: '2026-10-20T04:00:00Z',
  ended_at: '2026-10-20T05:00:00Z',
  paused_at: null,
  paused_seconds: 0,
  seconds: 3600,
  kind: 'work',
  memo: 'saved',
  target: null,
  version: 3,
};

describe('selects while a save is in flight', () => {
  it('keeps the item profile status and leader closed', async () => {
    vi.spyOn(api, 'itemProfile').mockResolvedValue(profile);
    vi.spyOn(work, 'assignable').mockResolvedValue([{ user_id: 'u1', display_name: 'First' }]);
    const save = vi.spyOn(api, 'setItemProfile').mockReturnValue(new Promise(() => undefined));
    render(ItemProfilePanel, { item, kind: 'project', onchanged: vi.fn() });
    await fireEvent.click(await screen.findByRole('button', { name: m.modules_edit() }));
    await fireEvent.input(screen.getByLabelText(m.modules_goal()), { target: { value: '목표' } });
    await fireEvent.click(screen.getByRole('button', { name: m.common_save() }));
    await waitFor(() => {
      expect(save).toHaveBeenCalledOnce();
    });
    expect(await opens(trigger('item-status-label'))).toBe(false);
    expect(await opens(trigger('item-leader-label'))).toBe(false);
  });

  it('keeps the time entry kind closed', async () => {
    vi.spyOn(api, 'timeEntryRevisions').mockResolvedValue([]);
    const update = vi.spyOn(api, 'updateTimeEntry').mockReturnValue(new Promise(() => undefined));
    render(TimeEntryDialog, { entry, zone: 'UTC', onclose: vi.fn(), onchanged: vi.fn() });
    await fireEvent.input(await screen.findByLabelText(m.time_memo()), {
      target: { value: '고침' },
    });
    await fireEvent.click(screen.getByRole('button', { name: m.common_save() }));
    await waitFor(() => {
      expect(update).toHaveBeenCalledOnce();
    });
    expect(await opens(trigger('time-edit-kind-label'))).toBe(false);
  });

  it('keeps the manual time kind closed', async () => {
    vi.spyOn(api, 'timeEntries').mockResolvedValue([]);
    vi.spyOn(api, 'timeSummary').mockResolvedValue([]);
    vi.spyOn(api, 'currentTimer').mockResolvedValue(null);
    const add = vi
      .spyOn(api, 'addTimeEntry')
      .mockReturnValue(new Promise<TimeEntryView>(() => undefined));
    render(TimeScreen);
    await fireEvent.input(await screen.findByLabelText(m.time_memo()), {
      target: { value: '수동' },
    });
    await fireEvent.click(screen.getByRole('button', { name: m.time_add_entry() }));
    await waitFor(() => {
      expect(add).toHaveBeenCalledOnce();
    });
    expect(await opens(trigger('time-manual-kind-label'))).toBe(false);
  });

  it('still opens the manual time kind when idle (contrast)', async () => {
    vi.spyOn(api, 'timeEntries').mockResolvedValue([]);
    vi.spyOn(api, 'timeSummary').mockResolvedValue([]);
    vi.spyOn(api, 'currentTimer').mockResolvedValue(null);
    render(TimeScreen);
    await screen.findByLabelText(m.time_memo());
    expect(await opens(trigger('time-manual-kind-label'))).toBe(true);
  });
});
