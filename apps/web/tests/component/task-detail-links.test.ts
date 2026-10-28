import { taskData as work } from '$lib/features/tasks/detail-data';
import './fixtures/task-statuses';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { ShiftProposalView, TaskLinkView, TaskView } from '$lib/api/client';
import { api, NetworkError } from '$lib/api/client';
import * as feedback from '$lib/features/feedback/feedback.svelte';
import TaskDetail from '$lib/features/tasks/TaskDetail.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

// 업무 상세의 선행 관계 종류·지연일(D-91): 관계를 글로 보이고, 고치면 먼저 영향을 미리 보고 승인한 뒤에만 저장한다.
// 후속 업무 목록은 다른 업무의 같은 관계 목록을 거꾸로 찾는다.
afterEach(() => {
  vi.restoreAllMocks();
});

function task(
  id: string,
  title: string,
  links: TaskLinkView[] = [],
  dates?: [string, string],
): TaskView {
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
    start_date: dates?.[0] ?? null,
    due_date: dates?.[1] ?? null,
    estimate_minutes: null,
    tags: [],
    milestone: false,
    predecessors: links,
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
    updated_at: '2026-10-21T00:00:00Z',
  };
}

const design = task('a', '설계', [], ['2026-10-19', '2026-10-21']);
const build = task(
  'b',
  '구현',
  [{ task_id: 'a', kind: 'SS', lag_days: 1 }],
  ['2026-10-20', '2026-10-22'],
);
const review = task(
  'c',
  '검토',
  [{ task_id: 'b', kind: 'FF', lag_days: 2 }],
  ['2026-10-23', '2026-10-24'],
);
const shift: ShiftProposalView = {
  task_id: 'b',
  reference: 'KED-b',
  title: '구현',
  base_version: 1,
  from_start_date: '2026-10-20',
  from_due_date: '2026-10-22',
  start_date: '2026-10-22',
  due_date: '2026-10-24',
  causes: [
    {
      predecessor_id: 'a',
      predecessor_reference: 'KED-a',
      predecessor_title: '설계',
      kind: 'SS',
      lag_days: 3,
    },
  ],
};

function mount(current: TaskView = build, others: TaskView[] = [design, review]): void {
  vi.spyOn(work, 'task').mockResolvedValue({ ...current, checklist: [] });
  vi.spyOn(work, 'assignable').mockResolvedValue([]);
  vi.spyOn(api, 'taskBacklinks').mockResolvedValue([]);
  vi.spyOn(api, 'comments').mockResolvedValue([]);
  render(TaskDetail, {
    props: {
      taskId: current.id,
      tasks: [...others, current],
      workspaces: [],
      onchanged: () => undefined,
      onopen: () => undefined,
    },
  });
}

async function editLagTo(lag: string): Promise<void> {
  await fireEvent.click(await screen.findByRole('button', { name: 'KED-a 설계 관계 고치기' }));
  await fireEvent.input(screen.getByLabelText('지연(일)'), { target: { value: lag } });
  await fireEvent.click(screen.getByRole('button', { name: '관계 저장' }));
}

describe('task detail dependency kinds', () => {
  it('shows a subtask custom status name instead of its category label', async () => {
    await setLocale('ko', { reload: false });
    mount(build, [{ ...review, parent_id: build.id, status: '검수 대기' }]);
    const section = (await screen.findByRole('heading', { name: '하위 업무' })).closest('section');
    if (section === null) throw new Error('section');
    expect(within(section).getByText('검수 대기')).toBeInTheDocument();
  });

  it('describes predecessors and followers with their type and lag', async () => {
    await setLocale('ko', { reload: false });
    mount();
    const preds = (await screen.findByRole('heading', { name: '선행 업무' })).closest('section');
    if (preds === null) throw new Error('section');
    const row = await within(preds).findByText('시작 후 시작 (SS), 1일 뒤');
    expect(row.closest('[data-predecessor]')?.getAttribute('data-predecessor')).toBe('a');
    // 후속 업무는 c의 관계 목록(b → c, FF·2)을 거꾸로 찾은 것이다.
    const succs = screen.getByRole('heading', { name: '후속 업무' }).closest('section');
    if (succs === null) throw new Error('section');
    expect(within(succs).getByRole('button', { name: 'KED-c 검토' })).toBeTruthy();
    expect(within(succs).getByText('종료 후 종료 (FF), 2일 뒤')).toBeTruthy();
  });

  it('previews the change, shows the moved task with its cause, and saves only after approval', async () => {
    await setLocale('ko', { reload: false });
    const done = vi.spyOn(feedback, 'notifyDone');
    mount();
    const preview = vi
      .spyOn(work, 'previewLink')
      .mockResolvedValue({ shifts: [shift], undated: [], blocked: null });
    const set = vi.spyOn(work, 'setPredecessor').mockResolvedValue({
      kind: 'updated',
      task: {
        ...build,
        version: 2,
        predecessors: [{ task_id: 'a', kind: 'SS', lag_days: 3 }],
        checklist: [],
      },
    });
    await editLagTo('3');
    expect(preview).toHaveBeenCalledWith('b', 'a', 'SS', 3);
    // 미리보기만으로는 아무것도 저장하지 않는다.
    expect(set).not.toHaveBeenCalled();
    const panel = (
      await screen.findByRole('heading', { name: '후속 업무 1개가 관계를 지키지 못합니다' })
    ).closest('section');
    if (panel === null) throw new Error('panel');
    expect(within(panel).getByText(/2026-10-20~2026-10-22 → 2026-10-22~2026-10-24/)).toBeTruthy();
    expect(within(panel).getByText('원인: KED-a 설계 — 시작 후 시작 (SS), 3일 뒤')).toBeTruthy();
    await fireEvent.click(
      within(panel).getByRole('button', { name: '관계 저장하고 후속 업무 옮기기' }),
    );
    await waitFor(() => {
      expect(set).toHaveBeenCalledWith(
        expect.any(String),
        'b',
        'a',
        'SS',
        3,
        { kind: 'SS', lag_days: 1 },
        [{ task_id: 'b', base_version: 1, start_date: '2026-10-22', due_date: '2026-10-24' }],
      );
    });
    // 일반 완료는 토스트 한 번이고 화면 안 완료 문구는 없다(D-82).
    await waitFor(() => {
      expect(done).toHaveBeenCalledOnce();
    });
    expect(done).toHaveBeenCalledWith('관계를 저장했습니다.', 'task-link');
    expect(screen.queryByText('관계를 저장했습니다.')).toBeNull();
  });

  it('saves nothing when the preview is cancelled and returns focus to the save button', async () => {
    await setLocale('ko', { reload: false });
    mount();
    vi.spyOn(work, 'previewLink').mockResolvedValue({
      shifts: [shift],
      undated: [],
      blocked: null,
    });
    const set = vi.spyOn(work, 'setPredecessor');
    await editLagTo('3');
    const panel = (await screen.findByRole('heading', { name: /관계를 지키지 못합니다/ })).closest(
      'section',
    );
    if (panel === null) throw new Error('panel');
    await fireEvent.click(within(panel).getByRole('button', { name: '취소' }));
    await waitFor(() => {
      expect(screen.queryByRole('heading', { name: /관계를 지키지 못합니다/ })).toBeNull();
    });
    expect(set).not.toHaveBeenCalled();
    // 편집 칸은 그대로 남고 초점은 관계 저장 단추로 돌아온다(대화상자 맨 위로 가지 않게).
    await waitFor(() => {
      expect(document.activeElement?.id).toBe('link-save');
    });
    expect(screen.getByLabelText<HTMLInputElement>('지연(일)').value).toBe('3');
  });

  it('asks again with the recalculated plan when the approved list went stale', async () => {
    await setLocale('ko', { reload: false });
    mount();
    const moved = { ...shift, start_date: '2026-10-23', due_date: '2026-10-25' };
    const preview = vi
      .spyOn(work, 'previewLink')
      .mockResolvedValueOnce({ shifts: [shift], undated: [], blocked: null })
      .mockResolvedValueOnce({ shifts: [moved], undated: [], blocked: null });
    const set = vi
      .spyOn(work, 'setPredecessor')
      .mockResolvedValue({ kind: 'shift_conflict', current: { ...build, checklist: [] } });
    await editLagTo('3');
    await fireEvent.click(
      await screen.findByRole('button', { name: '관계 저장하고 후속 업무 옮기기' }),
    );
    expect(await screen.findByText('후속 업무가 그사이 바뀌어 다시 계산했습니다.')).toBeTruthy();
    expect(screen.getByText(/→ 2026-10-23~2026-10-25/)).toBeTruthy();
    expect(preview).toHaveBeenCalledTimes(2);
    expect(set).toHaveBeenCalledTimes(1);
  });

  it('saves the link without asking again when the recalculated plan moves nothing', async () => {
    // R5 최종 A FA-2(D-91 개정·D-125): 다시 계산한 계획에 밀릴 업무가 없으면 묻지 않고 저장하고 그렇게 알린다.
    await setLocale('ko', { reload: false });
    const done = vi.spyOn(feedback, 'notifyDone');
    mount();
    const preview = vi
      .spyOn(work, 'previewLink')
      .mockResolvedValueOnce({ shifts: [shift], undated: [], blocked: null })
      .mockResolvedValue({ shifts: [], undated: [], blocked: null });
    const set = vi
      .spyOn(work, 'setPredecessor')
      .mockResolvedValueOnce({ kind: 'shift_conflict', current: { ...build, checklist: [] } })
      .mockResolvedValue({
        kind: 'updated',
        task: {
          ...build,
          version: 2,
          predecessors: [{ task_id: 'a', kind: 'SS', lag_days: 3 }],
          checklist: [],
        },
      });
    await editLagTo('3');
    await fireEvent.click(
      await screen.findByRole('button', { name: '관계 저장하고 후속 업무 옮기기' }),
    );
    await waitFor(() => {
      expect(set).toHaveBeenCalledTimes(2);
    });
    expect(preview).toHaveBeenCalledTimes(2);
    expect(set.mock.calls[1]?.[6]).toEqual([]);
    // 일반 완료는 토스트 한 번이다(D-82).
    await waitFor(() => {
      expect(done).toHaveBeenCalledOnce();
    });
    expect(done).toHaveBeenCalledWith(
      '관계를 저장했습니다. 밀릴 업무가 없어 그대로 저장했습니다.',
      'task-link',
    );
    expect(screen.queryByText(/밀릴 업무가 없어 그대로 저장했습니다/)).toBeNull();
    expect(screen.queryByRole('heading', { name: /관계를 지키지 못합니다/ })).toBeNull();
  });

  it('saves the dates without asking again when the recalculated plan moves nothing', async () => {
    // R5 최종 A FA-2: 상세의 일정 저장도 같다(재현 R-shift).
    await setLocale('ko', { reload: false });
    const done = vi.spyOn(feedback, 'notifyDone');
    mount(design, [build, review]);
    const preview = vi
      .spyOn(work, 'previewSchedule')
      .mockResolvedValueOnce({ shifts: [shift], undated: [], blocked: null })
      .mockResolvedValue({ shifts: [], undated: [], blocked: null });
    const update = vi
      .spyOn(work, 'updateTask')
      .mockResolvedValueOnce({ kind: 'shift_conflict', current: { ...design, checklist: [] } })
      .mockResolvedValue({
        kind: 'updated',
        task: { ...design, due_date: '2026-10-22', version: 2, checklist: [] },
      });
    await fireEvent.input(await screen.findByLabelText('마감일'), {
      target: { value: '2026-10-22' },
    });
    await fireEvent.click(screen.getByRole('button', { name: '저장' }));
    await fireEvent.click(await screen.findByRole('button', { name: '후속 업무도 옮겨 저장' }));
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(2);
    });
    expect(preview).toHaveBeenCalledTimes(2);
    expect(update.mock.calls[1]?.[4]).toEqual([]);
    await waitFor(() => {
      expect(done).toHaveBeenCalledWith(
        expect.stringContaining('밀릴 업무가 없어 그대로 저장했습니다.'),
        'task-save',
      );
    });
    expect(screen.queryByText(/관계를 지키지 못합니다/)).toBeNull();
  });

  it('moves focus to save mine when the retry without shifts meets a conflict', async () => {
    // 수정 차분 검수 RA-2: 미리보기를 승인한 저장의 재시도가 409면 패널과 누른 단추가 사라지므로 "내 입력으로 다시 저장"으로
    // 초점을 돌린다. 재시도 전에 미리보기를 비워도 승인한 저장이었다는 것은 잃지 않는다.
    await setLocale('ko', { reload: false });
    mount(design, [build, review]);
    vi.spyOn(work, 'previewSchedule')
      .mockResolvedValueOnce({ shifts: [shift], undated: [], blocked: null })
      .mockResolvedValue({ shifts: [], undated: [], blocked: null });
    const update = vi
      .spyOn(work, 'updateTask')
      .mockResolvedValueOnce({ kind: 'shift_conflict', current: { ...design, checklist: [] } })
      .mockResolvedValueOnce({
        kind: 'conflict',
        current: { ...design, title: '남의 제목', version: 2, checklist: [] },
      });
    await fireEvent.input(await screen.findByLabelText('마감일'), {
      target: { value: '2026-10-22' },
    });
    await fireEvent.click(screen.getByRole('button', { name: '저장' }));
    const approve = await screen.findByRole('button', { name: '후속 업무도 옮겨 저장' });
    approve.focus();
    await fireEvent.click(approve);
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(2);
    });
    expect(update.mock.calls[1]?.[4]).toEqual([]);
    await waitFor(() => {
      expect(document.activeElement?.id).toBe('task-save-mine');
    });
  });

  it('keeps the typed values and shows the latest link when the link changed elsewhere meanwhile', async () => {
    // 감사 W4: 편집을 시작한 관계(SS·1)가 그사이 다른 곳에서 FF·4로 바뀌었다. 서버가 409 link_conflict로 거절하면
    // 덮지 않고 최신 관계를 알리며, 입력한 지연일은 그대로 두고 다음 저장은 최신 관계를 기준으로 보낸다.
    await setLocale('ko', { reload: false });
    mount();
    vi.spyOn(work, 'previewLink').mockResolvedValue({ shifts: [], undated: [], blocked: null });
    const latest = {
      ...build,
      version: 2,
      predecessors: [{ task_id: 'a', kind: 'FF' as const, lag_days: 4 }],
      checklist: [],
    };
    const set = vi
      .spyOn(work, 'setPredecessor')
      .mockResolvedValueOnce({ kind: 'link_conflict', current: latest })
      .mockResolvedValueOnce({
        kind: 'updated',
        task: { ...latest, version: 3, predecessors: [{ task_id: 'a', kind: 'SS', lag_days: 2 }] },
      });
    await editLagTo('2');
    await waitFor(() => {
      expect(set).toHaveBeenCalledTimes(1);
    });
    expect(set.mock.calls[0]?.[5]).toEqual({ kind: 'SS', lag_days: 1 });
    expect((await screen.findByRole('alert')).textContent).toContain(
      '지금: 종료 후 종료 (FF), 4일 뒤',
    );
    const lag = screen.getByLabelText('지연(일)');
    expect(lag instanceof HTMLInputElement ? lag.value : '').toBe('2');
    await fireEvent.click(screen.getByRole('button', { name: '관계 저장' }));
    await waitFor(() => {
      expect(set).toHaveBeenCalledTimes(2);
    });
    expect(set.mock.calls[1]?.slice(1, 6)).toEqual([
      'b',
      'a',
      'SS',
      2,
      { kind: 'FF', lag_days: 4 },
    ]);
  });

  it('saves directly when nothing moves and says which undated followers were not checked', async () => {
    await setLocale('ko', { reload: false });
    const done = vi.spyOn(feedback, 'notifyDone');
    mount();
    vi.spyOn(work, 'previewLink').mockResolvedValue({
      shifts: [],
      undated: [{ task_id: 'c', reference: 'KED-c', title: '검토' }],
      blocked: null,
    });
    const set = vi
      .spyOn(work, 'setPredecessor')
      .mockResolvedValue({ kind: 'updated', task: { ...build, checklist: [] } });
    await editLagTo('0');
    await waitFor(() => {
      expect(set).toHaveBeenCalledWith(
        expect.any(String),
        'b',
        'a',
        'SS',
        0,
        { kind: 'SS', lag_days: 1 },
        [],
      );
    });
    // 날짜 없는 업무 안내는 완료 토스트 문구에 붙는다(D-82).
    await waitFor(() => {
      expect(done).toHaveBeenCalledOnce();
    });
    expect(done).toHaveBeenCalledWith(
      expect.stringMatching(/^관계를 저장했습니다\. 날짜가 없어 옮기지 않는 업무: KED-c 검토/),
      'task-link',
    );
    expect(screen.queryByText(/날짜가 없어 옮기지 않는 업무/)).toBeNull();
  });

  it('explains a plan with a date outside the saveable range and only offers saving the link alone', async () => {
    await setLocale('ko', { reload: false });
    mount();
    const far = { ...shift, start_date: '2201-01-01', due_date: '2201-01-03' };
    vi.spyOn(work, 'previewLink').mockResolvedValue({
      shifts: [far],
      undated: [],
      blocked: {
        reason: 'date_out_of_range',
        shift_count: 1,
        max_shifts: 500,
        task_id: 'b',
        reference: 'KED-b',
        date: '2201-01-01',
      },
    });
    const set = vi
      .spyOn(work, 'setPredecessor')
      .mockResolvedValue({ kind: 'updated', task: { ...build, checklist: [] } });
    await editLagTo('3');
    const panel = (
      await screen.findByRole('heading', { name: '후속 업무 1개가 관계를 지키지 못합니다' })
    ).closest('section');
    if (panel === null) throw new Error('panel');
    // 제안 날짜를 자르지 않고 그대로 보이며, 왜 함께 저장할 수 없는지 승인 전에 알린다.
    expect(within(panel).getByText(/→ 2201-01-01~2201-01-03/)).toBeTruthy();
    expect(within(panel).getByTestId('shift-blocked').textContent).toBe(
      'KED-b 구현의 제안 날짜 2201-01-01이(가) 저장할 수 있는 범위(1900~2200년) 밖이라 함께 저장할 수 없습니다. 후속 업무를 옮기지 않고 저장하거나 취소하세요.',
    );
    const all = within(panel).getByRole('button', { name: '관계 저장하고 후속 업무 옮기기' });
    expect(all).toHaveProperty('disabled', true);
    await fireEvent.click(all);
    expect(set).not.toHaveBeenCalled();
    await fireEvent.click(within(panel).getByRole('button', { name: '관계만 저장' }));
    await waitFor(() => {
      expect(set).toHaveBeenCalledWith(
        expect.any(String),
        'b',
        'a',
        'SS',
        3,
        { kind: 'SS', lag_days: 1 },
        [],
      );
    });
  });

  it('rejects a lag that is not a whole number from 0 to 3650 before asking the server', async () => {
    await setLocale('ko', { reload: false });
    mount();
    const preview = vi.spyOn(work, 'previewLink');
    for (const bad of ['-1', '1.5', '3651', '']) {
      await editLagTo(bad);
      expect(await screen.findByText('지연일은 0부터 3650까지의 정수로 입력합니다.')).toBeTruthy();
    }
    expect(preview).not.toHaveBeenCalled();
  });
});

// 고치는 중인 관계를 지우면(#41 리뷰): 편집 칸이 지운 관계를 계속 가리키면 "관계 저장"이 그 관계를 다시 만든다.
describe('removing a dependency while editing', () => {
  const prep = task('x', '준비', [], ['2026-10-15', '2026-10-16']);
  const two = task('b', '구현', [
    { task_id: 'a', kind: 'SS', lag_days: 1 },
    { task_id: 'x', kind: 'FS', lag_days: 0 },
  ]);
  const without = (id: string) => ({
    ...two,
    predecessors: two.predecessors.filter((l) => l.task_id !== id),
    checklist: [],
  });

  it('closes the editor of the removed link, returns focus and does not recreate it on save', async () => {
    await setLocale('ko', { reload: false });
    const done = vi.spyOn(feedback, 'notifyDone');
    mount(two, [design, prep]);
    const remove = vi.spyOn(work, 'removePredecessor').mockResolvedValue(without('a'));
    const preview = vi
      .spyOn(work, 'previewLink')
      .mockResolvedValue({ shifts: [], undated: [], blocked: null });
    const set = vi.spyOn(work, 'setPredecessor');
    await fireEvent.click(await screen.findByRole('button', { name: 'KED-a 설계 관계 고치기' }));
    expect(screen.getByText('고치는 관계: 선행 업무 KED-a 설계 → 이 업무')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'KED-a 설계 선행 관계 지우기' }));
    await waitFor(() => {
      expect(remove).toHaveBeenCalledWith(expect.any(String), 'b', 'a');
    });
    // 편집 칸은 새 관계 추가로 돌아가고(고르지 않으면 저장할 수 없음), 지운 관계를 가리키는 글이 없다.
    await waitFor(() => {
      expect(screen.queryByText(/고치는 관계: 선행 업무 KED-a/)).toBeNull();
    });
    expect(screen.getByText('선행 업무 추가')).toBeTruthy();
    expect(screen.getByRole('button', { name: '관계 저장' }).hasAttribute('disabled')).toBe(true);
    await fireEvent.click(screen.getByRole('button', { name: '관계 저장' }));
    expect(preview).not.toHaveBeenCalled();
    expect(set).not.toHaveBeenCalled();
    expect(done).toHaveBeenCalledOnce();
    expect(done).toHaveBeenCalledWith('관계를 지웠습니다.', 'task-link');
    expect(screen.queryByText('관계를 지웠습니다.')).toBeNull();
    await waitFor(() => {
      expect(document.activeElement?.id).toBe('predecessors-heading');
    });
  });

  it('keeps the editor of another link when a different link is removed', async () => {
    await setLocale('ko', { reload: false });
    const done = vi.spyOn(feedback, 'notifyDone');
    mount(two, [design, prep]);
    vi.spyOn(work, 'removePredecessor').mockResolvedValue(without('x'));
    await fireEvent.click(await screen.findByRole('button', { name: 'KED-a 설계 관계 고치기' }));
    await fireEvent.input(screen.getByLabelText('지연(일)'), { target: { value: '4' } });
    await fireEvent.click(screen.getByRole('button', { name: 'KED-x 준비 선행 관계 지우기' }));
    await waitFor(() => {
      expect(done).toHaveBeenCalledWith('관계를 지웠습니다.', 'task-link');
    });
    expect(screen.getByText('고치는 관계: 선행 업무 KED-a 설계 → 이 업무')).toBeTruthy();
    expect(screen.getByLabelText<HTMLInputElement>('지연(일)').value).toBe('4');
  });

  it('keeps the link and the unsaved edit when removing fails', async () => {
    await setLocale('ko', { reload: false });
    mount(two, [design, prep]);
    vi.spyOn(work, 'removePredecessor').mockRejectedValue(new NetworkError('offline'));
    await fireEvent.click(await screen.findByRole('button', { name: 'KED-a 설계 관계 고치기' }));
    await fireEvent.input(screen.getByLabelText('지연(일)'), { target: { value: '5' } });
    await fireEvent.click(screen.getByRole('button', { name: 'KED-a 설계 선행 관계 지우기' }));
    expect(await screen.findByText(/선행 업무를 바꾸지 못했습니다/)).toBeTruthy();
    expect(screen.getByText('고치는 관계: 선행 업무 KED-a 설계 → 이 업무')).toBeTruthy();
    expect(screen.getByLabelText<HTMLInputElement>('지연(일)').value).toBe('5');
    expect(screen.queryByText('관계를 지웠습니다.')).toBeNull();
    expect(document.querySelector('[data-predecessor="a"]')).not.toBeNull();
  });

  it('does not let a save start while the removal is still waiting for the server', async () => {
    await setLocale('ko', { reload: false });
    mount(two, [design, prep]);
    let finish: (v: ReturnType<typeof without>) => void = () => undefined;
    vi.spyOn(work, 'removePredecessor').mockReturnValue(new Promise((r) => (finish = r)));
    const preview = vi
      .spyOn(work, 'previewLink')
      .mockResolvedValue({ shifts: [], undated: [], blocked: null });
    const set = vi.spyOn(work, 'setPredecessor');
    await fireEvent.click(await screen.findByRole('button', { name: 'KED-a 설계 관계 고치기' }));
    await fireEvent.click(screen.getByRole('button', { name: 'KED-a 설계 선행 관계 지우기' }));
    // 지우기 응답 전에 저장을 눌러도 아무 요청도 가지 않는다.
    await fireEvent.click(screen.getByRole('button', { name: '관계 저장' }));
    expect(preview).not.toHaveBeenCalled();
    finish(without('a'));
    await waitFor(() => {
      expect(screen.queryByText(/고치는 관계: 선행 업무 KED-a/)).toBeNull();
    });
    expect(preview).not.toHaveBeenCalled();
    expect(set).not.toHaveBeenCalled();
  });
});

// D-82: 일반 완료는 토스트다. 상세의 상위 업무 바꾸기와 저장(날짜 없는 업무 안내 포함)은 화면 안 완료 문구를 두지 않는다.
describe('task detail completions are toasts', () => {
  it('reports a changed parent once as a toast', async () => {
    await setLocale('ko', { reload: false });
    // 선택 상자(bits-ui)는 포인터 잡기를 쓴다. jsdom에는 없다.
    Element.prototype.hasPointerCapture = (): boolean => false;
    Element.prototype.setPointerCapture = (): void => undefined;
    Element.prototype.releasePointerCapture = (): void => undefined;
    Element.prototype.scrollIntoView = (): void => undefined;
    const done = vi.spyOn(feedback, 'notifyDone');
    const child = { ...review, parent_id: design.id };
    mount(child, [design, build]);
    const set = vi
      .spyOn(work, 'setTaskParent')
      .mockResolvedValue({ ...child, parent_id: null, version: 2, checklist: [] });
    const trigger = await waitFor(() => {
      const el = document.querySelector<HTMLElement>('[aria-labelledby="parent-choose-label"]');
      if (el === null) throw new Error('parent trigger');
      return el;
    });
    await fireEvent.pointerDown(trigger, { button: 0, pointerType: 'mouse' });
    await fireEvent.click(trigger);
    const option = await screen.findByRole('option', { name: '없음(최상위 업무)' });
    await fireEvent.pointerUp(option, { button: 0, pointerType: 'mouse' });
    await fireEvent.click(option);
    await fireEvent.click(screen.getByRole('button', { name: '바꾸기' }));

    await waitFor(() => {
      expect(done).toHaveBeenCalledOnce();
    });
    expect(set).toHaveBeenCalledWith(expect.any(String), child.id, null);
    expect(done).toHaveBeenCalledWith('상위 업무를 바꿨습니다.');
    expect(screen.queryByText('상위 업무를 바꿨습니다.')).toBeNull();
  });

  it('puts the undated followers note into the save toast', async () => {
    await setLocale('ko', { reload: false });
    const done = vi.spyOn(feedback, 'notifyDone');
    mount(design, [build, review]);
    vi.spyOn(work, 'previewSchedule').mockResolvedValue({
      shifts: [],
      undated: [{ task_id: 'c', reference: 'KED-c', title: '검토' }],
      blocked: null,
    });
    vi.spyOn(work, 'updateTask').mockResolvedValue({
      kind: 'updated',
      task: { ...design, due_date: '2026-10-22', version: 2, checklist: [] },
    });
    await fireEvent.input(await screen.findByLabelText('마감일'), {
      target: { value: '2026-10-22' },
    });
    await fireEvent.click(screen.getByRole('button', { name: '저장' }));

    await waitFor(() => {
      expect(done).toHaveBeenCalledOnce();
    });
    expect(done).toHaveBeenCalledWith(
      expect.stringMatching(/^저장했습니다\. 날짜가 없어 옮기지 않는 업무: KED-c 검토/),
      'task-save',
    );
    expect(screen.queryByText(/날짜가 없어 옮기지 않는 업무/)).toBeNull();
    expect(screen.queryByText('저장했습니다.')).toBeNull();
  });
});
