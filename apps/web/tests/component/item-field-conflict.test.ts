import { taskData as work } from '$lib/features/tasks/detail-data';
import './fixtures/task-statuses';
import type { Module, Workspace } from '@fvoci/api-client';
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { api, ApiError, type ItemProfileView, type ItemView } from '$lib/api/client';
import * as feedback from '$lib/features/feedback/feedback.svelte';
import ItemProfilePanel from '$lib/features/modules/ItemProfilePanel.svelte';
import ItemScreen from '$lib/features/modules/ItemScreen.svelte';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import { router } from '$lib/router.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';
import { openItemInfo, resetRoute } from './fixtures/item-groups';

// 항목 정보·프로필 충돌(D-94 개정, 승인 6568773043): 첫 409에서 공통 비교 부품으로 칸마다 고른다. 양쪽이 바꾼 칸은
// 미선택·맨 위, 나만 바꾼 칸은 내 입력, 서버만 바꾼 칸은 접힌 목록(저장을 막지 않음). 검토한 판으로 저장하고, 다시 바뀌면
// 바로 다시 고른다(자동 재전송 없음). 원래 입력·후보는 계정별 초안으로 보존한다.

const workspace: Workspace = {
  access: 'owner',
  id: 'ws-1',
  organization_id: 'org-1',
  name: '공간',
  role: 'admin',
  is_owner: true,
};
const mod: Module = {
  access: 'owner',
  id: 'm1',
  workspace_id: 'ws-1',
  kind: 'project',
  name: '개발',
  key: 'DEV',
  previous_keys: [],
  archived: false,
  icon: '',
  description: '',
};
function item(over: Partial<ItemView> = {}): ItemView {
  return {
    id: 'i1',
    module_id: 'm1',
    workspace_id: 'ws-1',
    reference: 'DEV-1',
    title: '처음 제목',
    description: '처음 설명',
    visibility: 'private',
    access: 'owner',
    archived: false,
    updated_at: '2026-11-02T00:00:00Z',
    status: 'active',
    is_template: false,
    version: 1,
    starts_on: null,
    ends_on: null,
    ...over,
  };
}
const profile: ItemProfileView = {
  leader: null,
  goal: '처음 목표',
  scope: '처음 범위',
  starts_on: '2026-11-01',
  ends_on: '2026-11-30',
  status: 'active',
  details: {},
  members: [],
  groups: [],
  can_edit: true,
  can_manage_members: false,
  version: 1,
};
const REGION = m.common_conflict_heading();

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  localStorage.clear();
  me.userId = 'user-a';
  vi.stubGlobal('fetch', () => new Promise<Response>(() => undefined));
  vi.spyOn(api, 'module').mockResolvedValue(mod);
  vi.spyOn(api, 'itemDocuments').mockResolvedValue([]);
  vi.spyOn(api, 'itemProfile').mockResolvedValue(profile);
  vi.spyOn(work, 'assignable').mockResolvedValue([]);
  Element.prototype.hasPointerCapture = (): boolean => false;
  Element.prototype.setPointerCapture = (): void => undefined;
  Element.prototype.releasePointerCapture = (): void => undefined;
  Element.prototype.scrollIntoView = (): void => undefined;
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  localStorage.clear();
  sessionStorage.clear();
  resetRoute();
  me.userId = '';
});

function choose(panel: HTMLElement, label: string, side: 'mine' | 'server') {
  const value = side === 'mine' ? m.common_conflict_use_mine() : m.common_conflict_use_server();
  return fireEvent.click(within(panel).getByRole('radio', { name: `${label}: ${value}` }));
}
const saveChosen = (panel: HTMLElement) =>
  within(panel).getByRole('button', { name: m.common_conflict_save() });
const infoForm = () => screen.getByRole('form', { name: m.modules_item_info() });
const title = () => screen.getByLabelText(m.modules_title());
const description = () => screen.getByLabelText(m.modules_description());
const shared = () => screen.getByRole('checkbox', { name: m.modules_share_with_members() });

async function mountInfo(read: ItemView = item()) {
  const reads = vi.spyOn(api, 'item').mockResolvedValue(read);
  const view = render(ItemScreen, {
    props: { workspace, itemId: read.id, workspaces: [workspace] },
  });
  await openItemInfo();
  await waitFor(() => {
    expect(title()).toHaveValue(read.title);
  });
  return { ...view, reads };
}
async function typeInfo(next: { title?: string; description?: string }) {
  if (next.title !== undefined) await fireEvent.input(title(), { target: { value: next.title } });
  if (next.description !== undefined)
    await fireEvent.input(description(), { target: { value: next.description } });
}
async function submitInfo() {
  await fireEvent.click(within(infoForm()).getByRole('button', { name: m.common_save() }));
}

describe('item information conflict', () => {
  it('opens the comparison on the first 409 without another read and saves mixed choices on the reviewed version', async () => {
    const server = item({ title: '서버 제목', description: '처음 설명', version: 2 });
    const update = vi
      .spyOn(api, 'updateItem')
      .mockResolvedValueOnce({ kind: 'conflict', current: server })
      .mockResolvedValueOnce({
        kind: 'updated',
        item: item({ title: '서버 제목', description: '내 설명', version: 3 }),
      });
    const done = vi.spyOn(feedback, 'notifyDone');
    const { reads } = await mountInfo();
    await typeInfo({ title: '내 제목', description: '내 설명' });
    await submitInfo();
    const panel = await screen.findByRole('region', { name: REGION });
    expect(within(panel).getByRole('heading', { name: REGION })).toHaveFocus();
    expect(reads).toHaveBeenCalledTimes(1);
    expect(screen.queryByText('그사이 다른 사람이', { exact: false })).not.toBeInTheDocument();
    // 제목은 양쪽이 바꿔 미선택, 설명은 나만 바꿔 내 입력이 골라져 있다.
    expect(
      within(panel).getByRole('radio', {
        name: `${m.modules_title()}: ${m.common_conflict_use_mine()}`,
      }),
    ).not.toBeChecked();
    expect(
      within(panel).getByRole('radio', {
        name: `${m.modules_title()}: ${m.common_conflict_use_server()}`,
      }),
    ).not.toBeChecked();
    expect(
      within(panel).getByRole('radio', {
        name: `${m.modules_description()}: ${m.common_conflict_use_mine()}`,
      }),
    ).toBeChecked();
    await choose(panel, m.modules_title(), 'server');
    await fireEvent.click(saveChosen(panel));
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(2);
    });
    expect(update.mock.calls[1]).toEqual([
      'i1',
      2,
      { title: '서버 제목', description: '내 설명' },
      'user-a',
    ]);
    await waitFor(() => {
      expect(screen.queryByRole('region', { name: REGION })).not.toBeInTheDocument();
    });
    expect(done).toHaveBeenCalledWith(m.common_saved());
    expect(reads).toHaveBeenCalledTimes(1);
  });

  it('keeps server-only changes folded, does not block the save and keeps them', async () => {
    const server = item({ description: '서버 설명', visibility: 'workspace', version: 2 });
    const update = vi
      .spyOn(api, 'updateItem')
      .mockResolvedValueOnce({ kind: 'conflict', current: server })
      .mockResolvedValueOnce({
        kind: 'updated',
        item: { ...server, title: '내 제목', version: 3 },
      });
    await mountInfo();
    await typeInfo({ title: '내 제목' });
    await submitInfo();
    const panel = await screen.findByRole('region', { name: REGION });
    const folded = within(panel).getByText(m.common_conflict_server_changes({ count: 2 }));
    expect(folded.closest('details')).not.toHaveAttribute('open');
    expect(
      within(panel).queryByRole('radio', {
        name: `${m.modules_description()}: ${m.common_conflict_use_mine()}`,
      }),
    ).toBeNull();
    await fireEvent.click(saveChosen(panel));
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(2);
    });
    // 공개 범위는 서버 값이 기준이 되었으므로 보내지 않는다(서버 값 유지).
    expect(update.mock.calls[1]).toEqual([
      'i1',
      2,
      { title: '내 제목', description: '서버 설명' },
      'user-a',
    ]);
  });

  it('moves focus to the first unchosen row and tells how many remain', async () => {
    const server = item({ title: '서버 제목', description: '서버 설명', version: 2 });
    const update = vi
      .spyOn(api, 'updateItem')
      .mockResolvedValueOnce({ kind: 'conflict', current: server });
    await mountInfo();
    await typeInfo({ title: '내 제목', description: '내 설명' });
    await submitInfo();
    const panel = await screen.findByRole('region', { name: REGION });
    await fireEvent.click(saveChosen(panel));
    expect(within(panel).getByRole('alert')).toHaveTextContent(
      m.common_conflict_missing({ count: 2 }),
    );
    await waitFor(() => {
      expect(
        within(panel).getByRole('radio', {
          name: `${m.modules_title()}: ${m.common_conflict_use_mine()}`,
        }),
      ).toHaveFocus();
    });
    expect(update).toHaveBeenCalledOnce();
  });

  it('asks again right away when it changed again and keeps the first input to restore', async () => {
    const second = item({ title: '서버 제목', version: 2 });
    const third = item({ title: '또 다른 제목', version: 3 });
    const update = vi
      .spyOn(api, 'updateItem')
      .mockResolvedValueOnce({ kind: 'conflict', current: second })
      .mockResolvedValueOnce({ kind: 'conflict', current: third });
    await mountInfo();
    await typeInfo({ title: '내 제목', description: '내 설명' });
    await submitInfo();
    let panel = await screen.findByRole('region', { name: REGION });
    await choose(panel, m.modules_title(), 'server');
    await fireEvent.click(saveChosen(panel));
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(2);
    });
    panel = await screen.findByRole('region', { name: REGION });
    await waitFor(() => {
      expect(within(panel).getByText(m.common_conflict_repeated())).toBeInTheDocument();
    });
    expect(update.mock.calls[1]).toEqual([
      'i1',
      2,
      { title: '서버 제목', description: '내 설명' },
      'user-a',
    ]);
    // 다시 고를 때까지 보내지 않는다.
    expect(update).toHaveBeenCalledTimes(2);
    // 재충돌의 기준은 검토한 판(서버 제목)이라 제목은 서버만 바꾼 칸(접힌 목록)이고 설명은 내가 바꾼 칸이다.
    expect(within(panel).getByText('또 다른 제목')).toBeInTheDocument();
    expect(
      within(panel).getByRole('radio', {
        name: `${m.modules_description()}: ${m.common_conflict_use_mine()}`,
      }),
    ).toBeChecked();
    // 처음 입력(첫 409 앞의 내 제목)으로 돌아갈 수 있다.
    await fireEvent.click(
      within(panel).getByRole('button', { name: m.common_conflict_original() }),
    );
    expect(title()).toHaveValue('내 제목');
    expect(description()).toHaveValue('내 설명');
    expect(screen.queryByRole('region', { name: REGION })).not.toBeInTheDocument();
  });

  it('keeps the comparison, my input and the candidate when the chosen save fails', async () => {
    const server = item({ title: '서버 제목', version: 2 });
    const update = vi
      .spyOn(api, 'updateItem')
      .mockResolvedValueOnce({ kind: 'conflict', current: server })
      .mockRejectedValueOnce(new ApiError(503, 'unavailable', 'down'));
    await mountInfo();
    await typeInfo({ title: '내 제목', description: '내 설명' });
    await submitInfo();
    const panel = await screen.findByRole('region', { name: REGION });
    await choose(panel, m.modules_title(), 'server');
    await fireEvent.click(saveChosen(panel));
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(2);
    });
    expect(await screen.findByRole('region', { name: REGION })).toBeInTheDocument();
    await fireEvent.click(
      within(screen.getByRole('region', { name: REGION })).getByRole('button', {
        name: m.common_conflict_cancel(),
      }),
    );
    expect(title()).toHaveValue('내 제목');
    expect(description()).toHaveValue('내 설명');
    expect(update).toHaveBeenCalledTimes(2);
  });

  it('blocks the save after the permission is lost and hides the values after a 404', async () => {
    const server = item({ title: '서버 제목', version: 2 });
    const update = vi
      .spyOn(api, 'updateItem')
      .mockResolvedValueOnce({ kind: 'conflict', current: server })
      .mockRejectedValueOnce(new ApiError(403, 'forbidden', 'revoked'));
    await mountInfo();
    await typeInfo({ title: '내 제목' });
    await submitInfo();
    let panel = await screen.findByRole('region', { name: REGION });
    await choose(panel, m.modules_title(), 'mine');
    await fireEvent.click(saveChosen(panel));
    panel = await screen.findByRole('region', { name: REGION });
    await within(panel).findByText(m.common_conflict_blocked());
    expect(saveChosen(panel)).toBeDisabled();
    expect(update).toHaveBeenCalledTimes(2);
  });

  it('hides the compared values after a 404', async () => {
    const server = item({ title: '서버 제목', version: 2 });
    vi.spyOn(api, 'updateItem')
      .mockResolvedValueOnce({ kind: 'conflict', current: server })
      .mockRejectedValueOnce(new ApiError(404, 'not_found', 'gone'));
    await mountInfo();
    await typeInfo({ title: '내 제목' });
    await submitInfo();
    const panel = await screen.findByRole('region', { name: REGION });
    await choose(panel, m.modules_title(), 'mine');
    await fireEvent.click(saveChosen(panel));
    await waitFor(() => {
      expect(screen.queryByRole('region', { name: REGION })).not.toBeInTheDocument();
    });
    expect(document.body).not.toHaveTextContent('서버 제목');
  });

  it('rechecks access once after a transient invalid_state and lets the person retry', async () => {
    const server = item({ title: '서버 제목', version: 2 });
    const update = vi
      .spyOn(api, 'updateItem')
      .mockResolvedValueOnce({ kind: 'conflict', current: server })
      .mockRejectedValueOnce(new ApiError(409, 'invalid_state', 'serialization'))
      .mockResolvedValueOnce({
        kind: 'updated',
        item: { ...server, title: '내 제목', version: 3 },
      });
    await mountInfo();
    await typeInfo({ title: '내 제목' });
    await submitInfo();
    const panel = await screen.findByRole('region', { name: REGION });
    await choose(panel, m.modules_title(), 'mine');
    const rechecks = vi.mocked(api.itemProfile).mock.calls.length;
    await fireEvent.click(saveChosen(panel));
    await waitFor(() => {
      expect(vi.mocked(api.itemProfile).mock.calls.length).toBe(rechecks + 1);
    });
    const again = await screen.findByRole('region', { name: REGION });
    expect(within(again).queryByText(m.common_conflict_blocked())).toBeNull();
    expect(saveChosen(again)).toBeEnabled();
    await fireEvent.click(saveChosen(again));
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(3);
    });
    expect(update.mock.calls[2]).toEqual([
      'i1',
      2,
      { title: '내 제목', description: '처음 설명' },
      'user-a',
    ]);
  });

  it('blocks only when the recheck after invalid_state confirms the item is archived', async () => {
    const server = item({ title: '서버 제목', version: 2 });
    vi.spyOn(api, 'updateItem')
      .mockResolvedValueOnce({ kind: 'conflict', current: server })
      .mockRejectedValueOnce(new ApiError(409, 'invalid_state', 'archived'));
    await mountInfo();
    await typeInfo({ title: '내 제목' });
    await submitInfo();
    const panel = await screen.findByRole('region', { name: REGION });
    await choose(panel, m.modules_title(), 'mine');
    vi.mocked(api.itemProfile).mockResolvedValue({ ...profile, can_edit: false });
    await fireEvent.click(saveChosen(panel));
    const blocked = await screen.findByRole('region', { name: REGION });
    await within(blocked).findByText(m.common_conflict_blocked());
    expect(saveChosen(blocked)).toBeDisabled();
  });

  it('reopens a restored comparison against the newer read instead of saving the stale snapshot', async () => {
    const server = item({ title: '서버 제목', version: 2 });
    const update = vi
      .spyOn(api, 'updateItem')
      .mockResolvedValueOnce({ kind: 'conflict', current: server });
    const first = await mountInfo();
    await typeInfo({ title: '내 제목' });
    await submitInfo();
    await screen.findByRole('region', { name: REGION });
    first.unmount();

    const newer = item({ title: '더 새 제목', version: 3 });
    await mountInfo(newer);
    await fireEvent.click(await screen.findByRole('button', { name: m.common_draft_restore() }));
    const panel = await screen.findByRole('region', { name: REGION });
    await choose(panel, m.modules_title(), 'server');
    await fireEvent.click(saveChosen(panel));
    // 옛 판(2)의 서버 값으로 바꾸거나 판을 낮추지 않고, 새로 읽은 판(3)과 다시 비교한다.
    const again = await screen.findByRole('region', { name: REGION });
    await waitFor(() => {
      expect(within(again).getAllByDisplayValue('더 새 제목').length).toBeGreaterThan(0);
    });
    expect(update).toHaveBeenCalledOnce();
    await choose(again, m.modules_title(), 'mine');
    update.mockResolvedValueOnce({
      kind: 'updated',
      item: { ...newer, title: '내 제목', version: 4 },
    });
    await fireEvent.click(saveChosen(again));
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(2);
    });
    expect(update.mock.calls[1]?.[1]).toBe(3);
  });

  it('does not restore the comparison of another item or show it to another account', async () => {
    const server = item({ title: '서버 제목', version: 2 });
    vi.spyOn(api, 'updateItem').mockResolvedValueOnce({ kind: 'conflict', current: server });
    const first = await mountInfo();
    await typeInfo({ title: '내 제목' });
    await submitInfo();
    await screen.findByRole('region', { name: REGION });
    first.unmount();
    // 다른 항목 키로 옮겨 둔 초안(같은 계정)도 그 항목의 비교로 되살리지 않는다.
    for (const key of Object.keys(localStorage))
      if (key.includes('item-info:i1')) {
        const value = localStorage.getItem(key);
        if (value !== null)
          localStorage.setItem(key.replace('item-info:i1', 'item-info:i2'), value);
      }
    const other = await mountInfo(item({ id: 'i2', title: '다른 항목' }));
    const offer = screen.queryByRole('button', { name: m.common_draft_restore() });
    if (offer !== null) await fireEvent.click(offer);
    expect(screen.queryByRole('region', { name: REGION })).not.toBeInTheDocument();
    expect(document.body).not.toHaveTextContent('서버 제목');
    other.unmount();

    me.userId = 'user-b';
    await mountInfo();
    expect(
      screen.queryByRole('button', { name: m.common_draft_restore() }),
    ).not.toBeInTheDocument();
    expect(screen.queryByRole('region', { name: REGION })).not.toBeInTheDocument();
    expect(document.body).not.toHaveTextContent('서버 제목');
    expect(document.body).not.toHaveTextContent('내 제목');
  });

  it('leaves no stored comparison after discarding the open comparison', async () => {
    const server = item({ title: '서버 제목', version: 2 });
    vi.spyOn(api, 'updateItem').mockResolvedValueOnce({ kind: 'conflict', current: server });
    await mountInfo();
    await typeInfo({ title: '내 제목' });
    await submitInfo();
    const panel = await screen.findByRole('region', { name: REGION });
    expect(
      Object.values(localStorage).some((v) => typeof v === 'string' && v.includes('choices')),
    ).toBe(true);
    // 비교가 열린 채 다른 화면으로 가려다 "변경 버리고 닫기"를 고르면 비교도 초안에 남지 않는다(CR-305-2).
    expect(panel).toBeInTheDocument();
    router.navigate('/elsewhere');
    await fireEvent.click(await screen.findByRole('button', { name: m.common_discard_confirm() }));
    await waitFor(() => {
      expect(screen.queryByRole('region', { name: REGION })).not.toBeInTheDocument();
    });
    expect(
      Object.values(localStorage).some((v) => typeof v === 'string' && v.includes('choices')),
    ).toBe(false);
    expect(shared()).not.toBeChecked();
  });
});

async function mountProfile(read: ItemProfileView = profile, kind: Module['kind'] = 'project') {
  vi.mocked(api.itemProfile).mockResolvedValue(read);
  const onchanged = vi.fn();
  const view = render(ItemProfilePanel, { props: { item: item(), kind, onchanged } });
  await fireEvent.click(await screen.findByRole('button', { name: m.modules_edit() }));
  return { ...view, onchanged };
}
const goal = () => document.querySelector<HTMLTextAreaElement>('#item-goal');
const starts = () => document.querySelector<HTMLInputElement>('#item-starts');

describe('item profile conflict', () => {
  it('opens on the first 409 and saves the period bundle and goal choices on the reviewed version', async () => {
    const server: ItemProfileView = {
      ...profile,
      goal: '서버 목표',
      starts_on: '2026-11-05',
      ends_on: '2026-12-05',
      version: 2,
    };
    const save = vi
      .spyOn(api, 'setItemProfile')
      .mockResolvedValueOnce({ kind: 'conflict', current: server })
      .mockResolvedValueOnce({ kind: 'saved' });
    const { onchanged } = await mountProfile();
    const reads = vi.mocked(api.itemProfile).mock.calls.length;
    const g = goal();
    const s = starts();
    if (g === null || s === null) throw new Error('profile fields');
    await fireEvent.input(g, { target: { value: '내 목표' } });
    await fireEvent.input(s, { target: { value: '2026-11-03' } });
    await fireEvent.click(screen.getByRole('button', { name: m.common_save() }));
    const panel = await screen.findByRole('region', { name: REGION });
    expect(vi.mocked(api.itemProfile).mock.calls.length).toBe(reads);
    expect(onchanged).not.toHaveBeenCalled();
    await choose(panel, m.modules_goal(), 'mine');
    await choose(panel, m.modules_period(), 'server');
    await fireEvent.click(saveChosen(panel));
    await waitFor(() => {
      expect(save).toHaveBeenCalledTimes(2);
    });
    expect(save.mock.calls[1]?.[1]).toBe(2);
    expect(save.mock.calls[1]?.[2]).toMatchObject({
      goal: '내 목표',
      starts_on: '2026-11-05',
      ends_on: '2026-12-05',
      scope: '처음 범위',
    });
    await waitFor(() => {
      expect(onchanged).toHaveBeenCalledOnce();
    });
  });

  it('asks again right away when the profile changed again', async () => {
    const save = vi
      .spyOn(api, 'setItemProfile')
      .mockResolvedValueOnce({
        kind: 'conflict',
        current: { ...profile, goal: '서버 목표', version: 2 },
      })
      .mockResolvedValueOnce({
        kind: 'conflict',
        current: { ...profile, goal: '또 목표', version: 3 },
      });
    await mountProfile();
    const g = goal();
    if (g === null) throw new Error('goal');
    await fireEvent.input(g, { target: { value: '내 목표' } });
    await fireEvent.click(screen.getByRole('button', { name: m.common_save() }));
    let panel = await screen.findByRole('region', { name: REGION });
    await choose(panel, m.modules_goal(), 'mine');
    await fireEvent.click(saveChosen(panel));
    await waitFor(() => {
      expect(save).toHaveBeenCalledTimes(2);
    });
    panel = await screen.findByRole('region', { name: REGION });
    await within(panel).findByText(m.common_conflict_repeated());
    expect(save).toHaveBeenCalledTimes(2);
  });

  it('rechecks the profile once after invalid_state and does not lock a still-editable item', async () => {
    const save = vi
      .spyOn(api, 'setItemProfile')
      .mockResolvedValueOnce({
        kind: 'conflict',
        current: { ...profile, goal: '서버 목표', version: 2 },
      })
      .mockRejectedValueOnce(new ApiError(409, 'invalid_state', 'deadlock'));
    await mountProfile();
    const g = goal();
    if (g === null) throw new Error('goal');
    await fireEvent.input(g, { target: { value: '내 목표' } });
    await fireEvent.click(screen.getByRole('button', { name: m.common_save() }));
    const panel = await screen.findByRole('region', { name: REGION });
    await choose(panel, m.modules_goal(), 'mine');
    const reads = vi.mocked(api.itemProfile).mock.calls.length;
    await fireEvent.click(saveChosen(panel));
    await waitFor(() => {
      expect(vi.mocked(api.itemProfile).mock.calls.length).toBe(reads + 1);
    });
    const again = await screen.findByRole('region', { name: REGION });
    expect(within(again).queryByText(m.common_conflict_blocked())).toBeNull();
    expect(saveChosen(again)).toBeEnabled();
    expect(save).toHaveBeenCalledTimes(2);
  });

  it('names the server leader from the 409 even when that person is not assignable', async () => {
    vi.spyOn(api, 'setItemProfile').mockResolvedValueOnce({
      kind: 'conflict',
      current: {
        ...profile,
        leader: { user_id: 'u-x', display_name: '서버 리더' },
        goal: '서버 목표',
        version: 2,
      },
    });
    await mountProfile();
    const g = goal();
    if (g === null) throw new Error('goal');
    await fireEvent.input(g, { target: { value: '내 목표' } });
    await fireEvent.click(screen.getByRole('button', { name: m.common_save() }));
    const panel = await screen.findByRole('region', { name: REGION });
    expect(within(panel).getByText('서버 리더')).toBeInTheDocument();
  });

  it('leaves no stored comparison after canceling the edit with an open comparison', async () => {
    vi.spyOn(api, 'setItemProfile').mockResolvedValueOnce({
      kind: 'conflict',
      current: { ...profile, goal: '서버 목표', version: 2 },
    });
    await mountProfile();
    const g = goal();
    if (g === null) throw new Error('goal');
    await fireEvent.input(g, { target: { value: '내 목표' } });
    await fireEvent.click(screen.getByRole('button', { name: m.common_save() }));
    const panel = await screen.findByRole('region', { name: REGION });
    expect(
      Object.values(localStorage).some((v) => typeof v === 'string' && v.includes('choices')),
    ).toBe(true);
    // 비교를 닫아도 저장하지 않은 비교는 초안에 남는다. 편집을 취소하고 버리면 남지 않는다(CR-305-2).
    await fireEvent.click(within(panel).getByRole('button', { name: m.common_conflict_cancel() }));
    expect(
      Object.values(localStorage).some((v) => typeof v === 'string' && v.includes('choices')),
    ).toBe(true);
    await fireEvent.click(screen.getByRole('button', { name: m.common_cancel() }));
    await fireEvent.click(await screen.findByRole('button', { name: m.common_discard_confirm() }));
    await waitFor(() => {
      expect(screen.queryByRole('region', { name: REGION })).not.toBeInTheDocument();
    });
    expect(
      Object.values(localStorage).some((v) => typeof v === 'string' && v.includes('choices')),
    ).toBe(false);
  });
});

const storedChoices = () =>
  Object.values(localStorage).some((v) => typeof v === 'string' && v.includes('choices'));

// CR-306-1~3(D-128, D-96, 승인 6568773043): 고른 값이 모두 서버 값이면 저장 없이 닫아도 상위가 항목을 다시 읽는다.
// 404 뒤에는 비교값을 숨기고 저장을 막은 채, 폼을 잠그지 않아 입력에 닿고 편집을 닫을 수 있다.
describe('item conflict after an equal choice or a lost item', () => {
  it('rereads the item when every chosen profile value is the server value (CR-306-1)', async () => {
    // 다른 탭이 상태를 완료로, 목표를 서버 목표로 바꿨다. 이 탭은 목표만 고쳤다.
    const server: ItemProfileView = { ...profile, goal: '서버 목표', status: 'done', version: 2 };
    const save = vi
      .spyOn(api, 'setItemProfile')
      .mockResolvedValueOnce({ kind: 'conflict', current: server });
    const done = vi.spyOn(feedback, 'notifyDone');
    const { reads } = await mountInfo();
    reads.mockResolvedValue(item({ status: 'done', version: 2 }));
    await fireEvent.click(await screen.findByRole('button', { name: m.modules_edit() }));
    const g = goal();
    const form = g?.closest('form');
    if (g === null || form === null || form === undefined) throw new Error('profile form');
    await fireEvent.input(g, { target: { value: '내 목표' } });
    await fireEvent.click(within(form).getByRole('button', { name: m.common_save() }));
    const panel = await screen.findByRole('region', { name: REGION });
    await choose(panel, m.modules_goal(), 'server');
    await fireEvent.click(saveChosen(panel));
    await waitFor(() => {
      expect(done).toHaveBeenCalledWith(m.common_conflict_equal());
    });
    expect(save).toHaveBeenCalledOnce();
    expect(goal()).toBeNull();
    // 보낼 것은 없지만 상태·기간이 바뀌었을 수 있어 항목을 다시 읽고 머리 요약이 새 상태를 보인다.
    await waitFor(() => {
      expect(reads).toHaveBeenCalledTimes(2);
    });
    await waitFor(() => {
      expect(document.getElementById('item-info-profile-status')).toHaveTextContent(
        m.modules_status_done(),
      );
    });
  });

  it('lets the profile edit be canceled after a 404 while the save stays blocked (CR-306-2)', async () => {
    const save = vi
      .spyOn(api, 'setItemProfile')
      .mockResolvedValueOnce({
        kind: 'conflict',
        current: { ...profile, goal: '서버 목표', version: 2 },
      })
      .mockRejectedValueOnce(new ApiError(404, 'not_found', 'gone'));
    await mountProfile();
    const g = goal();
    if (g === null) throw new Error('goal');
    await fireEvent.input(g, { target: { value: '내 목표' } });
    await fireEvent.click(screen.getByRole('button', { name: m.common_save() }));
    const panel = await screen.findByRole('region', { name: REGION });
    await choose(panel, m.modules_goal(), 'mine');
    await fireEvent.click(saveChosen(panel));
    await waitFor(() => {
      expect(screen.queryByRole('region', { name: REGION })).not.toBeInTheDocument();
    });
    expect(save).toHaveBeenCalledTimes(2);
    expect(document.body).not.toHaveTextContent('서버 목표');
    const form = goal()?.closest('form');
    if (form === null || form === undefined) throw new Error('profile form');
    // 숨긴 비교가 폼을 잠그면 취소 단추에 닿을 수 없다(Svelte는 jsdom에서 속성이 아니라 `inert` 프로퍼티를 쓴다).
    expect(form.inert).toBe(false);
    expect(goal()).toHaveValue('내 목표');
    await fireEvent.click(within(form).getByRole('button', { name: m.common_save() }));
    expect(save).toHaveBeenCalledTimes(2);
    await fireEvent.click(within(form).getByRole('button', { name: m.common_cancel() }));
    await fireEvent.click(await screen.findByRole('button', { name: m.common_discard_confirm() }));
    await waitFor(() => {
      expect(goal()).toBeNull();
    });
    expect(storedChoices()).toBe(false);
    expect(save).toHaveBeenCalledTimes(2);
  });

  it('unlocks the information form after a 404 and still sends nothing (CR-306-3)', async () => {
    const update = vi
      .spyOn(api, 'updateItem')
      .mockResolvedValueOnce({
        kind: 'conflict',
        current: item({ title: '서버 제목', version: 2 }),
      })
      .mockRejectedValueOnce(new ApiError(404, 'not_found', 'gone'));
    await mountInfo();
    await typeInfo({ title: '내 제목' });
    await submitInfo();
    const panel = await screen.findByRole('region', { name: REGION });
    await choose(panel, m.modules_title(), 'mine');
    await fireEvent.click(saveChosen(panel));
    await waitFor(() => {
      expect(screen.queryByRole('region', { name: REGION })).not.toBeInTheDocument();
    });
    expect(update).toHaveBeenCalledTimes(2);
    // 숨긴 비교가 폼을 잠그면 입력에도, 저장 단추에도 닿을 수 없다(정보 폼에는 취소 단추가 없다).
    expect(infoForm().inert).toBe(false);
    expect(title()).toHaveValue('내 제목');
    await submitInfo();
    expect(update).toHaveBeenCalledTimes(2);
    // 숨은 비교는 이탈 확인에서 버린다.
    router.navigate('/elsewhere');
    await fireEvent.click(await screen.findByRole('button', { name: m.common_discard_confirm() }));
    await waitFor(() => {
      expect(storedChoices()).toBe(false);
    });
    expect(update).toHaveBeenCalledTimes(2);
  });
});
