import './fixtures/task-statuses';
import type { Module, Workspace } from '@fvoci/api-client';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { ItemProfileView, ItemView, TaskView } from '$lib/api/client';
import { api, ApiError } from '$lib/api/client';
import * as feedback from '$lib/features/feedback/feedback.svelte';
import ItemScreen from '$lib/features/modules/ItemScreen.svelte';
import { itemViewPath, itemTaskPath, router } from '$lib/router.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';
import './fixtures/authenticated';

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
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
    title: '항목',
    description: '원래 설명',
    visibility: 'workspace',
    access: 'owner',
    archived: false,
    updated_at: '2026-10-19T00:00:00Z',
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

/** 항목 화면의 하위 칸(업무·일정·기록 등)이 부르는 나머지 요청은 끝나지 않게 둔다. 이 테스트는 항목 정보만 본다. */
function quietServer(): void {
  vi.stubGlobal('fetch', () => new Promise<Response>(() => undefined));
}

function mockItem(...views: ItemView[]): void {
  const spy = vi.spyOn(api, 'item');
  for (const v of views) spy.mockResolvedValueOnce(v);
  spy.mockResolvedValue(views.at(-1) ?? item());
  vi.spyOn(api, 'module').mockResolvedValue(mod);
  vi.spyOn(api, 'itemDocuments').mockResolvedValue([]);
}

function description(): HTMLTextAreaElement {
  const el = document.getElementById('item-description');
  if (el instanceof HTMLTextAreaElement) return el;
  throw new Error('item-description');
}

function titleInput(): HTMLInputElement {
  const el = document.getElementById('item-title');
  if (el instanceof HTMLInputElement) return el;
  throw new Error('item-title');
}

// 감사 W16: 항목 정보를 저장하거나 템플릿 표시를 바꾸면 항목을 다시 읽는다. 그때 저장하지 않은 제목·설명 입력을
// 서버 값으로 덮어 입력이 사라졌다. 고치지 않은 칸만 새 값으로 바꾸고 입력 중인 칸은 그대로 둔다.
describe('item info form while the item is read again', () => {
  it('keeps a typed description when the template flag is toggled', async () => {
    await setLocale('ko', { reload: false });
    quietServer();
    mockItem(item(), item({ is_template: true, title: '다른 사람이 고친 제목' }));
    vi.spyOn(api, 'setItemTemplate').mockResolvedValue(undefined);
    render(ItemScreen, { props: { workspace, itemId: 'i1', workspaces: [workspace] } });

    await fireEvent.input(await waitFor(description), { target: { value: '쓰는 중인 설명' } });
    await fireEvent.click(await screen.findByRole('button', { name: '템플릿으로 표시' }));
    // 다시 읽은 결과가 화면에 들어온 뒤에 본다.
    await screen.findByRole('button', { name: '템플릿 해제' });

    expect(description().value).toBe('쓰는 중인 설명');
    // 고치지 않은 제목 칸은 다시 읽은 값으로 바뀐다.
    expect(titleInput().value).toBe('다른 사람이 고친 제목');
  });

  it('keeps a typed description when the item profile is saved', async () => {
    await setLocale('ko', { reload: false });
    quietServer();
    mockItem(item(), item({ title: '다른 사람이 고친 제목' }));
    vi.spyOn(api, 'itemProfile').mockResolvedValue(profile);
    const setProfile = vi.spyOn(api, 'setItemProfile').mockResolvedValue({ kind: 'saved' });
    render(ItemScreen, { props: { workspace, itemId: 'i1', workspaces: [workspace] } });

    await fireEvent.input(await waitFor(description), { target: { value: '쓰는 중인 설명' } });
    const panel = await screen.findByRole('region', { name: '정보' });
    await fireEvent.click(await within(panel).findByRole('button', { name: '고치기' }));
    await fireEvent.click(within(panel).getByRole('button', { name: '저장' }));
    await waitFor(() => {
      expect(setProfile).toHaveBeenCalledTimes(1);
    });
    // 저장 뒤 다시 읽은 항목이 화면에 들어온 뒤에 본다.
    await screen.findByRole('heading', { name: '다른 사람이 고친 제목' });

    expect(description().value).toBe('쓰는 중인 설명');
  });

  // 리뷰: 저장 응답을 기다리는 사이 더 고친 제목을 응답(보낸 값)으로 덮어 입력이 사라지고, 그 입력은 저장되지
  // 않았는데도 "저장했습니다."가 보였다. 보낸 그대로인 칸만 응답 값으로 바꾼다.
  it('keeps edits typed while the save is in flight and does not claim they were saved', async () => {
    await setLocale('ko', { reload: false });
    quietServer();
    const done = vi.spyOn(feedback, 'notifyDone');
    mockItem(item());
    let finish: (next: ItemView) => void = () => undefined;
    type Saved = Awaited<ReturnType<typeof api.updateItem>>;
    const update = vi.spyOn(api, 'updateItem').mockImplementation(
      () =>
        new Promise<Saved>((resolve) => {
          finish = (next) => {
            resolve({ kind: 'updated', item: next });
          };
        }),
    );
    render(ItemScreen, { props: { workspace, itemId: 'i1', workspaces: [workspace] } });

    await fireEvent.input(await waitFor(titleInput), { target: { value: '보낸 제목' } });
    const form = screen.getByRole('form', { name: '항목 정보' });
    await fireEvent.click(within(form).getByRole('button', { name: '저장' }));
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(1);
    });
    // 응답을 기다리는 사이 제목을 더 고친다.
    await fireEvent.input(titleInput(), { target: { value: '더 고친 제목' } });
    finish(item({ title: '보낸 제목' }));
    await screen.findByRole('heading', { name: '보낸 제목' });

    expect(titleInput().value).toBe('더 고친 제목');
    // 일반 완료는 토스트다(D-82). 더 고친 칸이 남았으면 알리지 않는다.
    expect(done).not.toHaveBeenCalled();
  });

  it('shows the saved status when nothing more was typed during the save', async () => {
    await setLocale('ko', { reload: false });
    quietServer();
    const done = vi.spyOn(feedback, 'notifyDone');
    mockItem(item());
    vi.spyOn(api, 'updateItem').mockResolvedValue({
      kind: 'updated',
      item: item({ title: '보낸 제목' }),
    });
    render(ItemScreen, { props: { workspace, itemId: 'i1', workspaces: [workspace] } });

    // 앞뒤 공백은 서버가 다듬어 돌려준다. 그대로 보낸 칸이므로 다듬은 값으로 바꾸고 저장된 것으로 본다.
    await fireEvent.input(await waitFor(titleInput), { target: { value: '보낸 제목 ' } });
    const form = screen.getByRole('form', { name: '항목 정보' });
    await fireEvent.click(within(form).getByRole('button', { name: '저장' }));

    await waitFor(() => {
      expect(done).toHaveBeenCalledWith('저장했습니다.');
    });
    expect(titleInput().value).toBe('보낸 제목');
  });
});

// FC-2(R5 P1 F-11): 쓸 수 없게 된 항목에는 남긴 입력의 되살리기 대신 읽기 전용 안내와 버리기만 보인다. 되살리면 저장할
// 수 없는 편집 상태가 열렸다.
describe('item info draft after write access is withdrawn', () => {
  it('offers only a read-only notice and discard for the kept input', async () => {
    await setLocale('ko', { reload: false });
    localStorage.clear();
    quietServer();
    mockItem(item());
    const update = vi.spyOn(api, 'updateItem');
    const view = render(ItemScreen, { props: { workspace, itemId: 'i1', workspaces: [workspace] } });
    await fireEvent.input(await waitFor(titleInput), { target: { value: '권한 철회 전 제목' } });
    view.unmount();

    mockItem(item({ access: 'read' }));
    render(ItemScreen, { props: { workspace, itemId: 'i1', workspaces: [workspace] } });
    const offer = await screen.findByTestId('form-draft-offer');
    expect(within(offer).queryByRole('button', { name: m.common_draft_restore() })).toBeNull();
    expect(offer).toHaveTextContent(m.common_draft_read_only());
    await fireEvent.click(within(offer).getByRole('button', { name: m.common_draft_discard() }));
    expect(screen.queryByTestId('form-draft-offer')).toBeNull();
    expect(update).not.toHaveBeenCalled();
    localStorage.clear();
  });
});

// 리뷰: 공개 범위는 체크를 바꿨을 때만 보낸다(감사 W13). 비공개 항목을 공개로 바꿔 저장하고, 응답 전에 체크를 풀어 다시
// 저장하면 그 저장은 아직 비공개인 연 때 값과 같아 공개 범위를 보내지 않았다. 먼저 보낸 공개가 서버에 남는데 화면은
// 비공개로 보였다. 저장이 끝날 때까지 다시 보내지 않고, 끝난 뒤의 저장은 응답 값과 비교해 비공개를 보낸다.
describe('item info save while a visibility change is in flight', () => {
  it('sends no second save until the public one returns, then sends private', async () => {
    await setLocale('ko', { reload: false });
    quietServer();
    const done = vi.spyOn(feedback, 'notifyDone');
    mockItem(item({ visibility: 'private' }));
    // 서버의 공개 범위. 첫 저장은 손으로 끝내고, 그 뒤의 저장은 보낸 공개 범위만 바꾼다(없으면 그대로).
    let serverVisibility: ItemView['visibility'] = 'private';
    let finishFirst: (next: ItemView) => void = () => undefined;
    type Saved = Awaited<ReturnType<typeof api.updateItem>>;
    const update = vi
      .spyOn(api, 'updateItem')
      .mockImplementationOnce(
        () =>
          new Promise<Saved>((resolve) => {
            finishFirst = (next) => {
              resolve({ kind: 'updated', item: next });
            };
          }),
      )
      .mockImplementation(
        (_id: string, _base: number, input: Parameters<typeof api.updateItem>[2]) => {
          serverVisibility = input.visibility ?? serverVisibility;
          return Promise.resolve({ kind: 'updated', item: item({ visibility: serverVisibility }) });
        },
      );
    render(ItemScreen, { props: { workspace, itemId: 'i1', workspaces: [workspace] } });

    const sharedBox = await screen.findByLabelText(/워크스페이스 멤버에게 공개/);
    const form = screen.getByRole('form', { name: '항목 정보' });
    const saveButton = within(form).getByRole('button', { name: '저장' });
    await fireEvent.click(sharedBox);
    await fireEvent.submit(form);
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(1);
    });
    expect(update.mock.calls[0]?.[2]).toEqual({
      title: '항목',
      description: '원래 설명',
      visibility: 'workspace',
    });

    // 응답 전에 체크를 풀고 다시 보낸다(두 번 누르기·Enter). 그 저장이 있었다면 끝까지 돌게 둔다.
    await fireEvent.click(sharedBox);
    await fireEvent.submit(form);
    await new Promise((r) => setTimeout(r, 0));
    expect(update).toHaveBeenCalledTimes(1);
    expect(saveButton).toBeDisabled();

    // 첫 저장이 서버에 공개를 넣고 돌아온다. 보낸 뒤 푼 체크는 그대로 둔다.
    serverVisibility = 'workspace';
    finishFirst(item({ visibility: 'workspace' }));
    await screen.findByText('워크스페이스 공개');
    expect(sharedBox).not.toBeChecked();
    await waitFor(() => {
      expect(saveButton).toBeEnabled();
    });

    // 끝난 뒤 다시 저장하면 응답(공개)과 다르므로 비공개를 보낸다.
    await fireEvent.submit(form);
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(2);
    });
    expect(update.mock.calls[1]?.[2]).toEqual({
      title: '항목',
      description: '원래 설명',
      visibility: 'private',
    });
    // 일반 완료는 토스트다(D-82). 체크를 푼 채 돌아온 첫 저장은 알리지 않고, 끝난 뒤의 저장만 알린다.
    await waitFor(() => {
      expect(done).toHaveBeenCalledWith('저장했습니다.');
    });
    expect(done).toHaveBeenCalledTimes(1);
    // 화면은 서버에 남은 값과 같다.
    expect(serverVisibility).toBe('private');
    expect(screen.getByText('비공개')).toBeInTheDocument();
    expect(screen.queryByText('워크스페이스 공개')).toBeNull();
    expect(sharedBox).not.toBeChecked();
  });
});

function task(id: string, title: string): TaskView {
  return {
    id,
    item_id: 'i1',
    module_id: 'm1',
    workspace_id: 'ws-1',
    parent_id: null,
    reference: `DEV-${id}`,
    title,
    description: '',
    status: '할 일',
    priority: 'none',
    start_date: null,
    due_date: null,
    estimate_minutes: null,
    tags: [],
    milestone: false,
    predecessors: [],
    series: null,
    occurrence_date: null,
    assignees: [],
    status_category: 'todo',
    checklist_done: 0,
    checklist_total: 0,
    version: 1,
    move_version: 1,
    completed_at: null,
    access: 'write',
    updated_at: '2026-10-21T00:00:00Z',
  };
}

// 리뷰: 업무 보드가 업무 옮기기를 거절당해(403·409) 항목을 다시 읽을 때 항목 상태만 바꾸고 입력란은 옛 값으로 두었다.
// 그러면 다음 저장이 그사이 다른 사람이 바꾼 제목을 옛 제목으로 되돌렸다. 고치지 않은 칸은 새 값으로, 입력 중인 칸은
// 그대로 둔다.
describe('item info form when the task board finds the item changed', () => {
  it('fills untouched fields from the item read again and keeps the typed ones', async () => {
    await setLocale('ko', { reload: false });
    window.sessionStorage.clear();
    quietServer();
    mockItem(item(), item({ title: '다른 사람이 고친 제목' }));
    vi.spyOn(api, 'tasks').mockResolvedValue([task('1', '문서 검토')]);
    vi.spyOn(api, 'savedViews').mockResolvedValue([]);
    vi.spyOn(api, 'syncRecurrences').mockResolvedValue(0);
    const move = vi
      .spyOn(api, 'moveTask')
      .mockRejectedValue(new ApiError(403, 'forbidden', 'no longer allowed'));
    render(ItemScreen, { props: { workspace, itemId: 'i1', workspaces: [workspace] } });

    await fireEvent.input(await waitFor(description), { target: { value: '쓰는 중인 설명' } });
    await screen.findByRole('button', { name: '문서 검토 옮기기' });
    const card = document.querySelector<HTMLElement>('[data-task-card="1"]');
    if (card === null) throw new Error('card');
    await fireEvent.keyDown(card, { key: 'ArrowRight', altKey: true });
    await waitFor(() => {
      expect(move).toHaveBeenCalledTimes(1);
    });
    await screen.findByRole('heading', { name: '다른 사람이 고친 제목' });

    expect(titleInput().value).toBe('다른 사람이 고친 제목');
    expect(description().value).toBe('쓰는 중인 설명');
  });
});
it('ItemScreen follows direct gantt URLs, view navigation and task detail URLs (E regression)', async () => {
  await setLocale('ko', { reload: false });
  quietServer();
  const ws = { ...workspace, id: '10000000-0000-0000-0000-000000000001' };
  const current = item({ id: '20000000-0000-0000-0000-000000000001', workspace_id: ws.id });
  const entry = {
    ...task('30000000-0000-0000-0000-000000000001', '경로 업무'),
    item_id: current.id,
    workspace_id: ws.id,
    start_date: '2026-10-25',
    due_date: '2026-10-28',
  };
  mockItem(current);
  vi.spyOn(api, 'tasks').mockResolvedValue([entry]);
  vi.spyOn(api, 'savedViews').mockResolvedValue([]);
  vi.spyOn(api, 'syncRecurrences').mockResolvedValue(0);
  vi.spyOn(api, 'task').mockResolvedValue({ ...entry, checklist: [] });
  vi.spyOn(api, 'assignable').mockResolvedValue([]);
  router.guard = undefined;
  router.path = itemViewPath(ws.id, current.id, 'gantt');
  window.history.replaceState(null, '', router.path);
  render(ItemScreen, { workspace: ws, itemId: current.id, workspaces: [ws] });
  await screen.findByTestId('task-gantt');
  expect(screen.getByRole('button', { name: '간트' })).toHaveAttribute('aria-pressed', 'true');
  await fireEvent.click(screen.getByRole('button', { name: '칸반' }));
  await screen.findByTestId('task-board');
  expect(window.location.pathname).toBe(itemViewPath(ws.id, current.id, 'board'));
  await fireEvent.click(screen.getByRole('button', { name: '간트' }));
  await screen.findByTestId('task-gantt');
  expect(window.location.pathname).toBe(itemViewPath(ws.id, current.id, 'gantt'));
  router.navigate(itemTaskPath(ws.id, current.id, entry.id));
  await screen.findByRole('dialog');
  await waitFor(() => {
    expect(api.task).toHaveBeenCalledWith(entry.id);
  });
});
