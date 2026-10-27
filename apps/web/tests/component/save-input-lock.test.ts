import type { Module, Workspace } from '@fvoci/api-client';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { EventView, ItemProfileView, ItemView, TimeEntryView } from '$lib/api/client';
import { api, NetworkError } from '$lib/api/client';
import * as feedback from '$lib/features/feedback/feedback.svelte';
import ItemEvents from '$lib/features/events/ItemEvents.svelte';
import ItemCopy from '$lib/features/modules/ItemCopy.svelte';
import ItemProfilePanel from '$lib/features/modules/ItemProfilePanel.svelte';
import ModuleScreen from '$lib/features/modules/ModuleScreen.svelte';
import TimeEntryDialog from '$lib/features/time/TimeEntryDialog.svelte';
import TimeScreen from '$lib/features/time/TimeScreen.svelte';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';
import { router } from '$lib/router.svelte';

// F-16·F-8(R5 B): 저장 요청이 가는 동안 입력과 초안 되살리기가 열려 있으면, 그사이 고친 입력 때문에 같은 사용자의
// 성공 응답을 버렸다. 만들기는 고친 입력으로 다시 내면 새 요청 id로 두 번째 항목·일정이 생겼고, 고치기는 기준 판을
// 옮기지 않아 다음 저장이 자기 저장과 409로 부딪쳤다. 저장 중에는 입력을 잠그고(D-97 구현 해석) 결과를 반영한다.

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  localStorage.clear();
  me.userId = 'user-a';
});
afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  localStorage.clear();
  me.userId = '';
});

/** 끝나지 않은 요청과 그것을 끝낼 함수. */
function held<T>(): {
  promise: Promise<T>;
  resolve: (value: T) => void;
  reject: (e: unknown) => void;
} {
  let resolve: ((value: T) => void) | undefined;
  let reject: ((e: unknown) => void) | undefined;
  const promise = new Promise<T>((ok, fail) => {
    resolve = ok;
    reject = fail;
  });
  if (resolve === undefined || reject === undefined) throw new Error('promise');
  return { promise, resolve, reject };
}

const workspace: Workspace = {
  id: 'ws',
  organization_id: 'org-1',
  name: '공간',
  access: 'owner',
  role: 'admin',
  is_owner: true,
  realtime_enabled: false,
};
const mod: Module = {
  id: 'mod',
  workspace_id: 'ws',
  kind: 'project',
  name: '개발',
  key: 'DEV',
  previous_keys: [],
  archived: false,
  icon: '',
  description: '',
  access: 'owner',
};
const item: ItemView = {
  id: 'source',
  module_id: mod.id,
  workspace_id: workspace.id,
  reference: 'DEV-1',
  title: '원본',
  description: '',
  visibility: 'workspace',
  access: 'owner',
  archived: false,
  updated_at: '',
  status: 'active',
  is_template: true,
  version: 1,
  starts_on: null,
  ends_on: null,
};

describe('creating while a request is in flight', () => {
  it('locks item copy input and shows the copy once it succeeds', async () => {
    vi.spyOn(api, 'modules').mockResolvedValue([mod]);
    const done = vi.spyOn(feedback, 'notifyDone');
    const request = held<Awaited<ReturnType<typeof api.copyItem>>>();
    const copy = vi.spyOn(api, 'copyItem').mockReturnValue(request.promise);
    render(ItemCopy, { item, module: mod });
    await fireEvent.click(screen.getByRole('button', { name: m.modules_copy() }));
    const title = await screen.findByLabelText(m.modules_copy_new_title());
    await fireEvent.input(title, { target: { value: '사본' } });
    await fireEvent.click(screen.getByRole('button', { name: m.modules_copy() }));
    await waitFor(() => {
      expect(copy).toHaveBeenCalledOnce();
    });
    expect(title).toBeDisabled();
    expect(screen.getByLabelText(m.modules_copy_module())).toBeDisabled();
    request.resolve({ item_id: 'copied', skipped_documents: 0 });
    // 일반 완료는 토스트이고 새 항목 열기는 그 동작 단추다(D-82).
    await waitFor(() => {
      expect(done).toHaveBeenCalledWith(
        m.modules_copy_done(),
        'item-copy:copied',
        expect.objectContaining({
          action: expect.objectContaining({ label: m.modules_copy_open() }),
        }),
      );
    });
    expect(screen.queryByLabelText(m.modules_copy_new_title())).toBeNull();
    expect(copy).toHaveBeenCalledOnce();
  });

  it('locks the template form and opens the created item once it succeeds', async () => {
    vi.stubGlobal('fetch', () => new Promise<Response>(() => undefined));
    vi.spyOn(api, 'module').mockResolvedValue(mod);
    vi.spyOn(api, 'items').mockResolvedValue([{ ...item, id: 'template' }]);
    const navigate = vi.spyOn(router, 'navigate').mockImplementation(() => undefined);
    const request = held<Awaited<ReturnType<typeof api.copyItem>>>();
    const copy = vi.spyOn(api, 'copyItem').mockReturnValue(request.promise);
    render(ModuleScreen, { workspace, moduleId: mod.id });
    const title = await screen.findByLabelText(m.modules_template_item_title());
    await fireEvent.input(title, { target: { value: '새 항목' } });
    await fireEvent.click(screen.getByRole('button', { name: m.modules_from_template() }));
    await waitFor(() => {
      expect(copy).toHaveBeenCalledOnce();
    });
    expect(title).toBeDisabled();
    expect(screen.getByLabelText(m.modules_template())).toBeDisabled();
    request.resolve({ item_id: 'created', skipped_documents: 0 });
    await waitFor(() => {
      expect(navigate).toHaveBeenCalledWith(expect.stringContaining('created'));
    });
    expect(copy).toHaveBeenCalledOnce();
  });

  it('locks a new event form and closes it once the event is created', async () => {
    vi.spyOn(api, 'itemEvents').mockResolvedValue([]);
    const request = held<EventView>();
    const create = vi.spyOn(api, 'createEvent').mockReturnValue(request.promise);
    render(ItemEvents, { item: { ...item, is_template: false } });
    await fireEvent.click(screen.getByRole('button', { name: m.events_add() }));
    const title = await screen.findByLabelText(m.events_title());
    await fireEvent.input(title, { target: { value: '회의' } });
    await fireEvent.click(screen.getByRole('button', { name: m.events_create() }));
    await waitFor(() => {
      expect(create).toHaveBeenCalledOnce();
    });
    expect(title).toBeDisabled();
    expect(screen.getByLabelText(m.events_description())).toBeDisabled();
    // bits-ui Select는 조상 fieldset의 disabled를 보지 않고 자기 disabled만 본다(pointerdown으로 연다). 그래서 종류
    // 선택은 fieldset이 아니라 자기 disabled로 잠겨야 한다(R5 delta2 확인 필요 1).
    expect(screen.getByRole('button', { name: m.events_kind() })).toHaveAttribute('data-disabled');
    request.resolve({
      id: 'e1',
      item_id: item.id,
      workspace_id: workspace.id,
      kind: 'meeting',
      title: '회의',
      description: '',
      all_day: false,
      start_date: null,
      end_date: null,
      starts_at: '2026-10-27T00:00:00Z',
      ends_at: '2026-10-27T01:00:00Z',
      timezone: 'UTC',
      recurrence: null,
      excluded_dates: [],
      version: 1,
      access: 'write',
    });
    await waitFor(() => {
      expect(screen.queryByLabelText(m.events_title())).toBeNull();
    });
    expect(create).toHaveBeenCalledOnce();
  });
});

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

describe('saving edits while a request is in flight', () => {
  it('locks the item profile form and closes it on its own save', async () => {
    vi.spyOn(api, 'itemProfile').mockResolvedValue(profile);
    vi.spyOn(api, 'assignable').mockResolvedValue([]);
    const request = held<Awaited<ReturnType<typeof api.setItemProfile>>>();
    const save = vi.spyOn(api, 'setItemProfile').mockReturnValue(request.promise);
    render(ItemProfilePanel, { item, kind: 'project', onchanged: vi.fn() });
    await fireEvent.click(await screen.findByRole('button', { name: m.modules_edit() }));
    const goal = screen.getByLabelText(m.modules_goal());
    await fireEvent.input(goal, { target: { value: '목표' } });
    await fireEvent.click(screen.getByRole('button', { name: m.common_save() }));
    await waitFor(() => {
      expect(save).toHaveBeenCalledOnce();
    });
    expect(goal).toBeDisabled();
    expect(screen.getByRole('button', { name: m.common_cancel() })).toBeDisabled();
    request.resolve({ kind: 'saved' });
    await waitFor(() => {
      expect(screen.queryByLabelText(m.modules_goal())).toBeNull();
    });
  });

  it('locks the time entry form and closes the dialog on its own save', async () => {
    vi.spyOn(api, 'timeEntryRevisions').mockResolvedValue([]);
    const request = held<Awaited<ReturnType<typeof api.updateTimeEntry>>>();
    const update = vi.spyOn(api, 'updateTimeEntry').mockReturnValue(request.promise);
    const onclose = vi.fn();
    render(TimeEntryDialog, { entry, zone: 'UTC', onclose, onchanged: vi.fn() });
    const memo = await screen.findByLabelText(m.time_memo());
    await fireEvent.input(memo, { target: { value: 'first edit' } });
    await fireEvent.click(screen.getByRole('button', { name: m.common_save() }));
    await waitFor(() => {
      expect(update).toHaveBeenCalledOnce();
    });
    expect(memo).toBeDisabled();
    request.resolve({ kind: 'updated', entry: { ...entry, memo: 'first edit', version: 4 } });
    await waitFor(() => {
      expect(onclose).toHaveBeenCalledOnce();
    });
  });

  it('locks manual time input and reports a failed manual entry', async () => {
    vi.spyOn(api, 'timeEntries').mockResolvedValue([]);
    vi.spyOn(api, 'timeSummary').mockResolvedValue([]);
    vi.spyOn(api, 'currentTimer').mockResolvedValue(null);
    const request = held<TimeEntryView>();
    const add = vi.spyOn(api, 'addTimeEntry').mockReturnValue(request.promise);
    render(TimeScreen);
    const memo = await screen.findByLabelText(m.time_memo());
    await fireEvent.input(memo, { target: { value: 'manual' } });
    await fireEvent.click(screen.getByRole('button', { name: m.time_add_entry() }));
    await waitFor(() => {
      expect(add).toHaveBeenCalledOnce();
    });
    expect(memo).toBeDisabled();
    // 잠금을 지나 입력이 바뀌었어도(같은 사용자) 실패는 알린다.
    await fireEvent.input(memo, { target: { value: 'manual changed' } });
    request.reject(new NetworkError('offline'));
    expect(await screen.findByRole('alert')).toHaveTextContent(m.time_manual_failed());
  });
});
