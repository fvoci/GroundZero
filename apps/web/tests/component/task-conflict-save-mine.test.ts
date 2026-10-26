import './fixtures/task-statuses';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import type { MockInstance } from 'vitest';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type {
  AssigneeView,
  ItemView,
  ShiftProposalView,
  TaskDetailView,
  TaskInput,
} from '$lib/api/client';
import { api, ApiError, TaskRuleConflict } from '$lib/api/client';
import { formatDay } from '$lib/display';
import { clashRows } from '$lib/features/tasks/labels';
import TaskBoard from '$lib/features/tasks/TaskBoard.svelte';
import TaskDetail from '$lib/features/tasks/TaskDetail.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';
import { me } from '$lib/me.svelte';

// 업무 저장 충돌(409) 뒤 "내 입력으로 다시 저장"(실제 브라우저 점검에서 찾음): 전에는 내 폼 전체를 새 판 번호로 보내,
// 내가 고치지 않은 칸(다른 사람이 바꾼 제목 등)을 옛 값으로 조용히 되돌렸다. 이제 최신 판 위에 내가 고친 칸만 올리고,
// 같은 칸을 다른 사람도 다른 값으로 바꿨으면 양쪽 값을 보이고 덮어쓸지 묻는다(D-94).
beforeEach(() => {
  me.userId = 'user-a';
});
afterEach(() => {
  me.userId = '';
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
  predecessors: [],
  series: null,
  occurrence_date: null,
  assignees: [],
  status_category: 'todo',
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

function mount(current: TaskDetailView = v2, people: AssigneeView[] = []): void {
  vi.spyOn(api, 'task').mockResolvedValue(current);
  vi.spyOn(api, 'assignable').mockResolvedValue(people);
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

/** 덮어쓰기 확인의 한 줄에 보인 서버 값과 내 값(글 그대로). */
function sides(ask: HTMLElement, key: string): { server: string; mine: string } {
  const row = ask.querySelector(`[data-clash-field="${key}"]`);
  if (row === null) throw new Error(`no clash row ${key}`);
  const text = (side: string): string => {
    const el = row.querySelector(`[data-clash-side="${side}"]`);
    if (el === null) throw new Error(`no ${side} value in ${key}`);
    return el.textContent;
  };
  return { server: text('server'), mine: text('mine') };
}

describe('saving my input after a task conflict', () => {
  it('shows the latest custom status name in the conflict notice', async () => {
    await setLocale('ko', { reload: false });
    mount();
    vi.spyOn(api, 'updateTask').mockResolvedValue({
      kind: 'conflict',
      current: { ...v2, version: 3, status: '검수 대기' },
    });
    await screen.findByLabelText('설명');
    await type('task-description', '내 설명');
    await saveIntoConflict();
    expect(screen.getByText(/검수 대기/)).toBeInTheDocument();
  });

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
    expect(update).toHaveBeenNthCalledWith(
      1,
      expect.any(String),
      't1',
      2,
      { ...unchanged, description: 'B 설명' },
      [],
    );

    await fireEvent.click(saveMineButton());
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(2);
    });
    // 겹친 칸이 없으므로 묻지 않고 최신 판 번호로 저장한다.
    expect(update).toHaveBeenNthCalledWith(
      2,
      expect.any(String),
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
    expect(sides(ask, 'title')).toEqual({ server: '남의 제목', mine: '내 제목' });
    // 양쪽 값에는 이름표가 붙는다.
    expect(
      within(ask)
        .getAllByRole('term')
        .map((t) => t.textContent),
    ).toEqual(['서버의 지금 값', '내 값', '서버의 지금 값', '내 값']);
    const estimate = ask.querySelector('[data-clash-field="estimate_minutes"]');
    expect(estimate?.firstElementChild?.textContent).toBe('예상 시간(분)');
    expect(sides(ask, 'estimate_minutes')).toEqual({ server: '30', mine: '45' });
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
      expect.any(String),
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
    expect(await screen.findByText(/지금 제목: “세 번째 제목”, 상태: in_progress/)).toBeTruthy();
    expect(update).toHaveBeenNthCalledWith(
      2,
      expect.any(String),
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
      expect.any(String),
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
      expect.any(String),
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
    // 누른 단추가 사라지므로 다시 채운 폼의 첫 칸으로 초점을 옮긴다(전에는 문서로 빠졌다, 2차 검증 nit).
    await waitFor(() => {
      expect(document.activeElement?.id).toBe('task-title');
    });
  });
});

/** 요소나 그 조상이 inert인지(Svelte는 속성이 아니라 `inert` 프로퍼티로 둔다). */
function isInert(el: Element | null): boolean {
  for (let node = el; node !== null; node = node.parentElement) {
    if (node instanceof HTMLElement && node.inert) return true;
  }
  return false;
}

/** 다른 사람이 제목을 바꾼 판 3과의 충돌. */
const titleConflict: { kind: 'conflict'; current: TaskDetailView } = {
  kind: 'conflict',
  current: { ...v2, version: 3, title: '남의 제목' },
};

/** 탭 B: 제목을 고쳐 저장해 `titleConflict`로 충돌한 뒤 다시 저장하면 덮어쓸지 묻는다. */
async function askAboutTitle(): Promise<HTMLElement> {
  await screen.findByLabelText('설명');
  await type('task-title', '내 제목');
  await saveIntoConflict();
  await fireEvent.click(saveMineButton());
  return screen.findByRole('alertdialog');
}

// 리뷰에서 찾은 덮어쓰기 확인의 빈틈(D-94).
describe('the overwrite question after a task conflict', () => {
  // 리뷰 1·2: 일정(시작일·마감일·마일스톤)을 칸마다 골라, 확인에 보인 시작일이 병합된 마일스톤 때문에 저장되지
  // 않았다. 이제 일정은 한 묶음으로 묻고, 보인 내 일정이 그대로 보내진다.
  it('asks about the whole schedule and saves exactly the schedule it showed', async () => {
    await setLocale('ko', { reload: false });
    const dated: TaskDetailView = { ...v2, start_date: '2026-10-01', due_date: '2026-10-10' };
    mount(dated);
    const preview = vi
      .spyOn(api, 'previewSchedule')
      .mockResolvedValue({ shifts: [], undated: [], blocked: null });
    // 다른 사람이 마일스톤으로 바꿨다(마일스톤에는 시작일이 없다).
    const v3: TaskDetailView = { ...dated, version: 3, start_date: null, milestone: true };
    const update = vi
      .spyOn(api, 'updateTask')
      .mockResolvedValueOnce({ kind: 'conflict', current: v3 })
      .mockResolvedValueOnce({
        kind: 'updated',
        task: { ...dated, version: 4, start_date: '2026-10-03' },
      });
    await screen.findByLabelText('시작일');
    await type('task-start', '2026-10-03');
    await saveIntoConflict();

    await fireEvent.click(saveMineButton());
    const ask = await screen.findByRole('alertdialog');
    // 시작일 한 칸이 아니라 양쪽 일정 전체를 한 줄로 보인다.
    expect(within(ask).getAllByRole('listitem')).toHaveLength(1);
    const shown = sides(ask, 'schedule');
    expect(shown).toEqual({
      server: `마일스톤, 마감일 ${formatDay('2026-10-10')}`,
      mine: `시작일 ${formatDay('2026-10-03')}, 마감일 ${formatDay('2026-10-10')}`,
    });

    await fireEvent.click(within(ask).getByRole('button', { name: '내 값으로 덮어쓰고 저장' }));
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(2);
    });
    const sent = update.mock.calls[1]?.[3];
    if (sent === undefined) throw new Error('no second save');
    expect(sent).toEqual({
      ...unchanged,
      start_date: '2026-10-03',
      due_date: '2026-10-10',
      milestone: false,
    });
    // 확인에 보인 내 일정과 보낸 일정이 같다(보낸 값을 같은 글로 바꿔 견준다).
    expect(clashRows(['schedule'], sent, sent, new Map())[0]?.mine).toBe(shown.mine);
    expect(preview).toHaveBeenLastCalledWith('t1', '2026-10-03', '2026-10-10');
  });

  // 리뷰 3: 담당자를 순서까지 견주어, 같은 사람들을 순서만 달리(화면은 덧붙이고 서버는 이름순) 지정한 것도 묻었다.
  it('does not ask when the same people are assigned in another order', async () => {
    await setLocale('ko', { reload: false });
    const ga: AssigneeView = { user_id: 'u1', display_name: '가나' };
    const na: AssigneeView = { user_id: 'u2', display_name: '나라' };
    const current: TaskDetailView = { ...v2, assignees: [na] };
    mount(current, [ga, na]);
    // 다른 사람이 제목을 바꾸고 '가나'도 지정했다. 서버는 담당자를 이름순으로 돌려준다.
    const v3: TaskDetailView = { ...current, version: 3, title: '새 제목', assignees: [ga, na] };
    const update = vi
      .spyOn(api, 'updateTask')
      .mockResolvedValueOnce({ kind: 'conflict', current: v3 })
      .mockResolvedValueOnce({ kind: 'updated', task: { ...v3, version: 4 } });
    await fireEvent.click(await screen.findByRole('checkbox', { name: '가나' }));
    await saveIntoConflict();
    expect(update).toHaveBeenNthCalledWith(
      1,
      expect.any(String),
      't1',
      2,
      { ...unchanged, assignee_ids: ['u2', 'u1'] },
      [],
    );

    await fireEvent.click(saveMineButton());
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(2);
    });
    expect(screen.queryByRole('alertdialog')).toBeNull();
    expect(update).toHaveBeenNthCalledWith(
      2,
      expect.any(String),
      't1',
      3,
      { ...unchanged, title: '새 제목', assignee_ids: ['u1', 'u2'] },
      [],
    );
  });

  // 리뷰 4: 값을 80자에서 자르고 줄바꿈·공백을 모아, 서로 다른 설명이 똑같이 보였다.
  it('shows both descriptions in full with their line breaks', async () => {
    await setLocale('ko', { reload: false });
    mount();
    const theirs = `${'가'.repeat(100)}\n끝 A`;
    vi.spyOn(api, 'updateTask').mockResolvedValueOnce({
      kind: 'conflict',
      current: { ...v2, version: 3, description: theirs },
    });
    await screen.findByLabelText('설명');
    const mine = `${'가'.repeat(100)} 끝 B`;
    await type('task-description', mine);
    await saveIntoConflict();
    await fireEvent.click(saveMineButton());
    const ask = await screen.findByRole('alertdialog');
    expect(sides(ask, 'description')).toEqual({ server: theirs, mine });
    // 줄바꿈을 지켜 보이고, 길면 칸 안에서 스크롤한다.
    const box = ask.querySelector(
      '[data-clash-field="description"] [data-clash-side="server"] > [data-clash-value]',
    );
    expect(box?.className).toContain('whitespace-pre-wrap');
    expect(box?.className).toContain('overflow-auto');
  });

  // 리뷰 6: 확인이 떠 있는 동안 폼만 잠겨, 체크리스트·하위·선행·상위 업무를 바꾸면 다시 채우기가 확인을 말없이 닫았다.
  it('locks the rest of the task while asking, but not the question or the notice', async () => {
    await setLocale('ko', { reload: false });
    mount();
    vi.spyOn(api, 'updateTask').mockResolvedValueOnce(titleConflict);
    const ask = await askAboutTitle();
    const locked = [
      screen.getByText('KED-1'),
      field('task-title'),
      document.getElementById('parent-heading'),
      document.getElementById('new-subtask'),
      document.getElementById('predecessors-heading'),
      document.getElementById('task-t1-new'),
    ];
    for (const el of locked) {
      expect(el).not.toBeNull();
      expect(isInert(el)).toBe(true);
    }
    const usable = [
      within(ask).getByRole('button', { name: '내 값으로 덮어쓰고 저장' }),
      within(ask).getByRole('button', { name: '취소' }),
      screen.getByRole('button', { name: '서버 내용 불러오기' }),
    ];
    for (const el of usable) expect(isInert(el)).toBe(false);
  });

  // 리뷰 5: 덮어쓰기나 겹친 칸 없는 다시 저장이 후속 업무 미리보기를 띄우고, 거기서 승인한 저장이 다시 409면 초점이
  // 문서(body)로 빠졌다.
  it.each([
    ['after overwriting', true],
    ['without a clash', false],
  ])(
    'returns focus to saving mine again when a save approved in the preview conflicts %s',
    async (_name, clash) => {
      await setLocale('ko', { reload: false });
      const dated: TaskDetailView = { ...v2, start_date: '2026-10-19', due_date: '2026-10-21' };
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
        causes: [],
      };
      vi.spyOn(api, 'previewSchedule')
        .mockResolvedValueOnce({ shifts: [], undated: [], blocked: null })
        .mockResolvedValue({ shifts: [shift], undated: [], blocked: null });
      const v3: TaskDetailView = { ...dated, version: 3, title: '남의 제목' };
      const update = vi
        .spyOn(api, 'updateTask')
        .mockResolvedValueOnce({ kind: 'conflict', current: v3 })
        .mockResolvedValueOnce({
          kind: 'conflict',
          current: { ...v3, version: 4, status: 'done' },
        });
      await screen.findByLabelText('마감일');
      if (clash) await type('task-title', '내 제목');
      await type('task-due', '2026-10-25');
      await saveIntoConflict();

      await fireEvent.click(saveMineButton());
      if (clash) {
        const ask = await screen.findByRole('alertdialog');
        await fireEvent.click(within(ask).getByRole('button', { name: '내 값으로 덮어쓰고 저장' }));
      }
      const panel = (await screen.findByRole('heading', { name: /후속 업무 1개/ })).closest(
        'section',
      );
      if (panel === null) throw new Error('panel');
      await fireEvent.click(within(panel).getByRole('button', { name: '후속 업무도 옮겨 저장' }));
      await waitFor(() => {
        expect(update).toHaveBeenCalledTimes(2);
      });
      expect(await screen.findByText(/상태: done/)).toBeTruthy();
      await waitFor(() => {
        expect(document.activeElement?.id).toBe('task-save-mine');
      });
    },
  );
});

const item: ItemView = {
  id: 'i1',
  module_id: 'm1',
  workspace_id: 'ws-1',
  reference: 'KED',
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

// 리뷰 8: 업무 대화상자 안의 덮어쓰기 확인에서 Esc는 확인만 닫아야 한다(보내지 않고, 입력은 그대로, 초점은 "내 입력으로
// 다시 저장"으로). 전에는 Esc가 둘러싼 대화상자를 닫았다. 대화상자는 앱 바깥에 그려지므로 실제 보드로 연다.
describe('the overwrite question inside the task dialog', () => {
  it('cancels with Escape without closing the dialog', async () => {
    await setLocale('ko', { reload: false });
    window.history.replaceState(null, '', '/w/ws-1/i/i1');
    vi.spyOn(api, 'tasks').mockResolvedValue([v2]);
    vi.spyOn(api, 'savedViews').mockResolvedValue([]);
    vi.spyOn(api, 'syncRecurrences').mockResolvedValue(0);
    vi.spyOn(api, 'task').mockResolvedValue(v2);
    vi.spyOn(api, 'assignable').mockResolvedValue([]);
    vi.spyOn(api, 'taskBacklinks').mockResolvedValue([]);
    vi.spyOn(api, 'comments').mockResolvedValue([]);
    const back = vi.spyOn(window.history, 'back').mockImplementation(() => undefined);
    const update = vi.spyOn(api, 'updateTask').mockResolvedValueOnce(titleConflict);
    const view = render(TaskBoard, { props: { item, workspaces: [], openTaskId: undefined } });
    await fireEvent.click(await screen.findByRole('button', { name: /^KED-1\s*옛 제목$/ }));
    await view.rerender({ item, workspaces: [], openTaskId: 't1' });
    await screen.findByRole('dialog');

    const ask = await askAboutTitle();
    await waitFor(() => {
      expect(document.activeElement?.id).toBe('task-clash-heading');
    });
    await fireEvent.keyDown(document.activeElement ?? ask, { key: 'Escape' });
    await waitFor(() => {
      expect(screen.queryByRole('alertdialog')).toBeNull();
    });
    expect(back).not.toHaveBeenCalled();
    expect(screen.getByRole('dialog')).toBeTruthy();
    expect(update).toHaveBeenCalledTimes(1);
    expect(field('task-title').value).toBe('내 제목');
    await waitFor(() => {
      expect(document.activeElement?.id).toBe('task-save-mine');
    });
  });

  // 리뷰: 확인 상자의 단추가 아닌 곳(안내문·값 상자)을 누르면 초점이 둘러싼 대화상자로 가서, 그 뒤의 Esc가 확인 대신
  // 업무 대화상자를 닫고 저장하지 않은 입력을 버렸다. 상자가 초점을 받으면(누른 곳의 가장 가까운 초점 대상) Esc는 확인만
  // 거둔다.
  it('keeps focus in the question after a click on its text, so Escape only cancels it', async () => {
    await setLocale('ko', { reload: false });
    window.history.replaceState(null, '', '/w/ws-1/i/i1');
    vi.spyOn(api, 'tasks').mockResolvedValue([v2]);
    vi.spyOn(api, 'savedViews').mockResolvedValue([]);
    vi.spyOn(api, 'syncRecurrences').mockResolvedValue(0);
    vi.spyOn(api, 'task').mockResolvedValue(v2);
    vi.spyOn(api, 'assignable').mockResolvedValue([]);
    vi.spyOn(api, 'taskBacklinks').mockResolvedValue([]);
    vi.spyOn(api, 'comments').mockResolvedValue([]);
    const back = vi.spyOn(window.history, 'back').mockImplementation(() => undefined);
    const update = vi.spyOn(api, 'updateTask').mockResolvedValueOnce(titleConflict);
    const view = render(TaskBoard, { props: { item, workspaces: [], openTaskId: undefined } });
    await fireEvent.click(await screen.findByRole('button', { name: /^KED-1\s*옛 제목$/ }));
    await view.rerender({ item, workspaces: [], openTaskId: 't1' });
    await screen.findByRole('dialog');

    const ask = await askAboutTitle();
    // 브라우저는 누른 곳의 가장 가까운 초점 대상으로 초점을 옮긴다. jsdom은 그러지 않으므로 그 대상(상자)에 직접 둔다.
    ask.focus();
    expect(document.activeElement).toBe(ask);
    await fireEvent.keyDown(ask, { key: 'Escape' });
    await waitFor(() => {
      expect(screen.queryByRole('alertdialog')).toBeNull();
    });
    expect(back).not.toHaveBeenCalled();
    expect(screen.getByRole('dialog')).toBeTruthy();
    expect(update).toHaveBeenCalledTimes(1);
    expect(field('task-title').value).toBe('내 제목');
  });
});

// 2차 반박 검증·CodeRabbit의 남은 지적(D-94).
describe('the overwrite question after the second review', () => {
  // CodeRabbit: 이름이 같은 두 사람이 양쪽에 있으면 확인의 두 값이 똑같이 "Alex"로 보였다. 이름이 겹칠 때만 워크스페이스
  // 배지(D-17)처럼 id 끝 네 글자를 붙인다(폼의 이름표도 같은 글이라 확인의 값과 맞춰 볼 수 있다).
  it('tells apart people with the same name', async () => {
    await setLocale('ko', { reload: false });
    const alexA: AssigneeView = {
      user_id: '0190a000-0000-7000-8000-00000000a1a1',
      display_name: 'Alex',
    };
    const alexB: AssigneeView = {
      user_id: '0190a000-0000-7000-8000-00000000b2b2',
      display_name: 'Alex',
    };
    const bo: AssigneeView = { user_id: 'u3', display_name: '보라' };
    mount(v2, [alexA, alexB, bo]);
    vi.spyOn(api, 'updateTask').mockResolvedValueOnce({
      kind: 'conflict',
      current: { ...v2, version: 3, assignees: [alexB] },
    });
    await fireEvent.click(await screen.findByRole('checkbox', { name: 'Alex #a1a1' }));
    // 이름이 겹치지 않는 사람은 이름만 보인다.
    expect(screen.getByRole('checkbox', { name: '보라' })).toBeTruthy();
    await saveIntoConflict();
    await fireEvent.click(saveMineButton());
    const ask = await screen.findByRole('alertdialog');
    expect(sides(ask, 'assignee_ids')).toEqual({ server: 'Alex #b2b2', mine: 'Alex #a1a1' });
  });

  // 2차 검증: 서버는 겹친 태그를 하나로 저장한다(`validate_tags`). 따르지 않으면 "a, a"를 고친 칸으로 보아 쓸데없이
  // 묻고, 확인에 보인 값과 저장되는 값이 달랐다.
  it('reads tags the way the server stores them, without repeats', async () => {
    await setLocale('ko', { reload: false });
    mount({ ...v2, tags: ['a'] });
    const update = vi
      .spyOn(api, 'updateTask')
      .mockResolvedValueOnce({ kind: 'conflict', current: { ...v2, version: 3, tags: ['b'] } })
      .mockResolvedValueOnce({ kind: 'updated', task: { ...v2, version: 4, tags: ['b'] } });
    await screen.findByLabelText('설명');
    await type('task-tags', 'a, a');
    await saveIntoConflict();
    await fireEvent.click(saveMineButton());
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(2);
    });
    expect(screen.queryByRole('alertdialog')).toBeNull();
    expect(update).toHaveBeenNthCalledWith(
      2,
      expect.any(String),
      't1',
      3,
      { ...unchanged, tags: ['b'] },
      [],
    );
  });

  it('shows and saves my tags without repeats', async () => {
    await setLocale('ko', { reload: false });
    mount();
    const update = vi
      .spyOn(api, 'updateTask')
      .mockResolvedValueOnce({ kind: 'conflict', current: { ...v2, version: 3, tags: ['x'] } })
      .mockResolvedValueOnce({ kind: 'updated', task: { ...v2, version: 4, tags: ['b', 'a'] } });
    await screen.findByLabelText('설명');
    await type('task-tags', 'b, a, b');
    await saveIntoConflict();
    await fireEvent.click(saveMineButton());
    const ask = await screen.findByRole('alertdialog');
    expect(sides(ask, 'tags')).toEqual({ server: 'x', mine: 'b, a' });
    await fireEvent.click(within(ask).getByRole('button', { name: '내 값으로 덮어쓰고 저장' }));
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(2);
    });
    expect(update).toHaveBeenNthCalledWith(
      2,
      expect.any(String),
      't1',
      3,
      { ...unchanged, tags: ['b', 'a'] },
      [],
    );
  });

  // 2차 검증: 앞뒤 공백만 친 제목은 저장되는 값이 그대로라 고친 칸이 아니다. 묻지 않고 다른 사람이 바꾼 제목을 따른다.
  it('does not treat surrounding spaces as an edit', async () => {
    await setLocale('ko', { reload: false });
    mount();
    const update = vi
      .spyOn(api, 'updateTask')
      .mockResolvedValueOnce(titleConflict)
      .mockResolvedValueOnce({ kind: 'updated', task: { ...titleConflict.current, version: 4 } });
    await screen.findByLabelText('설명');
    await type('task-title', `${v2.title} `);
    await saveIntoConflict();
    await fireEvent.click(saveMineButton());
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(2);
    });
    expect(screen.queryByRole('alertdialog')).toBeNull();
    expect(update).toHaveBeenNthCalledWith(
      2,
      expect.any(String),
      't1',
      3,
      { ...unchanged, title: '남의 제목' },
      [],
    );
  });

  // 2차 검증: 줄 끝 공백만 다른 설명처럼 서로 다른 값이 화면에서 똑같아 보이면 그렇다고 알린다.
  it('says so when the two values look the same', async () => {
    await setLocale('ko', { reload: false });
    mount();
    vi.spyOn(api, 'updateTask').mockResolvedValueOnce({
      kind: 'conflict',
      current: { ...v2, version: 3, description: '가 \n나' },
    });
    await screen.findByLabelText('설명');
    await type('task-description', '가\n나');
    await saveIntoConflict();
    await fireEvent.click(saveMineButton());
    const ask = await screen.findByRole('alertdialog');
    const row = ask.querySelector('[data-clash-field="description"]');
    expect(row?.textContent).toContain('같아 보이지만');
    // 다르게 보이는 값에는 붙지 않는다.
    expect(ask.querySelectorAll('[data-clash-looks-same]')).toHaveLength(1);
  });

  // 2차 검증 nit: 충돌 화면이 떠 있을 때 폼의 "저장"으로 다시 409가 나면, 누른 단추가 그대로 있는데도 초점을
  // "내 입력으로 다시 저장"으로 옮겼다.
  it('leaves focus on the form save button when saving the form conflicts again', async () => {
    await setLocale('ko', { reload: false });
    mount();
    vi.spyOn(api, 'updateTask')
      .mockResolvedValueOnce(titleConflict)
      .mockResolvedValueOnce({
        kind: 'conflict',
        current: { ...titleConflict.current, version: 4, status: 'done' },
      });
    await screen.findByLabelText('설명');
    await type('task-description', '내 설명');
    await saveIntoConflict();
    const save = document.getElementById('task-save');
    if (save === null) throw new Error('task-save');
    save.focus();
    await fireEvent.click(save);
    expect(await screen.findByText(/상태: done/)).toBeTruthy();
    await new Promise((resolve) => requestAnimationFrame(resolve));
    await new Promise((resolve) => requestAnimationFrame(resolve));
    expect(document.activeElement?.id).toBe('task-save');
  });

  // 덮어쓰기가 다시 409면(그사이 같은 칸이 또 바뀜) 입력을 지키고, 다음 "내 입력으로 다시 저장"은 더 새 값으로 다시 묻고
  // 그 판 번호로 보낸다.
  it('asks again with the newer value when overwriting conflicts again', async () => {
    await setLocale('ko', { reload: false });
    mount();
    const v4: TaskDetailView = { ...titleConflict.current, version: 4, title: '세 번째 제목' };
    const update = vi
      .spyOn(api, 'updateTask')
      .mockResolvedValueOnce(titleConflict)
      .mockResolvedValueOnce({ kind: 'conflict', current: v4 })
      .mockResolvedValueOnce({ kind: 'updated', task: { ...v4, version: 5, title: '내 제목' } });
    const ask = await askAboutTitle();
    await fireEvent.click(within(ask).getByRole('button', { name: '내 값으로 덮어쓰고 저장' }));
    expect(await screen.findByText(/지금 제목: “세 번째 제목”/)).toBeTruthy();
    expect(update).toHaveBeenNthCalledWith(
      2,
      expect.any(String),
      't1',
      3,
      { ...unchanged, title: '내 제목' },
      [],
    );
    expect(field('task-title').value).toBe('내 제목');
    await waitFor(() => {
      expect(document.activeElement?.id).toBe('task-save-mine');
    });

    await fireEvent.click(saveMineButton());
    const again = await screen.findByRole('alertdialog');
    expect(sides(again, 'title')).toEqual({ server: '세 번째 제목', mine: '내 제목' });
    await fireEvent.click(within(again).getByRole('button', { name: '내 값으로 덮어쓰고 저장' }));
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(3);
    });
    expect(update).toHaveBeenNthCalledWith(
      3,
      expect.any(String),
      't1',
      4,
      { ...unchanged, title: '내 제목' },
      [],
    );
  });

  // 승인한 덮어쓰기가 실패하면(서버에 닿지 않음) 입력과 충돌 화면이 남는다. 이미 승인한 판 3 위에서 다시 시도하므로
  // 다시 묻지 않고 같은 판 번호로 보낸다(그사이 또 바뀌었으면 서버가 409로 답한다).
  it('keeps my input when overwriting fails and retries on the version I approved', async () => {
    await setLocale('ko', { reload: false });
    mount();
    const update = vi
      .spyOn(api, 'updateTask')
      .mockResolvedValueOnce(titleConflict)
      .mockRejectedValueOnce(new Error('boom'))
      .mockResolvedValueOnce({
        kind: 'updated',
        task: { ...titleConflict.current, version: 4, title: '내 제목' },
      });
    const ask = await askAboutTitle();
    await fireEvent.click(within(ask).getByRole('button', { name: '내 값으로 덮어쓰고 저장' }));
    expect(await screen.findByText('저장하지 못했습니다.')).toBeTruthy();
    expect(field('task-title').value).toBe('내 제목');
    await waitFor(() => {
      expect(document.activeElement?.id).toBe('task-save-mine');
    });
    await fireEvent.click(saveMineButton());
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(3);
    });
    expect(screen.queryByRole('alertdialog')).toBeNull();
    expect(update).toHaveBeenNthCalledWith(
      3,
      expect.any(String),
      't1',
      3,
      { ...unchanged, title: '내 제목' },
      [],
    );
  });
});

// CodeRabbit·2차 검증: 확인이 뜨기 전에 보낸 체크리스트(반복·선행·상위 업무도 같은 `refresh`) 요청이 확인이 떠 있는
// 동안 돌아오면, 다시 채우기가 확인을 말없이 닫고 초점이 문서로 빠졌다. 이제 확인과 입력을 지키고, 받은 판이 더 새면 그
// 판으로 다시 묻는다(충돌 알림도 그 판을 보인다).
describe('a late response while the overwrite question is open', () => {
  const listed: TaskDetailView = {
    ...v2,
    checklist: [{ id: 'c1', text: '검토', checked: false }],
    checklist_total: 1,
  };

  async function askWithLateChecklist(
    conflictWith: TaskDetailView,
    reloaded: TaskDetailView,
  ): Promise<{ finish: () => void; update: MockInstance<typeof api.updateTask> }> {
    mount(listed);
    let finish: () => void = () => undefined;
    vi.spyOn(api, 'updateChecklistItem').mockReturnValue(
      new Promise<undefined>((resolve) => {
        finish = () => {
          resolve(undefined);
        };
      }),
    );
    const update = vi
      .spyOn(api, 'updateTask')
      .mockResolvedValueOnce({ kind: 'conflict', current: conflictWith })
      .mockResolvedValueOnce({ kind: 'updated', task: { ...reloaded, version: 9 } });
    await screen.findByLabelText('설명');
    await fireEvent.click(screen.getByRole('checkbox', { name: '미완료 항목: 검토' }));
    vi.mocked(api.task).mockResolvedValue(reloaded);
    await type('task-title', '내 제목');
    await saveIntoConflict();
    await fireEvent.click(saveMineButton());
    await screen.findByRole('alertdialog');
    await waitFor(() => {
      expect(document.activeElement?.id).toBe('task-clash-heading');
    });
    return { finish, update };
  }

  it('keeps the question when the reloaded version is older than the conflict', async () => {
    await setLocale('ko', { reload: false });
    const checked = [{ id: 'c1', text: '검토', checked: true }];
    const v3: TaskDetailView = { ...listed, version: 3, checklist: checked };
    const v4: TaskDetailView = { ...v3, version: 4, title: '남의 제목' };
    const { finish, update } = await askWithLateChecklist(v4, v3);
    finish();
    await waitFor(() => {
      expect(screen.getByRole('checkbox', { name: '완료 항목: 검토' })).toBeTruthy();
    });
    const ask = screen.getByRole('alertdialog');
    expect(sides(ask, 'title')).toEqual({ server: '남의 제목', mine: '내 제목' });
    expect(ask.contains(document.activeElement)).toBe(true);
    expect(field('task-title').value).toBe('내 제목');
    await fireEvent.click(within(ask).getByRole('button', { name: '내 값으로 덮어쓰고 저장' }));
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(2);
    });
    expect(update).toHaveBeenNthCalledWith(
      2,
      expect.any(String),
      't1',
      4,
      { ...unchanged, title: '내 제목' },
      [],
    );
  });

  it('asks again with the reloaded version when it is newer than the conflict', async () => {
    await setLocale('ko', { reload: false });
    const v3: TaskDetailView = { ...listed, version: 3, title: '남의 제목' };
    const v4: TaskDetailView = {
      ...v3,
      version: 4,
      title: '네 번째 제목',
      checklist: [{ id: 'c1', text: '검토', checked: true }],
    };
    const { finish, update } = await askWithLateChecklist(v3, v4);
    finish();
    expect(await screen.findByText(/지금 제목: “네 번째 제목”/)).toBeTruthy();
    const ask = screen.getByRole('alertdialog');
    expect(sides(ask, 'title')).toEqual({ server: '네 번째 제목', mine: '내 제목' });
    expect(ask.contains(document.activeElement)).toBe(true);
    expect(field('task-title').value).toBe('내 제목');
    await fireEvent.click(within(ask).getByRole('button', { name: '내 값으로 덮어쓰고 저장' }));
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(2);
    });
    expect(update).toHaveBeenNthCalledWith(
      2,
      expect.any(String),
      't1',
      4,
      { ...unchanged, title: '내 제목' },
      [],
    );
  });

  it('closes the question and returns to saving mine when the newer version no longer clashes', async () => {
    await setLocale('ko', { reload: false });
    const v3: TaskDetailView = { ...listed, version: 3, title: '남의 제목' };
    // 다른 사람이 제목을 되돌렸다.
    const v4: TaskDetailView = {
      ...listed,
      version: 4,
      checklist: [{ id: 'c1', text: '검토', checked: true }],
    };
    const { finish, update } = await askWithLateChecklist(v3, v4);
    finish();
    await waitFor(() => {
      expect(screen.queryByRole('alertdialog')).toBeNull();
    });
    await waitFor(() => {
      expect(document.activeElement?.id).toBe('task-save-mine');
    });
    expect(field('task-title').value).toBe('내 제목');
    expect(update).toHaveBeenCalledTimes(1);
  });
});

// 사용자 결정 B(2026-10-22): 값 상자는 길면 상자 안에서 스크롤하는데, Safari는 스크롤되는 영역에 키보드 초점을 주지
// 않아(Chrome·Firefox는 줌) 넘친 글을 키보드로 읽을 수 없었다. 넘치는 상자만 이름 있는 영역으로 탭 순서에 들고(초점을
// 받으면 화살표·Page 키로 스크롤), 짧은 상자는 역할·탭 정지를 더하지 않는다. jsdom은 크기를 재지 않으므로 서버 쪽 상자만
// 넘친다고 둔다(실제 Safari 확인은 이 환경에 없다).
describe('long values in the overwrite question', () => {
  it('lets the keyboard reach a value box that scrolls, and leaves short ones alone', async () => {
    await setLocale('ko', { reload: false });
    vi.spyOn(Element.prototype, 'scrollHeight', 'get').mockImplementation(function scrollHeight(
      this: Element,
    ) {
      return this.closest('[data-clash-side="server"]') === null ? 0 : 900;
    });
    vi.spyOn(Element.prototype, 'clientHeight', 'get').mockReturnValue(240);
    mount();
    const long = Array.from({ length: 80 }, (_, i) => `${String(i + 1)}번째 줄`).join('\n');
    vi.spyOn(api, 'updateTask').mockResolvedValueOnce({
      kind: 'conflict',
      current: { ...v2, version: 3, description: long },
    });
    await screen.findByLabelText('설명');
    await type('task-description', '내 설명');
    await saveIntoConflict();
    await fireEvent.click(saveMineButton());
    const ask = await screen.findByRole('alertdialog', { name: '다른 사람도 바꾼 칸이 있습니다' });

    const region = await within(ask).findByRole('region', { name: '설명 서버의 지금 값' });
    expect(region.tabIndex).toBe(0);
    expect(region.textContent).toBe(long);
    // 짧은 내 값은 영역도 탭 정지도 아니다.
    expect(within(ask).getAllByRole('region')).toHaveLength(1);
    const mine = ask.querySelector('[data-clash-side="mine"]');
    expect(mine?.querySelector('[tabindex]') ?? null).toBeNull();
    expect(mine?.textContent).toBe('내 설명');

    // 영역에서 Esc는 확인만 거둔다.
    region.focus();
    await fireEvent.keyDown(region, { key: 'Escape' });
    await waitFor(() => {
      expect(screen.queryByRole('alertdialog')).toBeNull();
    });
    expect(field('task-description').value).toBe('내 설명');
  });
});
it('preserves and hides task input after account_changed without another save', async () => {
  await setLocale('ko', { reload: false });
  mount();
  const update = vi
    .spyOn(api, 'updateTask')
    .mockRejectedValue(new ApiError(409, 'account_changed', 'account_changed'));
  await screen.findByLabelText('제목');
  await type('task-title', '계정 A의 비공개 입력');
  await fireEvent.click(screen.getByRole('button', { name: '저장' }));
  await screen.findByText(/계정이 바뀌어 입력을 숨겼습니다/);
  expect(field('task-title')).toHaveValue('계정 A의 비공개 입력');
  expect(field('task-title')).not.toBeVisible();
  expect(update).toHaveBeenCalledTimes(1);
});

it('keeps recurrence input on 409, shows the latest rule and explicitly resaves once', async () => {
  await setLocale('ko', { reload: false });
  const base = { freq: 'daily' as const, interval: 1, weekdays: [], until: null, count: null };
  const latest = { ...base, interval: 3 };
  const initial = {
    ...v2,
    due_date: '2026-10-26',
    series: { id: 'series', root_task_id: v2.id, timezone: 'Asia/Seoul', rule: base },
  };
  mount(initial);
  const save = vi
    .spyOn(api, 'setTaskRecurrence')
    .mockRejectedValueOnce(
      new TaskRuleConflict({ ...initial, series: { ...initial.series, rule: latest } }),
    )
    .mockReturnValueOnce(new Promise(() => undefined));
  await screen.findByLabelText('간격');
  await fireEvent.input(screen.getByLabelText('간격'), { target: { value: '2' } });
  await fireEvent.click(screen.getByRole('button', { name: '반복 저장' }));
  await screen.findByText(/그사이 다른 사람이 반복 규칙/);
  expect(screen.getByLabelText('간격')).toHaveValue('2');
  expect(save).toHaveBeenCalledTimes(1);
  await fireEvent.click(screen.getByRole('button', { name: '반복 규칙: 내 입력으로 다시 저장' }));
  await fireEvent.click(screen.getByRole('button', { name: '반복 규칙: 내 입력으로 다시 저장' }));
  expect(save).toHaveBeenCalledTimes(2);
  expect(save.mock.calls[1]?.[2]).toEqual({ ...base, interval: 2 });
  expect(save.mock.calls[1]?.[4]).toEqual(latest);
});
