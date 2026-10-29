import { type EventView } from '$lib/features/events/model';
import { eventsApi } from '$lib/features/events/api';
import type { TimeEntryView } from '$lib/features/time/model';
import { timeData } from '$lib/features/time/data';
import { taskData as work } from '$lib/features/tasks/detail-data';
import type { Module, Workspace } from '@fvoci/api-client';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { ItemProfileView, ItemView } from '$lib/api/client';
import { api } from '$lib/api/client';
import * as feedback from '$lib/features/feedback/feedback.svelte';
import { formDraftTabKey, writeFormDraftAt } from '$lib/features/common/form-draft';
import ItemEvents from '$lib/features/events/ItemEvents.svelte';
import AttachmentsPanel from '$lib/features/files/AttachmentsPanel.svelte';
import ItemCopy from '$lib/features/modules/ItemCopy.svelte';
import ItemProfilePanel from '$lib/features/modules/ItemProfilePanel.svelte';
import ModuleScreen from '$lib/features/modules/ModuleScreen.svelte';
import TimeEntryDialog from '$lib/features/time/TimeEntryDialog.svelte';
import TimeScreen from '$lib/features/time/TimeScreen.svelte';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';
import { itemPath, router } from '$lib/router.svelte';
import { ACCESS_FILE } from '../../stories/fixtures/access';

// D2-F-5(R5 2차): F-16·F-8 처분 (1)은 저장 중 입력과 **초안 되살리기**를 잠근다. 다른 탭에서 남긴 초안을 저장 중에
// 되살리면 스냅숏이 바뀌어 성공을 버리고(만들기는 다시 내면 새 요청 id로 하나 더 생김, 고치기는 기준 판을 못 옮김) 만다.
// `save-input-lock.test.ts`는 입력 칸만 보므로, 여기서는 대상마다 다른 탭 초안의 되살리기·버리기 단추가 저장 중에
// 눌리지 않는지 본다.

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

function held<T>(): { promise: Promise<T>; resolve: (value: T) => void } {
  let resolve: ((value: T) => void) | undefined;
  const promise = new Promise<T>((ok) => {
    resolve = ok;
  });
  if (resolve === undefined) throw new Error('promise');
  return { promise, resolve };
}

/** 같은 사용자의 다른 탭이 남긴 초안(이 탭에는 되살리기 제안으로 보인다). */
function otherTabDraft(name: string, input: unknown): void {
  const own = formDraftTabKey(name, 'user-a');
  if (own === undefined) throw new Error('draft key');
  writeFormDraftAt(own.replace(/:tab:.*$/, ':tab:other-tab'), {
    savedAt: '2026-10-27T00:00:00.000Z',
    input,
  });
}

function expectOfferLocked(): void {
  expect(screen.getByRole('button', { name: m.common_draft_restore() })).toBeDisabled();
  expect(screen.getByRole('button', { name: m.common_draft_discard() })).toBeDisabled();
}

const workspace: Workspace = {
  id: 'ws',
  organization_id: 'org-1',
  name: '공간',
  access: 'owner',
  role: 'admin',
  is_owner: true,
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

describe('draft offers while a create request is in flight', () => {
  it('locks the item copy offer', async () => {
    otherTabDraft(`item-copy:${item.id}`, {
      title: '다른 탭 사본',
      target: mod.id,
      reset: true,
      pending: null,
    });
    vi.spyOn(api, 'modules').mockResolvedValue([mod]);
    const done = vi.spyOn(feedback, 'notifyDone');
    const request = held<Awaited<ReturnType<typeof api.copyItem>>>();
    const copy = vi.spyOn(api, 'copyItem').mockReturnValue(request.promise);
    render(ItemCopy, { item, module: mod });
    await screen.findByTestId('form-draft-offer');
    await fireEvent.click(screen.getByRole('button', { name: m.modules_copy() }));
    await screen.findByLabelText(m.modules_copy_new_title());
    await fireEvent.click(screen.getByRole('button', { name: m.modules_copy() }));
    await waitFor(() => {
      expect(copy).toHaveBeenCalledOnce();
    });
    expectOfferLocked();
    request.resolve({ item_id: 'copied', skipped_documents: 0 });
    // 일반 완료는 토스트이고 새 항목 열기는 그 동작 단추다(D-82).
    await waitFor(() => {
      expect(done).toHaveBeenCalledOnce();
    });
    const [message, key, options] = done.mock.calls[0] ?? [];
    expect(message).toBe(m.modules_copy_done());
    expect(key).toBe('item-copy:copied');
    expect(options?.action?.label).toBe(m.modules_copy_open());
    const navigate = vi.spyOn(router, 'navigate').mockImplementation(() => undefined);
    options?.action?.onClick(new MouseEvent('click', { cancelable: true }));
    expect(navigate).toHaveBeenCalledWith(itemPath(item.workspace_id, 'copied'));
    expect(screen.getByRole('button', { name: m.common_draft_restore() })).not.toBeDisabled();
  });

  it('locks the create-from-template offer', async () => {
    otherTabDraft(`module-template:${mod.id}`, {
      title: '다른 탭 항목',
      source: '',
      pending: null,
    });
    vi.stubGlobal('fetch', () => new Promise<Response>(() => undefined));
    vi.spyOn(api, 'module').mockResolvedValue(mod);
    vi.spyOn(api, 'items').mockResolvedValue([{ ...item, id: 'template' }]);
    vi.spyOn(router, 'navigate').mockImplementation(() => undefined);
    const copy = vi.spyOn(api, 'copyItem').mockReturnValue(new Promise(() => undefined));
    render(ModuleScreen, { workspace, moduleId: mod.id });
    const title = await screen.findByLabelText(m.modules_template_item_title());
    await fireEvent.input(title, { target: { value: '새 항목' } });
    await fireEvent.click(screen.getByRole('button', { name: m.modules_from_template() }));
    await waitFor(() => {
      expect(copy).toHaveBeenCalledOnce();
    });
    expectOfferLocked();
  });

  it('locks the new event offer', async () => {
    const fields = {
      kind: 'meeting',
      title: '다른 탭 일정',
      description: '',
      allDay: false,
      startDate: '2026-10-27',
      endDate: '2026-10-27',
      startWall: '2026-10-27T09:00',
      endWall: '2026-10-27T10:00',
      timezone: 'UTC',
    };
    otherTabDraft(`event-new:${item.id}`, {
      fields,
      baseFields: { ...fields, title: '' },
      base: null,
      startChoice: null,
      endChoice: null,
      request: null,
    });
    vi.spyOn(eventsApi, 'itemEvents').mockResolvedValue([]);
    const create = vi
      .spyOn(eventsApi, 'createEvent')
      .mockReturnValue(new Promise<EventView>(() => undefined));
    render(ItemEvents, { item: { ...item, is_template: false } });
    await fireEvent.click(screen.getByRole('button', { name: m.events_add() }));
    await screen.findByTestId('form-draft-offer');
    await fireEvent.input(screen.getByLabelText(m.events_title()), { target: { value: '회의' } });
    await fireEvent.click(screen.getByRole('button', { name: m.events_create() }));
    await waitFor(() => {
      expect(create).toHaveBeenCalledOnce();
    });
    expectOfferLocked();
  });
});

describe('draft offers while an edit request is in flight', () => {
  it('locks the item profile offer', async () => {
    const form = {
      status: 'active',
      leader: '',
      goal: '다른 탭 목표',
      scope: '',
      starts_on: '',
      ends_on: '',
      keywords: '',
      values: {},
    };
    otherTabDraft(`item-profile:${item.id}:project`, {
      form,
      base: 1,
      baseline: JSON.stringify({ ...form, goal: '' }),
      leader: null,
    });
    vi.spyOn(api, 'itemProfile').mockResolvedValue(profile);
    vi.spyOn(work, 'assignable').mockResolvedValue([]);
    const save = vi.spyOn(api, 'setItemProfile').mockReturnValue(new Promise(() => undefined));
    render(ItemProfilePanel, { item, kind: 'project', onchanged: vi.fn() });
    await fireEvent.click(await screen.findByRole('button', { name: m.modules_edit() }));
    await fireEvent.input(screen.getByLabelText(m.modules_goal()), { target: { value: '목표' } });
    await fireEvent.click(screen.getByRole('button', { name: m.common_save() }));
    await waitFor(() => {
      expect(save).toHaveBeenCalledOnce();
    });
    expectOfferLocked();
  });

  it('locks the time entry offer', async () => {
    otherTabDraft(`time-entry:${entry.id}`, {
      form: {
        start: '2026-10-20T04:00',
        end: '2026-10-20T05:00',
        pausedMinutes: '0',
        kind: 'work',
        memo: '다른 탭',
        target: null,
      },
      base: entry,
      initial: { start: '2026-10-20T04:00', end: '2026-10-20T05:00', pausedMinutes: '0' },
      zone: 'UTC',
    });
    vi.spyOn(timeData, 'timeEntryRevisions').mockResolvedValue([]);
    const update = vi
      .spyOn(timeData, 'updateTimeEntry')
      .mockReturnValue(new Promise(() => undefined));
    render(TimeEntryDialog, { entry, zone: 'UTC', onclose: vi.fn(), onchanged: vi.fn() });
    await screen.findByTestId('time-entry-draft-offer');
    await fireEvent.input(screen.getByLabelText(m.time_memo()), { target: { value: '이 탭' } });
    await fireEvent.click(screen.getByRole('button', { name: m.common_save() }));
    await waitFor(() => {
      expect(update).toHaveBeenCalledOnce();
    });
    expectOfferLocked();
  });

  it('locks the manual time offer', async () => {
    otherTabDraft('time-manual', {
      form: { day: '2026-10-20', start: '09:00', end: '10:00', kind: 'work', memo: '다른 탭' },
      zone: 'UTC',
      pending: null,
    });
    vi.spyOn(timeData, 'timeEntries').mockResolvedValue([]);
    vi.spyOn(timeData, 'timeSummary').mockResolvedValue([]);
    vi.spyOn(timeData, 'currentTimer').mockResolvedValue(null);
    const add = vi
      .spyOn(timeData, 'addTimeEntry')
      .mockReturnValue(new Promise<TimeEntryView>(() => undefined));
    render(TimeScreen);
    await screen.findByTestId('time-manual-draft-offer');
    await fireEvent.input(screen.getByLabelText(m.time_memo()), { target: { value: '이 탭' } });
    await fireEvent.click(screen.getByRole('button', { name: m.time_add_entry() }));
    await waitFor(() => {
      expect(add).toHaveBeenCalledOnce();
    });
    expectOfferLocked();
  });
});

// FC-2(R5 P1 F-11): 쓸 수 없게 된 대상에는 다른 탭·이전에 남긴 입력의 되살리기 대신 읽기 전용 안내와 버리기만 보인다.
// 되살리면 저장할 수 없는 편집 상태가 열렸다(ItemProfilePanel은 편집을 열었다).
describe('draft offers after write access is withdrawn', () => {
  async function expectReadOnlyOffer(): Promise<void> {
    const offer = await screen.findByTestId('form-draft-offer');
    expect(within(offer).queryByRole('button', { name: m.common_draft_restore() })).toBeNull();
    expect(offer).toHaveTextContent(m.common_draft_read_only());
    await fireEvent.click(within(offer).getByRole('button', { name: m.common_draft_discard() }));
    expect(screen.queryByTestId('form-draft-offer')).toBeNull();
  }

  it('shows the item profile offer read-only', async () => {
    const form = {
      status: 'active',
      leader: '',
      goal: '권한 철회 전 목표',
      scope: '',
      starts_on: '',
      ends_on: '',
      keywords: '',
      values: {},
    };
    otherTabDraft(`item-profile:${item.id}:project`, {
      form,
      base: 1,
      baseline: JSON.stringify({ ...form, goal: '' }),
      leader: null,
    });
    vi.spyOn(api, 'itemProfile').mockResolvedValue({ ...profile, can_edit: false });
    const assignable = vi.spyOn(work, 'assignable').mockResolvedValue([]);
    render(ItemProfilePanel, { item, kind: 'project', onchanged: vi.fn() });
    await expectReadOnlyOffer();
    expect(screen.queryByLabelText(m.modules_goal())).toBeNull();
    expect(assignable).not.toHaveBeenCalled();
  });

  it('shows the create-from-template offer read-only', async () => {
    otherTabDraft(`module-template:${mod.id}`, {
      title: '권한 철회 전 항목',
      source: '',
      pending: null,
    });
    vi.stubGlobal('fetch', () => new Promise<Response>(() => undefined));
    vi.spyOn(api, 'module').mockResolvedValue(mod);
    vi.spyOn(api, 'items').mockResolvedValue([{ ...item, id: 'template' }]);
    render(ModuleScreen, { workspace: { ...workspace, access: 'read' }, moduleId: mod.id });
    await expectReadOnlyOffer();
  });

  // RB-1: 설정 폼은 관리자에게만 그린다. 쓰기 등급이면 설정 초안을 되살려도 보이지 않는 입력이 채워져 떠날 때 묻는다.
  it('shows the module settings offer read-only to a non-admin writer', async () => {
    otherTabDraft(`module-settings:${mod.id}`, {
      name: '강등 전 이름',
      key: 'DEV',
      icon: '',
      description: '',
    });
    vi.stubGlobal('fetch', () => new Promise<Response>(() => undefined));
    vi.spyOn(api, 'module').mockResolvedValue(mod);
    vi.spyOn(api, 'items').mockResolvedValue([]);
    render(ModuleScreen, { workspace: { ...workspace, access: 'write' }, moduleId: mod.id });
    await screen.findByRole('heading', { name: mod.name });
    expect(screen.queryByLabelText(m.modules_name())).toBeNull();
    await expectReadOnlyOffer();
    const unload = new Event('beforeunload', { cancelable: true });
    window.dispatchEvent(unload);
    expect(unload.defaultPrevented).toBe(false);
  });

  // D-27·D-113: 옛 이름 초안은 무시하고 직접 편집은 파일의 can_write를 따른다.
  it.each([false, true])(
    'ignores legacy attachment-name offers while file can_write=%s',
    async (writable) => {
      const oldName = '옛 이름 초안.pdf';
      otherTabDraft('attachment-rename:item:i1', {
        id: ACCESS_FILE.id,
        name: oldName,
        baseline: ACCESS_FILE.name,
      });
      vi.spyOn(api, 'itemFiles').mockResolvedValue([{ ...ACCESS_FILE, can_write: writable }]);
      const rename = vi.spyOn(api, 'renameFile');
      render(AttachmentsPanel, {
        props: { target: { kind: 'item', id: 'i1' }, canWrite: !writable },
      });
      await screen.findByText(ACCESS_FILE.name);
      expect(screen.queryByTestId('form-draft-offer')).toBeNull();
      const buttonName = m.files_rename_aria({ name: ACCESS_FILE.name });
      if (writable) {
        await fireEvent.click(screen.getByRole('button', { name: buttonName }));
        expect(screen.getByLabelText(m.files_new_name())).toHaveValue(ACCESS_FILE.name);
      } else {
        expect(screen.queryByRole('button', { name: buttonName })).toBeNull();
      }
      expect(screen.queryByDisplayValue(oldName)).toBeNull();
      expect(rename).not.toHaveBeenCalled();
    },
  );
});
