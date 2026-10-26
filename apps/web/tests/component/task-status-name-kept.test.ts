import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { TaskDetailView, TaskInput } from '$lib/api/client';
import { api, parseTask } from '$lib/api/client';
import { formOf } from '$lib/features/tasks/form';
import TaskDetail from '$lib/features/tasks/TaskDetail.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

// D-116: 업무 상태는 항목이 정의한 이름이고 완료 판단·기본 열은 범주다. 다른 칸만 고쳐 저장할 때 서버가 준 이름(`review`)을
// 그대로 돌려보내야 상태가 조용히 바뀌지 않는다(1단계 소비자의 같은 결함을 CodeRabbit #139 df376f7c가 지적; 이 단계는
// 상태 이름 문자열을 쓰므로 회귀 시험으로 둔다).
afterEach(() => {
  vi.restoreAllMocks();
});

const raw = {
  id: 't1',
  item_id: 'i1',
  module_id: 'm1',
  workspace_id: 'ws-1',
  parent_id: null,
  reference: 'KED-1',
  title: '검토 중인 업무',
  description: '',
  status: 'review',
  status_category: 'doing',
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
  updated_at: '2026-10-26T00:00:00Z',
};

function detail(): TaskDetailView {
  return { ...parseTask(raw), checklist: [] };
}

function mount(current: TaskDetailView): void {
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

function element(id: string): HTMLElement {
  const el = document.getElementById(id);
  if (el === null) throw new Error(id);
  return el;
}

describe('custom task status names survive a save of other fields', () => {
  it('parses an item-defined name as the status and keeps it in the form', () => {
    const task = detail();
    expect(task.status).toBe('review');
    expect(task.status_category).toBe('doing');
    expect(formOf(task).status).toBe('review');
  });

  it('sends the original name when only the title changed', async () => {
    await setLocale('ko', { reload: false });
    const current = detail();
    const sent: TaskInput[] = [];
    vi.spyOn(api, 'updateTask').mockImplementation((_user, _id, _base, input) => {
      sent.push(input);
      return Promise.resolve({
        kind: 'updated',
        task: { ...current, title: input.title, version: 2 },
      });
    });
    mount(current);
    const title = await screen.findByDisplayValue('검토 중인 업무');
    await fireEvent.input(title, { target: { value: '검토 끝낸 업무' } });
    await fireEvent.click(element('task-save'));
    await waitFor(() => {
      expect(sent).toHaveLength(1);
    });
    expect(sent[0]?.status).toBe('review');
    expect(sent[0]?.title).toBe('검토 끝낸 업무');
    expect(await screen.findByText('저장했습니다.')).toBeTruthy();
  });
});
