import './fixtures/task-statuses';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { ItemView, TaskView } from '$lib/api/client';
import { api, NetworkError } from '$lib/api/client';
import TaskBoard from '$lib/features/tasks/TaskBoard.svelte';
import { me } from '$lib/me.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

// D-96·D-128: 만들지 않은 새 업무 제목(아래 폼·열의 빠른 추가)은 친 계정의 브라우저 초안으로 남고, 다시 읽은 뒤 같은 계정에게만
// 되살릴지 묻는다. 되살리면 제목과 그때의 request_id를 시드하므로 응답을 잃었던 만들기를 다시 눌러도 중복되지 않는다. 사용자가
// "업무 만들기"를 눌러야 보낸다. 업무 편집 폼 시험(task-form-draft)과 별개다.

const A = 'user-a';
const B = 'user-b';
const prefix = (user: string): string => `fvoci:${user}:-:form-draft:task-new:i1:`;
const keysOf = (user: string): string[] =>
  Object.keys(window.localStorage).filter((k) => k.startsWith(prefix(user)));

function stored(user: string): { lane: string | null; name: string; requestId: string | null }[] {
  return keysOf(user).map((k) => {
    const value: unknown = JSON.parse(window.localStorage.getItem(k) ?? 'null');
    if (
      typeof value !== 'object' ||
      value === null ||
      !('lane' in value) ||
      !('name' in value) ||
      !('requestId' in value) ||
      (value.lane !== null && typeof value.lane !== 'string') ||
      typeof value.name !== 'string' ||
      (value.requestId !== null && typeof value.requestId !== 'string')
    )
      throw new Error('draft shape');
    return { lane: value.lane, name: value.name, requestId: value.requestId };
  });
}

const item: ItemView = {
  id: 'i1',
  module_id: 'm1',
  workspace_id: 'ws-1',
  reference: 'KED-1',
  title: '항목',
  description: '',
  visibility: 'workspace',
  access: 'write',
  archived: false,
  updated_at: '2026-10-19T00:00:00Z',
  status: 'active',
  is_template: false,
  version: 1,
  starts_on: null,
  ends_on: null,
};

function task(id: string, title: string): TaskView {
  return {
    id,
    item_id: 'i1',
    module_id: 'm1',
    workspace_id: 'ws-1',
    parent_id: null,
    reference: `KED-${id}`,
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
    status_category: 'todo',
    checklist_done: 0,
    checklist_total: 0,
    version: 1,
    move_version: 0,
    completed_at: null,
    access: 'write',
    updated_at: '2026-10-19T00:00:00Z',
  };
}

async function mount(): Promise<void> {
  vi.spyOn(api, 'tasks').mockResolvedValue([]);
  vi.spyOn(api, 'savedViews').mockResolvedValue([]);
  vi.spyOn(api, 'syncRecurrences').mockResolvedValue(0);
  render(TaskBoard, { props: { item, workspaces: [], openTaskId: undefined } });
  await screen.findByTestId('task-board');
}

async function input(label: string): Promise<HTMLInputElement> {
  const box = await screen.findByLabelText(label, { exact: true });
  if (!(box instanceof HTMLInputElement)) throw new Error(label);
  return box;
}

/** 첫 열의 "업무 추가" 단추. */
function firstAddButton(): HTMLElement {
  const button = screen.getAllByRole('button', { name: /^업무 추가$/ }).at(0);
  if (button === undefined) throw new Error('업무 추가');
  return button;
}

/** 새로고침처럼 저장소는 남기고 화면만 새로 그린다. */
function reloadLike(): void {
  const kept = Object.keys(window.localStorage).map(
    (k) => [k, window.localStorage.getItem(k) ?? ''] as const,
  );
  cleanup();
  window.localStorage.clear();
  for (const [k, v] of kept) window.localStorage.setItem(k, v);
}

describe('new task title across reloads and account switches', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
    window.localStorage.clear();
    window.sessionStorage.clear();
    me.userId = A;
  });
  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
    me.userId = '';
    window.localStorage.clear();
  });

  it('keeps a typed title for the editing account and reuses the request id of a failed creation after a reload', async () => {
    await mount();
    const create = vi.spyOn(api, 'createTask').mockRejectedValueOnce(new NetworkError('offline'));
    await fireEvent.input(await input('새 업무 제목'), { target: { value: 'A의 새 업무' } });
    await waitFor(() => {
      expect(stored(A)).toEqual([{ lane: null, name: 'A의 새 업무', requestId: null }]);
    });
    await fireEvent.click(screen.getByRole('button', { name: /^업무 만들기$/ }));
    await screen.findByRole('alert');
    const requestId = stored(A)[0]?.requestId;
    expect(typeof requestId).toBe('string');
    expect(create).toHaveBeenCalledTimes(1);
    reloadLike();

    await mount();
    const offer = await screen.findByTestId('task-new-draft-offer');
    expect(offer).toHaveTextContent('A의 새 업무');
    expect((await input('새 업무 제목')).value).toBe('');
    await fireEvent.click(screen.getByRole('button', { name: /^입력 복원$/ }));
    expect((await input('새 업무 제목')).value).toBe('A의 새 업무');
    expect(screen.queryByTestId('task-new-draft-offer')).toBeNull();
    expect(create).toHaveBeenCalledTimes(1);

    create.mockResolvedValueOnce({ ...task('9', 'A의 새 업무'), checklist: [] });
    await fireEvent.click(screen.getByRole('button', { name: /^업무 만들기$/ }));
    await waitFor(() => {
      expect(create).toHaveBeenCalledTimes(2);
    });
    expect(create).toHaveBeenLastCalledWith('i1', requestId, 'A의 새 업무');
    await waitFor(() => {
      expect(keysOf(A)).toEqual([]);
    });
  });

  it('keeps a lane quick-add title with its lane, discards it on Esc and restores it into that lane', async () => {
    await mount();
    await fireEvent.click(firstAddButton());
    const lane = await input('todo에 업무 추가');
    await fireEvent.input(lane, { target: { value: 'todo 열의 업무' } });
    await waitFor(() => {
      expect(stored(A)).toEqual([{ lane: 'todo', name: 'todo 열의 업무', requestId: null }]);
    });
    // Esc는 이 입력을 버리는 것이다.
    await fireEvent.keyDown(lane, { key: 'Escape' });
    await waitFor(() => {
      expect(keysOf(A)).toEqual([]);
    });

    await fireEvent.click(firstAddButton());
    await fireEvent.input(await input('todo에 업무 추가'), { target: { value: 'todo 열의 업무' } });
    await waitFor(() => {
      expect(stored(A)).toHaveLength(1);
    });
    reloadLike();

    await mount();
    const offer = await screen.findByTestId('task-new-draft-offer');
    expect(offer).toHaveTextContent('todo 열');
    expect(offer).toHaveTextContent('todo 열의 업무');
    await fireEvent.click(screen.getByRole('button', { name: /^입력 복원$/ }));
    expect((await input('todo에 업무 추가')).value).toBe('todo 열의 업무');
    expect(screen.queryByTestId('task-new-draft-offer')).toBeNull();
  });

  it('shows nothing to another account and leaves the first account’s title untouched', async () => {
    await mount();
    await fireEvent.input(await input('새 업무 제목'), { target: { value: 'A의 새 업무' } });
    await waitFor(() => {
      expect(stored(A)).toHaveLength(1);
    });
    reloadLike();

    me.userId = B;
    await mount();
    await input('새 업무 제목');
    expect(screen.queryByTestId('task-new-draft-offer')).toBeNull();
    expect(stored(A)).toEqual([{ lane: null, name: 'A의 새 업무', requestId: null }]);
    expect(keysOf(B)).toEqual([]);
  });

  it('drops the restored quick-add seed when that lane form closes, so reopening starts empty with a fresh request id', async () => {
    await mount();
    await fireEvent.click(firstAddButton());
    await fireEvent.input(await input('todo에 업무 추가'), { target: { value: 'todo 열의 업무' } });
    await waitFor(() => {
      expect(stored(A)).toHaveLength(1);
    });
    reloadLike();

    await mount();
    await screen.findByTestId('task-new-draft-offer');
    await fireEvent.click(screen.getByRole('button', { name: /^입력 복원$/ }));
    const seeded = await input('todo에 업무 추가');
    expect(seeded.value).toBe('todo 열의 업무');
    const create = vi
      .spyOn(api, 'createTask')
      .mockResolvedValue({ ...task('9', 'todo 열의 업무'), checklist: [] });
    const form = seeded.closest('form');
    if (form === null) throw new Error('quick add form');
    await fireEvent.submit(form);
    await waitFor(() => {
      expect(create).toHaveBeenCalledTimes(1);
    });
    const firstRequestId = create.mock.calls[0]?.[1];
    // 만들기가 끝나 폼이 비워질 때까지 기다린다(보내는 중에는 Esc를 받지 않는다).
    await waitFor(() => {
      expect(seeded.value).toBe('');
    });
    expect(typeof firstRequestId).toBe('string');

    // 닫고 같은 열을 다시 연다: 옛 제목이 다시 채워지지 않고, 같은 제목을 쳐도 새 request_id로 보낸다(옛 id면 서버가 앞서
    // 만든 업무를 돌려줘 새 업무가 생기지 않는다).
    await fireEvent.keyDown(await input('todo에 업무 추가'), { key: 'Escape' });
    await waitFor(() => {
      expect(screen.queryByLabelText('todo에 업무 추가')).toBeNull();
    });
    await fireEvent.click(firstAddButton());
    const reopened = await input('todo에 업무 추가');
    expect(reopened.value).toBe('');
    await fireEvent.input(reopened, { target: { value: 'todo 열의 업무' } });
    const again = reopened.closest('form');
    if (again === null) throw new Error('quick add form');
    await fireEvent.submit(again);
    await waitFor(() => {
      expect(create).toHaveBeenCalledTimes(2);
    });
    expect(create.mock.calls[1]?.[1]).not.toBe(firstRequestId);
  });
});
