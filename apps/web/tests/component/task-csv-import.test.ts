import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { TaskDetailView, TaskInput } from '$lib/api/client';
import { api, NetworkError } from '$lib/api/client';
import { canonicalJson } from '$lib/features/documents/drafts';
import type { TaskRow } from '$lib/features/tasks/csv';
import type { RowProgress } from '$lib/features/tasks/csv-import';
import { matchRows } from '$lib/features/tasks/csv-import';
import TaskCsvImport from '$lib/features/tasks/TaskCsvImport.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

function detail(id: string, title: string, version: number): TaskDetailView {
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
    predecessor_ids: [],
    predecessors: [],
    series: null,
    occurrence_date: null,
    assignees: [],
    checklist_done: 0,
    checklist_total: 0,
    version,
    move_version: 1,
    completed_at: null,
    access: 'write',
    updated_at: '2026-10-21T00:00:00Z',
    checklist: [],
  };
}

// 서버 흉내: 같은 request_id는 같은 업무를 지금 판으로 돌려주고, 새 id는 새 업무를 만든다. 고치기는 기준 판이 지금
// 판이면 판을 하나 올리고, 아니면 409(conflict)다. `failOnce` 제목의 첫 고치기는 `failure`로 실패한다.
function fakeServer(failOnce: string, failure: Error) {
  const byRequest = new Map<string, string>();
  const versions = new Map<string, number>();
  let failed = false;
  // 새로 만든 업무 id(만든 순서).
  const made: string[] = [];
  const create = vi
    .spyOn(api, 'createTask')
    .mockImplementation((_item: string, requestId: string, title: string) => {
      const known = byRequest.get(requestId);
      const id = known ?? `t${String(byRequest.size + 1)}`;
      byRequest.set(requestId, id);
      if (known === undefined) {
        versions.set(id, 1);
        made.push(id);
      }
      return Promise.resolve(detail(id, title, versions.get(id) ?? 1));
    });
  const update = vi
    .spyOn(api, 'updateTask')
    .mockImplementation((id: string, base: number, input: TaskInput) => {
      if (input.title === failOnce && !failed) {
        failed = true;
        return Promise.reject(failure);
      }
      const current = versions.get(id) ?? 1;
      if (base !== current) {
        return Promise.resolve({ kind: 'conflict' as const, current: detail(id, '', current) });
      }
      const next = base + 1;
      versions.set(id, next);
      return Promise.resolve({ kind: 'updated' as const, task: detail(id, input.title, next) });
    });
  // 다른 사람이 그 업무를 고친 것처럼 판을 올린다.
  const bump = (id: string): void => {
    versions.set(id, (versions.get(id) ?? 1) + 5);
  };
  return { create, update, bump, made };
}

async function setText(value: string): Promise<void> {
  await fireEvent.input(screen.getByLabelText(/한 줄에 한 업무/), { target: { value } });
}

async function pressImport(onimported: () => Promise<void>, times: number): Promise<void> {
  await fireEvent.click(screen.getByRole('button', { name: '등록' }));
  await waitFor(() => {
    expect(onimported).toHaveBeenCalledTimes(times);
  });
}

describe('TaskCsvImport resume (audit W1, W2)', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
    vi.spyOn(api, 'assignable').mockResolvedValue([]);
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  // W1: 가운데 줄에서 멈춘 뒤 글자를 고치지 않고 다시 누르면 남은 줄만 넣는다. 이미 넣은 줄을 다시 만들기·고치기로
  // 보내면(만들기가 지금 판을 돌려주므로) 그사이 다른 사람이 바꾼 상태·담당자를 CSV 값으로 조용히 되돌린다.
  it('continues with the remaining rows and never re-sends a finished row', async () => {
    const server = fakeServer('B', new NetworkError('offline'));
    const onimported = vi.fn(() => Promise.resolve());
    render(TaskCsvImport, { itemId: 'i1', onimported });
    await setText('A,,진행 중\nB\nC');
    await pressImport(onimported, 1);
    expect(screen.getByText(/1개 등록됨/)).toBeInTheDocument();
    const taskA = server.update.mock.calls.find((c) => c[2].title === 'A')?.[0] ?? '';
    expect(taskA).not.toBe('');
    // 팀원이 A를 고쳤다(판이 오름).
    server.bump(taskA);
    server.create.mockClear();
    server.update.mockClear();
    await pressImport(onimported, 2);
    expect(server.create.mock.calls.map((c) => c[2])).not.toContain('A');
    expect(server.update.mock.calls.map((c) => c[0])).not.toContain(taskA);
    expect(server.update.mock.calls.map((c) => c[2].title)).toEqual(['B', 'C']);
    expect(screen.getByText('업무 3개를 등록했습니다.')).toBeInTheDocument();
  });

  // W2: 실패한 줄만 고쳐 다시 누르면 고치지 않은 줄은 같은 요청 id를 쓰고(새로 만들지 않음), 고친 줄은 제목만 만들어진
  // 그 업무에 채운다(같은 제목의 업무를 하나 더 만들지 않음).
  it('keeps unchanged rows and fills the title-only task of the edited row', async () => {
    const server = fakeServer('B', new NetworkError('offline'));
    const onimported = vi.fn(() => Promise.resolve());
    render(TaskCsvImport, { itemId: 'i1', onimported });
    await setText('A\nB,,,,2026-10-01\nC');
    await pressImport(onimported, 1);
    const firstIds = new Map(server.create.mock.calls.map((c) => [c[2], c[1]]));
    await setText('A\nB,,,,2026-10-02\nC');
    await pressImport(onimported, 2);
    for (const [, requestId, title] of server.create.mock.calls) {
      expect(requestId).toBe(firstIds.get(title) ?? requestId);
    }
    // 새 업무는 A·B·C 하나씩만 생긴다.
    expect(server.made).toEqual(['t1', 't2', 't3']);
    const lastB = server.update.mock.calls.filter((c) => c[2].title === 'B').at(-1);
    expect(lastB?.[0]).toBe('t2');
    expect(lastB?.[2].start_date).toBe('2026-10-02');
  });

  // 이미 넣은 줄을 고쳐 다시 누르면 그 업무를 이 등록이 받은 판으로 고친다. 그사이 다른 사람이 바꿨으면 덮어쓰지 않고
  // 알린다. 알린 뒤 다시 누를 때만 이 줄 내용으로 덮어쓴다.
  it('re-sends an edited finished row on the version it saw and reports a newer change', async () => {
    const server = fakeServer('B', new NetworkError('offline'));
    const onimported = vi.fn(() => Promise.resolve());
    render(TaskCsvImport, { itemId: 'i1', onimported });
    await setText('A\nB');
    await pressImport(onimported, 1);
    server.bump('t1');
    server.update.mockClear();
    await setText('A,,완료\nB');
    await pressImport(onimported, 2);
    expect(server.update.mock.calls.map((c) => [c[0], c[1]])).toEqual([['t1', 2]]);
    expect(
      screen.getByText(
        /A: 그사이 다른 곳에서 바뀌었습니다\. 다시 누르면 이 줄 내용으로 덮어씁니다\./,
      ),
    ).toBeInTheDocument();
    expect(server.create).toHaveBeenCalledTimes(2);
    server.update.mockClear();
    await pressImport(onimported, 3);
    expect(server.update.mock.calls.map((c) => [c[0], c[1], c[2].status])).toEqual([
      ['t1', 7, 'done'],
      ['t2', 1, 'todo'],
    ]);
    expect(server.create).toHaveBeenCalledTimes(2);
  });
});

describe('matchRows', () => {
  const row = (title: string, description = ''): TaskRow => ({
    title,
    description,
    status: 'todo',
    priority: 'none',
    start_date: null,
    due_date: null,
    estimate_minutes: null,
    assignee_ids: [],
    tags: [],
  });

  it('gives identical rows their own progress and a fixed row the title-only task first', () => {
    const doneA: RowProgress = {
      requestId: 'r1',
      title: 'A',
      task: { id: 't1', version: 2 },
      applied: canonicalJson(row('A', 'x')),
    };
    const titleOnlyA: RowProgress = {
      requestId: 'r2',
      title: 'A',
      task: { id: 't2', version: 1 },
      applied: undefined,
    };
    // 다 넣은 줄을 지우고 실패한 같은 제목 줄을 고쳤다: 고친 줄은 제목만 있는 업무에 채운다.
    expect(matchRows([row('A', 'y')], [doneA, titleOnlyA]).map((s) => s.progress)).toEqual([
      titleOnlyA,
    ]);
    // 줄 순서를 바꿔도 다 넣은 줄은 제 상태를 찾는다.
    const steps = matchRows([row('A', 'y'), row('A', 'x')], [doneA, titleOnlyA]);
    expect(steps.map((s) => s.progress)).toEqual([titleOnlyA, doneA]);
    // 새 줄은 새 요청 id.
    const fresh = matchRows([row('B')], [doneA]);
    expect(fresh[0]?.progress.requestId).not.toBe('r1');
    expect(fresh[0]?.progress.task).toBeUndefined();
  });
});
