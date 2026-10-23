import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { TaskDetailView, TaskInput } from '$lib/api/client';
import { api, ApiError, NetworkError } from '$lib/api/client';
import { canonicalJson } from '$lib/features/documents/drafts';
import type { TaskRow } from '$lib/features/tasks/csv';
import type { RowProgress } from '$lib/features/tasks/csv-import';
import { matchRows, rowApplied } from '$lib/features/tasks/csv-import';
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

type Fields = Omit<TaskInput, 'title'>;

// 서버 흉내(D-105): 만들기 하나가 모든 칸을 한 번에 저장한다. 같은 request_id는 처음 만든 업무를 지금 상태로 돌려주고(보낸
// 칸이 달라도), 새 id는 새 업무를 만든다. 거절(`reject`)된 만들기는 아무것도 남기지 않는다. 저장은 서버처럼 같은 태그를
// 합치고 담당자를 정렬한다.
function fakeServer() {
  const byRequest = new Map<string, string>();
  const tasks = new Map<string, TaskDetailView>();
  // 지운(휴지통) 업무. 만들기는 같은 request_id에 그 업무 id를 찾지만(app_create_task) 지운 업무는 읽을 수 없어 404다.
  const deleted = new Set<string>();
  // 이 제목의 다음 만들기를 이 오류로 거절한다(아무것도 만들지 않음).
  const rejects = new Map<string, Error>();
  // 이 제목의 다음 만들기는 서버에서 만들고 응답만 잃는다.
  const lost = new Set<string>();
  // 다음 만들기는 이 약속이 풀릴 때까지 응답하지 않는다(등록하는 중).
  let gate: Promise<void> = Promise.resolve();
  // 새로 만든 업무 id(만든 순서).
  const made: string[] = [];
  const create = vi
    .spyOn(api, 'createTask')
    .mockImplementation(
      async (
        _item: string,
        requestId: string,
        title: string,
        _parent?: string | null,
        fields?: Fields,
      ) => {
        const held = gate;
        gate = Promise.resolve();
        await held;
        const known = byRequest.get(requestId);
        if (known !== undefined) {
          if (deleted.has(known)) throw new ApiError(404, 'not_found', 'not found');
          return tasks.get(known) ?? detail(known, title, 1);
        }
        const rejected = rejects.get(title);
        if (rejected !== undefined) {
          rejects.delete(title);
          throw rejected;
        }
        const id = `t${String(made.length + 1)}`;
        byRequest.set(requestId, id);
        made.push(id);
        const task: TaskDetailView = {
          ...detail(id, title, 1),
          ...(fields === undefined
            ? {}
            : {
                description: fields.description,
                status: fields.status,
                priority: fields.priority,
                start_date: fields.start_date,
                due_date: fields.due_date,
                estimate_minutes: fields.estimate_minutes,
                tags: [...new Set(fields.tags)],
                assignees: [...new Set(fields.assignee_ids)]
                  .sort()
                  .map((user_id) => ({ user_id, display_name: user_id })),
                milestone: fields.milestone,
              }),
        };
        tasks.set(id, task);
        if (lost.delete(title)) throw new NetworkError('offline');
        return task;
      },
    );
  // CSV 등록은 고치기를 보내지 않는다(만들기 뒤 따로 고치는 두 단계가 없다).
  const update = vi.spyOn(api, 'updateTask');
  // 다른 사람이 그 업무를 고친 것처럼 설명을 바꾸고 판을 올린다.
  const bump = (id: string): void => {
    const task = tasks.get(id);
    if (task !== undefined) {
      tasks.set(id, { ...task, description: '팀원이 고침', version: task.version + 5 });
    }
  };
  const remove = (id: string): void => {
    deleted.add(id);
  };
  const reject = (title: string, error: Error): void => {
    rejects.set(title, error);
  };
  const lose = (title: string): void => {
    lost.add(title);
  };
  // 다음 만들기를 붙잡아 둔다. 돌려준 함수를 부르면 응답한다.
  const holdNextCreate = (): (() => void) => {
    let release: () => void = () => undefined;
    gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    return release;
  };
  const titles = (): string[] => create.mock.calls.map((c) => c[2]);
  return { create, update, bump, remove, reject, lose, holdNextCreate, made, tasks, titles };
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

const tooManyTags = (): ApiError => new ApiError(400, 'bad_request', 'at most 20 tags');

describe('TaskCsvImport one request per row (D-105)', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
    vi.spyOn(api, 'assignable').mockResolvedValue([
      { user_id: 'u1', display_name: '가람' },
      { user_id: 'u2', display_name: '나래' },
    ]);
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  // 한 줄은 모든 칸을 실은 만들기 하나다. 고치기를 보내지 않으므로 제목만 있는 업무가 남을 두 번째 단계가 없다.
  it('sends every field of a row in its one create request', async () => {
    const server = fakeServer();
    const onimported = vi.fn(() => Promise.resolve());
    render(TaskCsvImport, { itemId: 'i1', onimported });
    await setText('A,설명,진행 중,높음,2026-11-02,2026-11-09,30,나래;가람,회의;보고\nB');
    await waitFor(() => {
      expect(screen.getByRole('button', { name: '등록' })).toBeEnabled();
    });
    await pressImport(onimported, 1);
    expect(server.create).toHaveBeenCalledTimes(2);
    expect(server.create.mock.calls[0]?.[4]).toEqual({
      description: '설명',
      status: 'in_progress',
      priority: 'high',
      start_date: '2026-11-02',
      due_date: '2026-11-09',
      estimate_minutes: 30,
      tags: ['회의', '보고'],
      assignee_ids: ['u2', 'u1'],
      milestone: false,
    });
    expect(server.update).not.toHaveBeenCalled();
    expect(server.made).toEqual(['t1', 't2']);
    expect(screen.getByText('업무 2개를 등록했습니다.')).toBeInTheDocument();
  });

  // 가운데 줄이 거절되면 거기서 멈추고 앞 줄의 결과는 남는다(제목만 있는 업무는 없다). 다시 누르면 남은 줄만 보내고, 넣은
  // 줄은 그사이 다른 사람이 고쳤어도 다시 보내지 않는다.
  it('stops at a rejected row, keeps earlier rows and resumes with the rest', async () => {
    const server = fakeServer();
    server.reject('B', tooManyTags());
    const onimported = vi.fn(() => Promise.resolve());
    render(TaskCsvImport, { itemId: 'i1', onimported });
    await setText('A,,진행 중\nB\nC');
    await pressImport(onimported, 1);
    const message = screen.getByText(/1개 등록됨/);
    expect(message).toHaveTextContent(
      'B: 등록하지 못했습니다. 입력한 값을 확인해 주세요. (1개 등록됨, 다시 누르면 나머지를 이어서 넣습니다)',
    );
    expect(message).not.toHaveTextContent('제목만');
    expect(server.made).toEqual(['t1']);
    server.bump('t1');
    server.create.mockClear();
    await pressImport(onimported, 2);
    expect(server.titles()).toEqual(['B', 'C']);
    expect(server.made).toEqual(['t1', 't2', 't3']);
    expect(server.tasks.get('t1')?.description).toBe('팀원이 고침');
    expect(server.update).not.toHaveBeenCalled();
    expect(screen.getByText('업무 3개를 등록했습니다.')).toBeInTheDocument();
  });

  // 응답을 잃은 만들기(서버에는 반영됨)를 다시 누르면 같은 요청 id로 보내 그 업무를 돌려받는다. 둘이 되지 않는다.
  it('retries a create whose response was lost with the same request id', async () => {
    const server = fakeServer();
    server.lose('B');
    const onimported = vi.fn(() => Promise.resolve());
    render(TaskCsvImport, { itemId: 'i1', onimported });
    await setText('A\nB,설명\nC');
    await pressImport(onimported, 1);
    expect(screen.getByText(/1개 등록됨/)).toBeInTheDocument();
    expect(server.made).toEqual(['t1', 't2']);
    const firstB = server.create.mock.calls.find((c) => c[2] === 'B')?.[1];
    server.create.mockClear();
    await pressImport(onimported, 2);
    expect(server.titles()).toEqual(['B', 'C']);
    expect(server.create.mock.calls[0]?.[1]).toBe(firstB);
    expect(server.made).toEqual(['t1', 't2', 't3']);
    expect(screen.getByText('업무 3개를 등록했습니다.')).toBeInTheDocument();
  });

  // 거절된 줄만 고쳐 다시 누르면 고치지 않은 줄은 건너뛰고, 고친 줄은 같은 요청 id로 고친 내용을 만든다(거절된 만들기는
  // 아무것도 남기지 않았다).
  it('creates a fixed rejected row once with its fixed contents', async () => {
    const server = fakeServer();
    server.reject('B', tooManyTags());
    const onimported = vi.fn(() => Promise.resolve());
    render(TaskCsvImport, { itemId: 'i1', onimported });
    await setText('A\nB,,,,2026-10-01\nC');
    await pressImport(onimported, 1);
    const firstB = server.create.mock.calls.find((c) => c[2] === 'B')?.[1];
    await setText('A\nB,,,,2026-10-02\nC');
    await pressImport(onimported, 2);
    const lastB = server.create.mock.calls.filter((c) => c[2] === 'B').at(-1);
    expect(lastB?.[1]).toBe(firstB);
    expect(server.made).toEqual(['t1', 't2', 't3']);
    expect(server.tasks.get('t2')?.start_date).toBe('2026-10-02');
    expect(screen.getByText('업무 3개를 등록했습니다.')).toBeInTheDocument();
  });

  // 응답을 잃은 줄을 고쳐 다시 누르면 같은 요청 id라 서버는 처음(고치기 전) 업무를 돌려준다. 같은 업무를 또 만들지 않고,
  // 고친 내용이 들어가지 않았다고 알린다. 다시 누르면 다음 줄부터 넣는다.
  it('reports a lost row edited before the retry instead of creating it twice', async () => {
    const server = fakeServer();
    server.lose('B');
    const onimported = vi.fn(() => Promise.resolve());
    render(TaskCsvImport, { itemId: 'i1', onimported });
    await setText('A\nB,첫 내용\nC');
    await pressImport(onimported, 1);
    await setText('A\nB,고친 내용\nC');
    await pressImport(onimported, 2);
    expect(screen.getByText(/1개 등록됨/)).toHaveTextContent(
      'B: 앞서 보낸 요청으로 이미 만들어진 업무가 이 줄 내용과 다릅니다.',
    );
    expect(server.made).toEqual(['t1', 't2']);
    expect(server.tasks.get('t2')?.description).toBe('첫 내용');
    server.create.mockClear();
    await pressImport(onimported, 3);
    expect(server.titles()).toEqual(['C']);
    expect(server.made).toEqual(['t1', 't2', 't3']);
    expect(screen.getByText('업무 3개를 등록했습니다.')).toBeInTheDocument();
  });

  // 응답을 잃은 줄의 업무(보드에는 보인다)를 지우면 같은 요청 id의 만들기가 404다. 다음 누름은 새 id로 한 번만 만든다.
  it('creates the row anew when the task of a lost create was deleted', async () => {
    const server = fakeServer();
    server.lose('A');
    const onimported = vi.fn(() => Promise.resolve());
    render(TaskCsvImport, { itemId: 'i1', onimported });
    await setText('A');
    await pressImport(onimported, 1);
    server.remove('t1');
    await pressImport(onimported, 2);
    expect(screen.getByText(/0개 등록됨/)).toHaveTextContent(
      '"A" 업무가 지워졌거나 볼 수 없습니다. 다시 누르면 새로 만듭니다.',
    );
    await pressImport(onimported, 3);
    const ids = server.create.mock.calls.map((c) => c[1]);
    expect(ids[1]).toBe(ids[0]);
    expect(ids[2]).not.toBe(ids[0]);
    expect(server.made).toEqual(['t1', 't2']);
    expect(screen.getByText('업무 1개를 등록했습니다.')).toBeInTheDocument();
  });
});

// 지난 묶음의 진행 상태가 다음 묶음에 붙으면 같은 제목의 줄이 지난 요청 id를 쓰거나(지난 업무가 돌아옴) 같은 줄이
// 건너뛰어진다.
describe('TaskCsvImport new batch after a failure (review)', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
    vi.spyOn(api, 'assignable').mockResolvedValue([]);
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  const resumeHint = /이미 등록한 1줄은 건너뜁니다/;

  it('drops the progress when the text is cleared and starts a new batch', async () => {
    const server = fakeServer();
    server.reject('B', tooManyTags());
    const onimported = vi.fn(() => Promise.resolve());
    render(TaskCsvImport, { itemId: 'i1', onimported });
    await setText('주간 회의,첫 주,,,2026-11-02\nB');
    await pressImport(onimported, 1);
    expect(screen.getByText(resumeHint)).toHaveTextContent(
      '이미 등록한 1줄은 건너뜁니다. 처음부터 새로 넣으려면 글자를 모두 지우세요.',
    );
    await setText('');
    expect(screen.queryByText(resumeHint)).not.toBeInTheDocument();
    await setText('주간 회의,둘째 주,,,2026-11-09\n주간 보고');
    await pressImport(onimported, 2);
    expect(server.made).toEqual(['t1', 't2', 't3']);
    expect(server.tasks.get('t2')?.description).toBe('둘째 주');
    expect(screen.getByText('업무 2개를 등록했습니다.')).toBeInTheDocument();
  });

  it('creates an identical row again after the text is cleared', async () => {
    const server = fakeServer();
    server.reject('B', tooManyTags());
    const onimported = vi.fn(() => Promise.resolve());
    render(TaskCsvImport, { itemId: 'i1', onimported });
    await setText('청소\nB');
    await pressImport(onimported, 1);
    const firstIds = server.create.mock.calls.map((c) => c[1]);
    await setText('');
    await setText('청소');
    await pressImport(onimported, 2);
    expect(server.create).toHaveBeenCalledTimes(3);
    expect(firstIds).not.toContain(server.create.mock.calls.at(-1)?.[1]);
    expect(server.made).toEqual(['t1', 't2']);
    expect(screen.getByText('업무 1개를 등록했습니다.')).toBeInTheDocument();
  });

  // 등록하는 동안 글자를 지워도 그 등록은 끝까지 간다. 그때 진행 상태를 버리면 실패 뒤 같은 글자로 다시 누를 때(안내는
  // 이어서 넣는다고 한다) 넣은 줄과 응답을 잃은 줄을 새 업무로 또 만든다.
  it('keeps the progress when the text is cleared during an import', async () => {
    const server = fakeServer();
    server.lose('B');
    const release = server.holdNextCreate();
    const onimported = vi.fn(() => Promise.resolve());
    render(TaskCsvImport, { itemId: 'i1', onimported });
    await setText('A\nB\nC');
    await fireEvent.click(screen.getByRole('button', { name: '등록' }));
    await waitFor(() => {
      expect(server.create).toHaveBeenCalledTimes(1);
    });
    await setText('');
    release();
    await waitFor(() => {
      expect(onimported).toHaveBeenCalledTimes(1);
    });
    expect(screen.getByText(/1개 등록됨/)).toBeInTheDocument();
    await setText('A\nB\nC');
    await pressImport(onimported, 2);
    expect(server.made).toEqual(['t1', 't2', 't3']);
    expect(screen.getByText('업무 3개를 등록했습니다.')).toBeInTheDocument();
  });

  // 검토: 등록하는 동안 글자를 지운 뒤 그 등록이 성공하면 끝난 묶음의 진행 상태를 버린다(새 묶음). 남기면 다음에 같은
  // 줄을 넣어도 "이미 등록함"으로 건너뛰고 등록했다고만 알렸다.
  it('starts a new batch when the text was cleared during an import that succeeded', async () => {
    const server = fakeServer();
    const release = server.holdNextCreate();
    const onimported = vi.fn(() => Promise.resolve());
    render(TaskCsvImport, { itemId: 'i1', onimported });
    await setText('청소\n빨래');
    await fireEvent.click(screen.getByRole('button', { name: '등록' }));
    await waitFor(() => {
      expect(server.create).toHaveBeenCalledTimes(1);
    });
    await setText('');
    release();
    await waitFor(() => {
      expect(onimported).toHaveBeenCalledTimes(1);
    });
    expect(screen.getByText('업무 2개를 등록했습니다.')).toBeInTheDocument();
    await setText('청소');
    expect(screen.queryByText(resumeHint)).not.toBeInTheDocument();
    await pressImport(onimported, 2);
    expect(server.made).toEqual(['t1', 't2', 't3']);
    expect(screen.getByText('업무 1개를 등록했습니다.')).toBeInTheDocument();
  });

  // 지우지 않고 새 글자를 붙여 넣은 경우: 지난 줄과 겹치는 줄이 없으면 새 묶음이다.
  it('drops the progress when pasted text shares no rows with it', async () => {
    const server = fakeServer();
    server.reject('B', tooManyTags());
    const onimported = vi.fn(() => Promise.resolve());
    render(TaskCsvImport, { itemId: 'i1', onimported });
    await setText('주간 회의,첫 주,,,2026-11-02\nB');
    await pressImport(onimported, 1);
    await setText('주간 회의,둘째 주,,,2026-11-09\n주간 보고');
    expect(screen.queryByText(resumeHint)).not.toBeInTheDocument();
    await pressImport(onimported, 2);
    expect(server.made).toEqual(['t1', 't2', 't3']);
    expect(screen.getByText('업무 2개를 등록했습니다.')).toBeInTheDocument();
  });

  // 지우지 않고 붙여 넣은 글자에 다 넣은 줄과 같은 줄이 있으면 이어서 넣는다. 건너뛸 줄을 누르기 전에 보여 준다.
  it('shows which rows a resume skips before the press', async () => {
    const server = fakeServer();
    server.reject('B', tooManyTags());
    const onimported = vi.fn(() => Promise.resolve());
    render(TaskCsvImport, { itemId: 'i1', onimported });
    await setText('청소\nB');
    await pressImport(onimported, 1);
    await setText('청소\n주간 보고');
    expect(screen.getByText(resumeHint)).toBeInTheDocument();
    server.create.mockClear();
    await pressImport(onimported, 2);
    expect(server.titles()).toEqual(['주간 보고']);
    expect(server.made).toEqual(['t1', 't2']);
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

  it('gives identical rows their own progress and a fixed row the unfinished request first', () => {
    const doneA: RowProgress = {
      requestId: 'r1',
      title: 'A',
      applied: canonicalJson(row('A', 'x')),
    };
    const pendingA: RowProgress = { requestId: 'r2', title: 'A', applied: undefined };
    // 다 넣은 줄을 지우고 실패한 같은 제목 줄을 고쳤다: 고친 줄은 그 줄의 요청 id로 보낸다.
    expect(matchRows([row('A', 'y')], [doneA, pendingA]).map((s) => s.progress)).toEqual([
      pendingA,
    ]);
    // 줄 순서를 바꿔도 다 넣은 줄은 제 상태를 찾는다.
    const steps = matchRows([row('A', 'y'), row('A', 'x')], [doneA, pendingA]);
    expect(steps.map((s) => s.progress)).toEqual([pendingA, doneA]);
    // 새 줄은 새 요청 id.
    const fresh = matchRows([row('B')], [doneA]);
    expect(fresh[0]?.progress.requestId).not.toBe('r1');
    expect(fresh[0]?.progress.applied).toBeUndefined();
  });

  it('never hands a finished row to a row with other content', () => {
    const doneA: RowProgress = {
      requestId: 'r1',
      title: 'A',
      applied: canonicalJson(row('A', 'x')),
    };
    const [step] = matchRows([row('A', 'z')], [doneA]);
    expect(step?.progress).not.toBe(doneA);
    expect(step?.progress.requestId).not.toBe('r1');
    expect(step?.progress.applied).toBeUndefined();
  });
});

describe('rowApplied', () => {
  const row: TaskRow = {
    title: 'A',
    description: '',
    status: 'todo',
    priority: 'none',
    start_date: null,
    due_date: null,
    estimate_minutes: null,
    assignee_ids: ['u2', 'u1'],
    tags: ['회의', '보고', '회의'],
  };
  const stored: TaskDetailView = {
    ...detail('t1', 'A', 2),
    tags: ['회의', '보고'],
    assignees: [
      { user_id: 'u1', display_name: '가람' },
      { user_id: 'u2', display_name: '나래' },
    ],
  };

  it('reads the row as the server stores it and anything else as a change', () => {
    expect(rowApplied(row, stored)).toBe(true);
    expect(rowApplied(row, { ...stored, milestone: true })).toBe(false);
    expect(rowApplied(row, { ...stored, assignees: stored.assignees.slice(1) })).toBe(false);
    expect(rowApplied(row, { ...stored, tags: ['보고', '회의'] })).toBe(false);
  });
});
