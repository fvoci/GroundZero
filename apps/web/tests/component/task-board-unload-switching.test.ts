import './fixtures/task-statuses';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { ItemView, TaskDetailView } from '$lib/api/client';
import { api, ApiError } from '$lib/api/client';
import { accountSafety } from '$lib/features/feedback/feedback.svelte';
import TaskBoard from '$lib/features/tasks/TaskBoard.svelte';
import { me } from '$lib/me.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

// #88·D-96: 다른 계정으로 바뀌어 이 창을 다시 읽을 때, 저장 안 한 업무 입력이 앞 계정의 브라우저 초안으로 **남았으면** beforeunload
// 경고로 다시 읽기를 막지 않는다(막고 남으면 앞 계정의 화면과 입력이 새 계정의 세션으로 요청을 보낸다). 초안을 **남기지 못했으면**
// (저장소 사용 불가·용량 초과) 보통 떠나기처럼 경고해 입력을 잃지 않게 한다 — 문서 편집기(DocumentEditor)와 같은 규칙이다. 전환이
// 아닐 때는 전처럼 경고한다. 실제 TaskBoard+TaskDetail 폼과 실제 localStorage로 본다.

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

const detail: TaskDetailView = {
  id: 't-1',
  item_id: 'i1',
  module_id: 'm1',
  workspace_id: 'ws-1',
  parent_id: null,
  reference: 'KED-1',
  title: '구현',
  description: '',
  status: 'todo',
  status_category: 'todo',
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
  move_version: 0,
  completed_at: null,
  access: 'write',
  updated_at: '2026-10-19T00:00:00Z',
  checklist: [],
};

const draftKeys = (): string[] =>
  Object.keys(window.localStorage).filter((k) =>
    k.startsWith('fvoci:user-a:-:form-draft:task:t-1'),
  );

function unloadPrevented(): boolean {
  const event = new Event('beforeunload', { cancelable: true });
  window.dispatchEvent(event);
  return event.defaultPrevented;
}

async function mountWithOpenTask(): Promise<HTMLElement> {
  vi.spyOn(api, 'tasks').mockResolvedValue([detail]);
  vi.spyOn(api, 'savedViews').mockResolvedValue([]);
  vi.spyOn(api, 'syncRecurrences').mockResolvedValue(0);
  vi.spyOn(api, 'task').mockResolvedValue(detail);
  vi.spyOn(api, 'assignable').mockResolvedValue([]);
  vi.spyOn(api, 'taskBacklinks').mockResolvedValue([]);
  vi.spyOn(api, 'comments').mockResolvedValue([]);
  render(TaskBoard, { props: { item, workspaces: [], openTaskId: 't-1' } });
  return screen.findByLabelText('설명', { exact: true });
}

describe('task board unload warning while the account is switching', () => {
  it('does not treat child unmount during account hiding as successful persistence', async () => {
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new DOMException('quota', 'QuotaExceededError');
    });
    await mountWithOpenTask();
    await fireEvent.input(screen.getByLabelText('새 항목'), { target: { value: '남기지 못함' } });
    vi.spyOn(api, 'updateTask').mockRejectedValue(
      new ApiError(409, 'account_changed', '계정이 바뀌었습니다'),
    );
    const save = document.getElementById('task-save');
    if (save === null) throw new Error('task-save');
    await fireEvent.click(save);
    await screen.findByText(/계정이 바뀌어 입력을 숨겼습니다/);
    accountSafety.switching = true;
    expect(unloadPrevented()).toBe(true);
  });
  it.each(['새 댓글', '새 항목'])('allows reload for durably kept %s input', async (label) => {
    await mountWithOpenTask();
    await fireEvent.input(await screen.findByLabelText(new RegExp(`^${label}`)), {
      target: { value: '남긴 입력' },
    });
    expect(unloadPrevented()).toBe(true);
    accountSafety.switching = true;
    await Promise.resolve();
    expect(unloadPrevented()).toBe(false);
  });

  it.each(['새 댓글', '새 항목'])(
    'keeps warning for failed %s storage and recovers after a successful write',
    async (label) => {
      const quota = vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
        throw new DOMException('quota', 'QuotaExceededError');
      });
      await mountWithOpenTask();
      const input = await screen.findByLabelText(new RegExp(`^${label}`));
      await fireEvent.input(input, { target: { value: '남기지 못함' } });
      accountSafety.switching = true;
      await Promise.resolve();
      expect(unloadPrevented()).toBe(true);
      quota.mockRestore();
      accountSafety.switching = false;
      await fireEvent.input(input, { target: { value: '이번에는 남김' } });
      accountSafety.switching = true;
      await Promise.resolve();
      expect(unloadPrevented()).toBe(false);
    },
  );
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
    me.userId = 'user-a';
    accountSafety.switching = false;
    window.localStorage.clear();
  });
  afterEach(() => {
    accountSafety.switching = false;
    me.userId = '';
    vi.restoreAllMocks();
    window.localStorage.clear();
  });

  it('warns normally, but not while switching when the input is kept as a draft', async () => {
    const description = await mountWithOpenTask();
    expect(unloadPrevented()).toBe(false);
    await fireEvent.input(description, { target: { value: '아직 저장하지 않은 설명' } });
    await waitFor(() => {
      expect(draftKeys()).toHaveLength(1);
    });
    expect(unloadPrevented()).toBe(true);

    accountSafety.switching = true;
    expect(unloadPrevented()).toBe(false);

    accountSafety.switching = false;
    expect(unloadPrevented()).toBe(true);
  });

  it('keeps warning while switching when the draft could not be stored, so the input is not lost', async () => {
    // 저장소 사용 불가·용량 초과: 초안 쓰기가 던진다(읽기는 그대로).
    const quota = vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new DOMException('quota exceeded (test)', 'QuotaExceededError');
    });
    const description = await mountWithOpenTask();
    await fireEvent.input(description, { target: { value: '남기지 못한 설명' } });
    await waitFor(() => {
      expect(quota).toHaveBeenCalled();
    });
    expect(draftKeys()).toEqual([]);
    expect(unloadPrevented()).toBe(true);

    accountSafety.switching = true;
    // 전환 중이어도 초안이 없으면 경고를 유지한다(막아도 화면은 안전 상태가 가리고 요청은 차단된다).
    expect(unloadPrevented()).toBe(true);
    if (!(description instanceof HTMLTextAreaElement)) throw new Error('설명');
    expect(description.value).toBe('남기지 못한 설명');

    // 저장소가 돌아와 다음 입력이 남으면 전환 중 경고는 사라진다.
    quota.mockRestore();
    accountSafety.switching = false;
    await fireEvent.input(description, { target: { value: '남기지 못한 설명 (이어서)' } });
    await waitFor(() => {
      expect(draftKeys()).toHaveLength(1);
    });
    accountSafety.switching = true;
    expect(unloadPrevented()).toBe(false);
  });

  it('still warns while switching after a save answered account_changed, when the draft was not stored', async () => {
    const quota = vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new DOMException('quota exceeded (test)', 'QuotaExceededError');
    });
    const description = await mountWithOpenTask();
    await fireEvent.input(description, { target: { value: '남기지 못한 설명' } });
    await waitFor(() => {
      expect(quota).toHaveBeenCalled();
    });
    // 저장이 409 account_changed로 끝나면 폼은 잠기지만(canEdit=false) 고친 입력은 그대로 있다.
    vi.spyOn(api, 'updateTask').mockRejectedValue(
      new ApiError(409, 'account_changed', '계정이 바뀌었습니다'),
    );
    const save = document.getElementById('task-save');
    if (save === null) throw new Error('task-save');
    await fireEvent.click(save);
    await screen.findByText(/계정이 바뀌어 입력을 숨겼습니다/);
    accountSafety.switching = true;
    expect(unloadPrevented()).toBe(true);
  });

  it('does not carry a failed-draft state over to the next task form', async () => {
    const quota = vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new DOMException('quota exceeded (test)', 'QuotaExceededError');
    });
    vi.spyOn(api, 'tasks').mockResolvedValue([
      detail,
      { ...detail, id: 't-2', title: '다음 업무' },
    ]);
    vi.spyOn(api, 'savedViews').mockResolvedValue([]);
    vi.spyOn(api, 'syncRecurrences').mockResolvedValue(0);
    const task = vi.spyOn(api, 'task').mockResolvedValue(detail);
    vi.spyOn(api, 'assignable').mockResolvedValue([]);
    vi.spyOn(api, 'taskBacklinks').mockResolvedValue([]);
    vi.spyOn(api, 'comments').mockResolvedValue([]);
    const { rerender } = render(TaskBoard, { props: { item, workspaces: [], openTaskId: 't-1' } });
    const description = await screen.findByLabelText('설명', { exact: true });
    await fireEvent.input(description, { target: { value: '남기지 못한 설명' } });
    await waitFor(() => {
      expect(quota).toHaveBeenCalled();
    });
    accountSafety.switching = true;
    expect(unloadPrevented()).toBe(true);
    accountSafety.switching = false;

    // 다른 업무를 열면 폼이 새로 만들어진다. 깨끗한 새 폼은 전환 중 경고하지 않는다.
    task.mockResolvedValue({ ...detail, id: 't-2', title: '다음 업무' });
    await rerender({ item, workspaces: [], openTaskId: 't-2' });
    await screen.findByDisplayValue('다음 업무');
    accountSafety.switching = true;
    expect(unloadPrevented()).toBe(false);
  });
});
