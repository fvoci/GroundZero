import type { Module, Workspace } from '@fvoci/api-client';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { ItemProfileView, ItemView, TaskView } from '$lib/api/client';
import { api, ApiError } from '$lib/api/client';
import ItemScreen from '$lib/features/modules/ItemScreen.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

const workspace: Workspace = {
  id: 'ws-1',
  name: '공간',
  role: 'admin',
  is_owner: true,
  realtime_enabled: false,
};

const mod: Module = {
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
    const setProfile = vi.spyOn(api, 'setItemProfile').mockResolvedValue(undefined);
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
    mockItem(item());
    let finish: (next: ItemView) => void = () => undefined;
    const update = vi.spyOn(api, 'updateItem').mockImplementation(
      () =>
        new Promise<ItemView>((resolve) => {
          finish = resolve;
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
    expect(screen.queryByText('저장했습니다.')).toBeNull();
  });

  it('shows the saved status when nothing more was typed during the save', async () => {
    await setLocale('ko', { reload: false });
    quietServer();
    mockItem(item());
    vi.spyOn(api, 'updateItem').mockResolvedValue(item({ title: '보낸 제목' }));
    render(ItemScreen, { props: { workspace, itemId: 'i1', workspaces: [workspace] } });

    // 앞뒤 공백은 서버가 다듬어 돌려준다. 그대로 보낸 칸이므로 다듬은 값으로 바꾸고 저장된 것으로 본다.
    await fireEvent.input(await waitFor(titleInput), { target: { value: '보낸 제목 ' } });
    const form = screen.getByRole('form', { name: '항목 정보' });
    await fireEvent.click(within(form).getByRole('button', { name: '저장' }));

    expect(await screen.findByText('저장했습니다.')).toBeInTheDocument();
    expect(titleInput().value).toBe('보낸 제목');
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
    status: 'todo',
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
