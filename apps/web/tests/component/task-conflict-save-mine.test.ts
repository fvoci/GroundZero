import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { ShiftProposalView, TaskDetailView, TaskInput } from '$lib/api/client';
import { api } from '$lib/api/client';
import TaskDetail from '$lib/features/tasks/TaskDetail.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

// 업무 저장 충돌(409) 뒤 "내 입력으로 다시 저장"(실제 브라우저 점검에서 찾음): 전에는 내 폼 전체를 새 판 번호로 보내,
// 내가 고치지 않은 칸(다른 사람이 바꾼 제목 등)을 옛 값으로 조용히 되돌렸다. 이제 최신 판 위에 내가 고친 칸만 올리고,
// 같은 칸을 다른 사람도 다른 값으로 바꿨으면 양쪽 값을 보이고 덮어쓸지 묻는다(D-94).
afterEach(() => {
  vi.restoreAllMocks();
});

const v2: TaskDetailView = {
  id: 't1',
  item_id: 'i1',
  module_id: 'm1',
  workspace_id: 'ws-1',
  parent_id: null,
  reference: 'KED-1',
  title: '옛 제목',
  description: '',
  status: 'todo',
  priority: 'none',
  start_date: null,
  due_date: null,
  estimate_minutes: null,
  tags: [],
  milestone: false,
  predecessor_ids: [],
  predecessors: [],
  series: null,
  occurrence_date: null,
  assignees: [],
  checklist_done: 0,
  checklist_total: 0,
  version: 2,
  move_version: 0,
  completed_at: null,
  access: 'write',
  updated_at: '2026-10-21T00:00:00Z',
  checklist: [],
};

const unchanged: TaskInput = {
  title: v2.title,
  description: v2.description,
  status: v2.status,
  priority: v2.priority,
  start_date: v2.start_date,
  due_date: v2.due_date,
  estimate_minutes: v2.estimate_minutes,
  tags: v2.tags,
  assignee_ids: [],
  milestone: v2.milestone,
};

function mount(current: TaskDetailView = v2): void {
  vi.spyOn(api, 'task').mockResolvedValue(current);
  vi.spyOn(api, 'assignable').mockResolvedValue([]);
  vi.spyOn(api, 'taskBacklinks').mockResolvedValue([]);
  vi.spyOn(api, 'comments').mockResolvedValue([]);
  render(TaskDetail, {
    props: {
      taskId: current.id,
      tasks: [current],
      workspaces: [],
      onchanged: () => undefined,
      onopen: () => undefined,
    },
  });
}

function field(id: string): HTMLInputElement | HTMLTextAreaElement {
  const el = document.getElementById(id);
  if (!(el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement)) throw new Error(id);
  return el;
}

async function type(id: string, value: string): Promise<void> {
  await fireEvent.input(field(id), { target: { value } });
}

/** 탭 B: 판 2에서 고쳐 저장하면 충돌 화면이 뜬다. */
async function saveIntoConflict(): Promise<void> {
  const save = document.getElementById('task-save');
  if (save === null) throw new Error('task-save');
  await fireEvent.click(save);
  await screen.findByRole('button', { name: '내 입력으로 다시 저장' });
}

function saveMineButton(): HTMLElement {
  return screen.getByRole('button', { name: '내 입력으로 다시 저장' });
}

describe('saving my input after a task conflict', () => {
  it('keeps the title the other person changed when I only edited the description', async () => {
    await setLocale('ko', { reload: false });
    mount();
    const v3 = { ...v2, version: 3, title: '새 제목' };
    const v4 = { ...v3, version: 4, description: 'B 설명' };
    const update = vi
      .spyOn(api, 'updateTask')
      .mockResolvedValueOnce({ kind: 'conflict', current: v3 })
      .mockResolvedValueOnce({ kind: 'updated', task: v4 });
    await screen.findByLabelText('설명');
    await type('task-description', 'B 설명');
    await saveIntoConflict();
    expect(update).toHaveBeenNthCalledWith(1, 't1', 2, { ...unchanged, description: 'B 설명' }, []);

    await fireEvent.click(saveMineButton());
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(2);
    });
    // 겹친 칸이 없으므로 묻지 않고 최신 판 번호로 저장한다.
    expect(update).toHaveBeenNthCalledWith(
      2,
      't1',
      3,
      { ...unchanged, title: '새 제목', description: 'B 설명' },
      [],
    );
    expect(screen.queryByRole('alertdialog')).toBeNull();
    // 저장한 판으로 다시 채워 화면이 저장된 내용을 보인다.
    expect(await screen.findByText('저장했습니다.')).toBeTruthy();
    expect(field('task-title').value).toBe('새 제목');
    expect(field('task-description').value).toBe('B 설명');
    expect(screen.queryByRole('button', { name: '내 입력으로 다시 저장' })).toBeNull();
  });

  it('asks before saving my value over fields the other person changed too', async () => {
    await setLocale('ko', { reload: false });
    mount();
    const preview = vi
      .spyOn(api, 'previewSchedule')
      .mockResolvedValue({ shifts: [], undated: [], blocked: null });
    const v3: TaskDetailView = {
      ...v2,
      version: 3,
      title: '남의 제목',
      priority: 'high',
      due_date: '2026-11-05',
      estimate_minutes: 30,
      tags: ['서버'],
    };
    const v4 = { ...v3, version: 4, title: '내 제목', estimate_minutes: 45 };
    const update = vi
      .spyOn(api, 'updateTask')
      .mockResolvedValueOnce({ kind: 'conflict', current: v3 })
      .mockResolvedValueOnce({ kind: 'updated', task: v4 });
    await screen.findByLabelText('설명');
    await type('task-title', '내 제목');
    await type('task-estimate', '45');
    await saveIntoConflict();

    // 제목·예상 시간은 양쪽에서 서로 다르게 바꿨다: 보내기 전에 칸마다 서버 값과 내 값을 보이고 묻는다.
    await fireEvent.click(saveMineButton());
    const ask = await screen.findByRole('alertdialog', { name: '다른 사람도 바꾼 칸이 있습니다' });
    expect(update).toHaveBeenCalledTimes(1);
    const title = ask.querySelector('[data-clash-field="title"]');
    // 칸 이름은 폼의 이름표와 같은 글이다.
    expect(title?.firstElementChild?.textContent).toBe('제목');
    expect(title?.textContent).toContain('서버의 지금 값: 남의 제목');
    expect(title?.textContent).toContain('내 값: 내 제목');
    const estimate = ask.querySelector('[data-clash-field="estimate_minutes"]');
    expect(estimate?.firstElementChild?.textContent).toBe('예상 시간(분)');
    expect(estimate?.textContent).toContain('서버의 지금 값: 30');
    expect(estimate?.textContent).toContain('내 값: 45');
    // 내가 고치지 않은 칸은 묻지 않는다.
    expect(within(ask).getAllByRole('listitem')).toHaveLength(2);
    await waitFor(() => {
      expect(document.activeElement?.id).toBe('task-clash-heading');
    });

    // 취소: 아무것도 보내지 않고 입력을 그대로 둔 채 "내 입력으로 다시 저장"으로 돌아간다.
    await fireEvent.click(within(ask).getByRole('button', { name: '취소' }));
    await waitFor(() => {
      expect(document.activeElement?.id).toBe('task-save-mine');
    });
    expect(screen.queryByRole('alertdialog')).toBeNull();
    expect(update).toHaveBeenCalledTimes(1);
    expect(field('task-title').value).toBe('내 제목');
    expect(field('task-estimate').value).toBe('45');
    expect(field('task-tags').value).toBe('');

    // 다시 눌러 덮어쓰기: 겹친 칸은 내 값, 고치지 않은 우선순위·마감일·태그는 최신 값으로 판 3에 저장한다.
    await fireEvent.click(saveMineButton());
    const again = await screen.findByRole('alertdialog');
    await fireEvent.click(within(again).getByRole('button', { name: '내 값으로 덮어쓰고 저장' }));
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(2);
    });
    expect(update).toHaveBeenNthCalledWith(
      2,
      't1',
      3,
      {
        ...unchanged,
        title: '내 제목',
        priority: 'high',
        due_date: '2026-11-05',
        estimate_minutes: 45,
        tags: ['서버'],
      },
      [],
    );
    // 날짜는 최신 판 그대로라 후속 업무 미리보기를 묻지 않는다(옛 날짜로 되돌리는 재배치를 제안하지 않는다).
    expect(preview).not.toHaveBeenCalled();
    expect(await screen.findByText('저장했습니다.')).toBeTruthy();
    expect(field('task-title').value).toBe('내 제목');
    expect(field('task-tags').value).toBe('서버');
    expect(field('task-estimate').value).toBe('45');
  });

  it('shows the newer version when it changed again and merges onto it next time', async () => {
    await setLocale('ko', { reload: false });
    mount();
    const v3 = { ...v2, version: 3, title: '새 제목' };
    // 다시 저장하려는 사이 또 바뀌었다(제목을 한 번 더, 상태도).
    const v4: TaskDetailView = { ...v3, version: 4, title: '세 번째 제목', status: 'in_progress' };
    const v5 = { ...v4, version: 5, description: 'B 설명' };
    const update = vi
      .spyOn(api, 'updateTask')
      .mockResolvedValueOnce({ kind: 'conflict', current: v3 })
      .mockResolvedValueOnce({ kind: 'conflict', current: v4 })
      .mockResolvedValueOnce({ kind: 'updated', task: v5 });
    await screen.findByLabelText('설명');
    await type('task-description', 'B 설명');
    await saveIntoConflict();

    await fireEvent.click(saveMineButton());
    // 서버가 판 3으로 검사해 다시 409로 답했다: 충돌 화면이 더 새 판을 보이고 입력은 그대로다.
    expect(await screen.findByText(/지금 제목: “세 번째 제목”, 상태: 진행 중/)).toBeTruthy();
    expect(update).toHaveBeenNthCalledWith(
      2,
      't1',
      3,
      { ...unchanged, title: '새 제목', description: 'B 설명' },
      [],
    );
    expect(field('task-description').value).toBe('B 설명');
    await waitFor(() => {
      expect(document.activeElement?.id).toBe('task-save-mine');
    });

    // 판 3에서 받은 제목은 내가 고친 칸이 아니므로, 판 4의 제목을 묻지 않고 따른다.
    await fireEvent.click(saveMineButton());
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(3);
    });
    expect(screen.queryByRole('alertdialog')).toBeNull();
    expect(update).toHaveBeenNthCalledWith(
      3,
      't1',
      4,
      { ...unchanged, title: '세 번째 제목', status: 'in_progress', description: 'B 설명' },
      [],
    );
    expect(await screen.findByText('저장했습니다.')).toBeTruthy();
  });

  it('keeps my input when saving again fails', async () => {
    await setLocale('ko', { reload: false });
    mount();
    const v3 = { ...v2, version: 3, title: '새 제목' };
    const update = vi
      .spyOn(api, 'updateTask')
      .mockResolvedValueOnce({ kind: 'conflict', current: v3 })
      .mockRejectedValueOnce(new Error('boom'));
    await screen.findByLabelText('설명');
    await type('task-description', 'B 설명');
    await saveIntoConflict();

    await fireEvent.click(saveMineButton());
    expect(await screen.findByText('저장하지 못했습니다.')).toBeTruthy();
    expect(update).toHaveBeenCalledTimes(2);
    expect(field('task-description').value).toBe('B 설명');
    expect(field('task-title').value).toBe('새 제목');
    // 충돌 화면은 남아 다시 시도할 수 있다.
    expect(saveMineButton()).toBeTruthy();
  });

  it('previews follower moves with my new due date on top of the latest version', async () => {
    await setLocale('ko', { reload: false });
    const dated = { ...v2, start_date: '2026-10-19', due_date: '2026-10-21' };
    mount(dated);
    const shift: ShiftProposalView = {
      task_id: 'c',
      reference: 'KED-c',
      title: '검토',
      base_version: 1,
      from_start_date: '2026-10-22',
      from_due_date: '2026-10-23',
      start_date: '2026-10-26',
      due_date: '2026-10-27',
      causes: [
        {
          predecessor_id: 't1',
          predecessor_reference: 'KED-1',
          predecessor_title: '새 제목',
          kind: 'FS',
          lag_days: 0,
        },
      ],
    };
    const preview = vi
      .spyOn(api, 'previewSchedule')
      .mockResolvedValueOnce({ shifts: [], undated: [], blocked: null })
      .mockResolvedValueOnce({ shifts: [shift], undated: [], blocked: null });
    const v3 = { ...dated, version: 3, title: '새 제목' };
    const update = vi
      .spyOn(api, 'updateTask')
      .mockResolvedValueOnce({ kind: 'conflict', current: v3 })
      .mockResolvedValueOnce({
        kind: 'updated',
        task: { ...v3, version: 4, due_date: '2026-10-25' },
      });
    await screen.findByLabelText('마감일');
    await type('task-due', '2026-10-25');
    await saveIntoConflict();

    await fireEvent.click(screen.getByRole('button', { name: '내 입력으로 다시 저장' }));
    const panel = (await screen.findByRole('heading', { name: /후속 업무 1개/ })).closest(
      'section',
    );
    if (panel === null) throw new Error('panel');
    expect(preview).toHaveBeenNthCalledWith(2, 't1', '2026-10-19', '2026-10-25');
    // 미리보기만으로는 저장하지 않는다.
    expect(update).toHaveBeenCalledTimes(1);
    await fireEvent.click(within(panel).getByRole('button', { name: '후속 업무도 옮겨 저장' }));
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(2);
    });
    expect(update).toHaveBeenNthCalledWith(
      2,
      't1',
      3,
      {
        ...unchanged,
        title: '새 제목',
        start_date: '2026-10-19',
        due_date: '2026-10-25',
      },
      [{ task_id: 'c', base_version: 1, start_date: '2026-10-26', due_date: '2026-10-27' }],
    );
  });

  it('still replaces the form with the latest version when loading the server content', async () => {
    await setLocale('ko', { reload: false });
    mount();
    const v3 = { ...v2, version: 3, title: '새 제목', description: 'A 설명' };
    const update = vi
      .spyOn(api, 'updateTask')
      .mockResolvedValueOnce({ kind: 'conflict', current: v3 });
    await screen.findByLabelText('설명');
    await type('task-description', 'B 설명');
    await saveIntoConflict();

    await fireEvent.click(screen.getByRole('button', { name: '서버 내용 불러오기' }));
    await waitFor(() => {
      expect(screen.queryByRole('button', { name: '서버 내용 불러오기' })).toBeNull();
    });
    expect(field('task-title').value).toBe('새 제목');
    expect(field('task-description').value).toBe('A 설명');
    expect(update).toHaveBeenCalledTimes(1);
  });
});
