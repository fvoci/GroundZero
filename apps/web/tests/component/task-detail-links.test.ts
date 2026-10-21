import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { ShiftProposalView, TaskLinkView, TaskView } from '$lib/api/client';
import { api } from '$lib/api/client';
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
    predecessor_ids: links.map((l) => l.task_id),
    predecessors: links,
    series: null,
    occurrence_date: null,
    assignees: [],
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

function mount(): void {
  vi.spyOn(api, 'task').mockResolvedValue({ ...build, checklist: [] });
  vi.spyOn(api, 'assignable').mockResolvedValue([]);
  vi.spyOn(api, 'taskBacklinks').mockResolvedValue([]);
  vi.spyOn(api, 'comments').mockResolvedValue([]);
  render(TaskDetail, {
    props: {
      taskId: 'b',
      tasks: [design, build, review],
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
    mount();
    const preview = vi
      .spyOn(api, 'previewLink')
      .mockResolvedValue({ shifts: [shift], undated: [] });
    const set = vi.spyOn(api, 'setPredecessor').mockResolvedValue({
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
      expect(set).toHaveBeenCalledWith('b', 'a', 'SS', 3, [
        { task_id: 'b', base_version: 1, start_date: '2026-10-22', due_date: '2026-10-24' },
      ]);
    });
    expect((await screen.findByTestId('link-status')).textContent).toContain(
      '관계를 저장했습니다.',
    );
  });

  it('saves nothing when the preview is cancelled and returns focus to the save button', async () => {
    await setLocale('ko', { reload: false });
    mount();
    vi.spyOn(api, 'previewLink').mockResolvedValue({ shifts: [shift], undated: [] });
    const set = vi.spyOn(api, 'setPredecessor');
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
      .spyOn(api, 'previewLink')
      .mockResolvedValueOnce({ shifts: [shift], undated: [] })
      .mockResolvedValueOnce({ shifts: [moved], undated: [] });
    const set = vi
      .spyOn(api, 'setPredecessor')
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

  it('saves directly when nothing moves and says which undated followers were not checked', async () => {
    await setLocale('ko', { reload: false });
    mount();
    vi.spyOn(api, 'previewLink').mockResolvedValue({
      shifts: [],
      undated: [{ task_id: 'c', reference: 'KED-c', title: '검토' }],
    });
    const set = vi
      .spyOn(api, 'setPredecessor')
      .mockResolvedValue({ kind: 'updated', task: { ...build, checklist: [] } });
    await editLagTo('0');
    await waitFor(() => {
      expect(set).toHaveBeenCalledWith('b', 'a', 'SS', 0, []);
    });
    expect((await screen.findByTestId('link-status')).textContent).toContain(
      '날짜가 없어 옮기지 않는 업무: KED-c 검토',
    );
  });

  it('rejects a lag that is not a whole number from 0 to 3650 before asking the server', async () => {
    await setLocale('ko', { reload: false });
    mount();
    const preview = vi.spyOn(api, 'previewLink');
    for (const bad of ['-1', '1.5', '3651', '']) {
      await editLagTo(bad);
      expect(await screen.findByText('지연일은 0부터 3650까지의 정수로 입력합니다.')).toBeTruthy();
    }
    expect(preview).not.toHaveBeenCalled();
  });
});
