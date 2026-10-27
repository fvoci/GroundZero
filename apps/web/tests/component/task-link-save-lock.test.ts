import './fixtures/task-statuses';

import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { TaskDetailView, TaskLinkView, TaskView } from '$lib/api/client';
import { api, ApiError } from '$lib/api/client';
import TaskDetail from '$lib/features/tasks/TaskDetail.svelte';
import { me } from '$lib/me.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

// R5 최종 A FA-1(D-97 "저장 중에는 입력을 잠근다", D-27 보존): 밀릴 업무가 없는 관계 저장이 가는 동안 관계 편집 칸
// (선행·종류·지연일)이 잠기지 않아, 그사이 친 값을 성공 뒤 resetLinkEditor()가 저장하지도 남기지도 않고 지웠다.
// 저장 중에는 칸 전체를 잠그고(Select는 자기 disabled), 요청이 끝나면 다시 쓸 수 있다.

beforeEach(async () => {
  me.userId = 'user-a';
  await setLocale('ko', { reload: false });
});

function task(id: string, title: string, links: TaskLinkView[] = []): TaskView {
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
    start_date: '2026-10-19',
    due_date: '2026-10-21',
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
const design = task('a', '설계');
const build = task('b', '구현', [{ task_id: 'a', kind: 'SS', lag_days: 1 }]);
let setPredecessor: ReturnType<typeof vi.spyOn>;
const saved: TaskDetailView = {
  ...build,
  version: 2,
  predecessors: [{ task_id: 'a', kind: 'SS', lag_days: 3 }],
  checklist: [],
};

function setUp(): { release: (ok: boolean) => void } {
  vi.spyOn(api, 'task').mockResolvedValue({ ...build, checklist: [] });
  vi.spyOn(api, 'assignable').mockResolvedValue([]);
  vi.spyOn(api, 'taskBacklinks').mockResolvedValue([]);
  vi.spyOn(api, 'comments').mockResolvedValue([]);
  vi.spyOn(api, 'previewLink').mockResolvedValue({ shifts: [], undated: [], blocked: null });
  let release: (ok: boolean) => void = () => undefined;
  setPredecessor = vi.spyOn(api, 'setPredecessor').mockImplementation(
    () =>
      new Promise((resolve, reject) => {
        release = (ok) => {
          if (ok) resolve({ kind: 'updated', task: saved });
          else reject(new ApiError(400, 'bad_request', 'nope'));
        };
      }),
  );
  render(TaskDetail, {
    props: {
      taskId: 'b',
      tasks: [design, build],
      workspaces: [],
      onchanged: () => undefined,
      onopen: () => undefined,
    },
  });
  return {
    release: (ok) => {
      release(ok);
    },
  };
}

async function startSave(): Promise<HTMLElement> {
  await fireEvent.click(await screen.findByRole('button', { name: 'KED-a 설계 관계 고치기' }));
  const lag = screen.getByLabelText('지연(일)');
  await fireEvent.input(lag, { target: { value: '3' } });
  await fireEvent.click(screen.getByRole('button', { name: '관계 저장' }));
  await waitFor(() => {
    expect(setPredecessor).toHaveBeenCalledOnce();
  });
  return lag;
}

describe('the link editor while a link save is in flight', () => {
  it('locks the lag and kind fields, then opens editable again after success', async () => {
    const { release } = setUp();
    const lag = await startSave();
    expect(lag).toBeDisabled();
    // Select는 조상 fieldset이 아닌 자기 disabled로 잠긴다.
    expect(screen.getByTestId('link-kind')).toHaveAttribute('data-disabled');

    release(true);
    await waitFor(() => {
      expect(screen.queryByLabelText('지연(일)')).toBeNull();
    });
    await fireEvent.click(await screen.findByRole('button', { name: 'KED-a 설계 관계 고치기' }));
    expect(screen.getByLabelText('지연(일)')).toBeEnabled();
    expect(screen.getByTestId('link-kind')).not.toHaveAttribute('data-disabled');
  });

  it('unlocks the fields with the typed values when the save fails', async () => {
    const { release } = setUp();
    const lag = await startSave();
    expect(lag).toBeDisabled();
    release(false);
    await waitFor(() => {
      expect(lag).toBeEnabled();
    });
    expect(lag).toHaveValue('3');
    await fireEvent.input(lag, { target: { value: '5' } });
    expect(lag).toHaveValue('5');
  });
});
