import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type {
  AssigneeView,
  ItemView,
  ShiftProposalView,
  TaskDetailView,
  TaskInput,
} from '$lib/api/client';
import { api } from '$lib/api/client';
import { formatDay } from '$lib/display';
import { clashRows } from '$lib/features/tasks/labels';
import TaskBoard from '$lib/features/tasks/TaskBoard.svelte';
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
    const sent = update.mock.calls[1]?.[2];
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
    const box = ask.querySelector('[data-clash-field="description"] [data-clash-side="server"]');
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
      expect(await screen.findByText(/상태: 완료/)).toBeTruthy();
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
