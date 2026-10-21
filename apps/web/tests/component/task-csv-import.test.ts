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
// 판이면 판을 하나 올리고, 아니면 409(conflict)와 지금 업무다. `failOnce` 제목의 첫 고치기는 `failure`로 실패한다.
function fakeServer(failOnce: string, failure: Error) {
  const byRequest = new Map<string, string>();
  // 업무 id별 지금 업무(판 포함). 고치기는 서버처럼 같은 태그를 합치고 담당자를 정렬해 저장한다.
  const tasks = new Map<string, TaskDetailView>();
  // 지운(휴지통) 업무. 만들기는 같은 request_id에 그 업무 id를 찾지만(app_create_task) 지운 업무는 읽을 수 없어 404,
  // 고치기도 404다.
  const deleted = new Set<string>();
  let failed = false;
  // 다음 만들기는 서버에서 만들고 응답만 잃는다.
  let loseNext = false;
  // 이 제목의 다음 고치기는 서버에 반영하고 응답만 잃는다.
  const loseUpdates = new Set<string>();
  // 다음 고치기는 이 약속이 풀릴 때까지 응답하지 않는다(등록하는 중).
  let gate: Promise<void> = Promise.resolve();
  // 새로 만든 업무 id(만든 순서).
  const made: string[] = [];
  const create = vi
    .spyOn(api, 'createTask')
    .mockImplementation((_item: string, requestId: string, title: string) => {
      const known = byRequest.get(requestId);
      const id = known ?? `t${String(byRequest.size + 1)}`;
      byRequest.set(requestId, id);
      if (known === undefined) {
        tasks.set(id, detail(id, title, 1));
        made.push(id);
      }
      if (deleted.has(id)) {
        return Promise.reject(new ApiError(404, 'not_found', 'not found'));
      }
      if (loseNext) {
        loseNext = false;
        return Promise.reject(new NetworkError('offline'));
      }
      return Promise.resolve(tasks.get(id) ?? detail(id, title, 1));
    });
  const update = vi
    .spyOn(api, 'updateTask')
    .mockImplementation(async (id: string, base: number, input: TaskInput) => {
      const held = gate;
      gate = Promise.resolve();
      await held;
      if (deleted.has(id)) {
        return Promise.reject(new ApiError(404, 'not_found', 'not found'));
      }
      if (input.title === failOnce && !failed) {
        failed = true;
        return Promise.reject(failure);
      }
      const current = tasks.get(id) ?? detail(id, input.title, 1);
      if (base !== current.version) {
        return Promise.resolve({ kind: 'conflict' as const, current });
      }
      const next: TaskDetailView = {
        ...current,
        title: input.title,
        description: input.description,
        status: input.status,
        priority: input.priority,
        start_date: input.start_date,
        due_date: input.due_date,
        estimate_minutes: input.estimate_minutes,
        tags: [...new Set(input.tags)],
        assignees: [...new Set(input.assignee_ids)]
          .sort()
          .map((user_id) => ({ user_id, display_name: user_id })),
        milestone: input.milestone,
        version: base + 1,
      };
      tasks.set(id, next);
      if (loseUpdates.delete(input.title)) {
        return Promise.reject(new NetworkError('offline'));
      }
      return Promise.resolve({ kind: 'updated' as const, task: next });
    });
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
  const loseNextCreate = (): void => {
    loseNext = true;
  };
  const loseUpdate = (title: string): void => {
    loseUpdates.add(title);
  };
  // 다음 고치기를 붙잡아 둔다. 돌려준 함수를 부르면 응답한다.
  const holdNextUpdate = (): (() => void) => {
    let release: () => void = () => undefined;
    gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    return release;
  };
  return { create, update, bump, remove, loseNextCreate, loseUpdate, holdNextUpdate, made };
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

  // 다 넣은 줄을 고친 줄은 그 업무를 다시 고치지 않는다(같은 제목이라도 다른 줄일 수 있다). 새 업무로 넣는다.
  it('never re-sends an edited finished row onto its task', async () => {
    const server = fakeServer('B', new ApiError(400, 'bad_request', 'at most 20 tags'));
    const onimported = vi.fn(() => Promise.resolve());
    render(TaskCsvImport, { itemId: 'i1', onimported });
    await setText('A\nB');
    await pressImport(onimported, 1);
    server.update.mockClear();
    await setText('A,설명\nB');
    await pressImport(onimported, 2);
    expect(server.update.mock.calls.map((c) => c[0])).toEqual(['t3', 't2']);
    expect(server.made).toEqual(['t1', 't2', 't3']);
  });
});

// 지난 묶음의 진행 상태가 다음 묶음에 붙으면 같은 제목의 줄이 지난 업무를 고치거나(덮어씀) 같은 줄이 건너뛰어진다.
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
    const server = fakeServer('B', new ApiError(400, 'bad_request', 'at most 20 tags'));
    const onimported = vi.fn(() => Promise.resolve());
    render(TaskCsvImport, { itemId: 'i1', onimported });
    await setText('주간 회의,첫 주,,,2026-11-02\nB');
    await pressImport(onimported, 1);
    expect(screen.getByText(resumeHint)).toHaveTextContent(
      '이미 등록한 1줄은 건너뜁니다. 제목만 만들어진 업무 1개는 이어서 채웁니다. 처음부터 새로 넣으려면 글자를 모두 지우세요.',
    );
    await setText('');
    expect(screen.queryByText(resumeHint)).not.toBeInTheDocument();
    await setText('주간 회의,둘째 주,,,2026-11-09\n주간 보고');
    server.update.mockClear();
    await pressImport(onimported, 2);
    expect(server.update.mock.calls.map((c) => c[0])).toEqual(['t3', 't4']);
    expect(server.made).toEqual(['t1', 't2', 't3', 't4']);
    expect(screen.getByText('업무 2개를 등록했습니다.')).toBeInTheDocument();
  });

  it('creates an identical row again after the text is cleared', async () => {
    const server = fakeServer('B', new ApiError(400, 'bad_request', 'at most 20 tags'));
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
    expect(server.made).toEqual(['t1', 't2', 't3']);
    expect(screen.getByText('업무 1개를 등록했습니다.')).toBeInTheDocument();
  });

  // 등록하는 동안 글자를 지워도 그 등록은 끝까지 간다. 그때 진행 상태를 버리면 실패 뒤 같은 글자로 다시 누를 때(안내는
  // 이어서 넣는다고 한다) 다 넣은 줄과 제목만 만든 줄을 새 업무로 또 만든다.
  it('keeps the progress when the text is cleared during an import', async () => {
    const server = fakeServer('B', new NetworkError('offline'));
    const release = server.holdNextUpdate();
    const onimported = vi.fn(() => Promise.resolve());
    render(TaskCsvImport, { itemId: 'i1', onimported });
    await setText('A\nB\nC');
    await fireEvent.click(screen.getByRole('button', { name: '등록' }));
    await waitFor(() => {
      expect(server.update).toHaveBeenCalledTimes(1);
    });
    await setText('');
    release();
    await waitFor(() => {
      expect(onimported).toHaveBeenCalledTimes(1);
    });
    expect(screen.getByText(/1개 등록됨/)).toHaveTextContent(
      '"B" 업무는 제목만 만들어졌습니다. 다시 누르면 그 업무에 나머지를 채웁니다.',
    );
    await setText('A\nB\nC');
    await pressImport(onimported, 2);
    expect(server.made).toEqual(['t1', 't2', 't3']);
    expect(server.update.mock.calls.at(-2)?.[0]).toBe('t2');
    expect(screen.getByText('업무 3개를 등록했습니다.')).toBeInTheDocument();
  });

  // 검토: 등록하는 동안 글자를 지운 뒤 그 등록이 성공하면 끝난 묶음의 진행 상태를 버린다(새 묶음). 남기면 다음에 같은
  // 줄을 넣어도 "이미 등록함"으로 건너뛰고 등록했다고만 알렸다.
  it('starts a new batch when the text was cleared during an import that succeeded', async () => {
    const server = fakeServer('', new NetworkError('offline'));
    const release = server.holdNextUpdate();
    const onimported = vi.fn(() => Promise.resolve());
    render(TaskCsvImport, { itemId: 'i1', onimported });
    await setText('청소\n빨래');
    await fireEvent.click(screen.getByRole('button', { name: '등록' }));
    await waitFor(() => {
      expect(server.update).toHaveBeenCalledTimes(1);
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
    const server = fakeServer('B', new ApiError(400, 'bad_request', 'at most 20 tags'));
    const onimported = vi.fn(() => Promise.resolve());
    render(TaskCsvImport, { itemId: 'i1', onimported });
    await setText('주간 회의,첫 주,,,2026-11-02\nB');
    await pressImport(onimported, 1);
    await setText('주간 회의,둘째 주,,,2026-11-09\n주간 보고');
    expect(screen.queryByText(resumeHint)).not.toBeInTheDocument();
    server.update.mockClear();
    await pressImport(onimported, 2);
    expect(server.update.mock.calls.map((c) => c[0])).toEqual(['t3', 't4']);
    expect(server.made).toEqual(['t1', 't2', 't3', 't4']);
    expect(screen.getByText('업무 2개를 등록했습니다.')).toBeInTheDocument();
  });

  // 지우지 않고 붙여 넣은 글자에 다 넣은 줄과 같은 줄이 있으면 이어서 넣는다. 건너뛸 줄을 누르기 전에 보여 준다.
  it('shows which rows a resume skips before the press', async () => {
    const server = fakeServer('B', new ApiError(400, 'bad_request', 'at most 20 tags'));
    const onimported = vi.fn(() => Promise.resolve());
    render(TaskCsvImport, { itemId: 'i1', onimported });
    await setText('청소\nB');
    await pressImport(onimported, 1);
    await setText('청소\n주간 보고');
    expect(screen.getByText(resumeHint)).toBeInTheDocument();
    server.update.mockClear();
    await pressImport(onimported, 2);
    expect(server.update.mock.calls.map((c) => c[0])).toEqual(['t3']);
    expect(server.made).toEqual(['t1', 't2', 't3']);
  });
});

describe('TaskCsvImport title-only task (audit W3)', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
    vi.spyOn(api, 'assignable').mockResolvedValue([]);
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  // 만들기(제목만) 뒤 고치기가 거부되면 그 업무는 제목만 있는 채로 남는다. 어느 업무인지 알린다.
  it('names the task left with only its title when filling it in fails', async () => {
    fakeServer('B', new ApiError(400, 'bad_request', 'at most 20 tags'));
    const onimported = vi.fn(() => Promise.resolve());
    render(TaskCsvImport, { itemId: 'i1', onimported });
    await setText('A\nB\nC');
    await pressImport(onimported, 1);
    expect(screen.getByText(/1개 등록됨/)).toHaveTextContent(
      '"B" 업무는 제목만 만들어졌습니다. 다시 누르면 그 업무에 나머지를 채웁니다.',
    );
  });

  // 제목만 만들어진 업무를 보드에서 지우면 그 업무에 채울 수 없다(404). 같은 요청 id로 만들면 지운 업무가 돌아오므로
  // 다음 누름은 새 요청 id로 한 번만 새로 만든다.
  it('creates the row anew once when its title-only task was deleted', async () => {
    const server = fakeServer('B', new ApiError(400, 'bad_request', 'at most 20 tags'));
    const onimported = vi.fn(() => Promise.resolve());
    render(TaskCsvImport, { itemId: 'i1', onimported });
    await setText('A\nB');
    await pressImport(onimported, 1);
    const firstIds = server.create.mock.calls.map((c) => c[1]);
    server.remove('t2');
    await pressImport(onimported, 2);
    const message = screen.getByText(/1개 등록됨/);
    expect(message).toHaveTextContent(
      '"B" 업무가 지워졌거나 볼 수 없습니다. 다시 누르면 새로 만듭니다.',
    );
    expect(message).not.toHaveTextContent('제목만 만들어졌습니다');
    server.create.mockClear();
    server.update.mockClear();
    await pressImport(onimported, 3);
    expect(server.create).toHaveBeenCalledTimes(1);
    expect(firstIds).not.toContain(server.create.mock.calls[0]?.[1]);
    expect(server.update.mock.calls.map((c) => c[0])).toEqual(['t3']);
    expect(server.made).toEqual(['t1', 't2', 't3']);
    expect(screen.getByText('업무 2개를 등록했습니다.')).toBeInTheDocument();
  });

  // 만들기 응답을 잃은 줄의 업무(보드에는 보인다)를 지우면 같은 요청 id의 만들기가 404다. 다음 누름은 새 id로 만든다.
  it('creates the row anew when the task of a lost create was deleted', async () => {
    const server = fakeServer('', new NetworkError('offline'));
    server.loseNextCreate();
    const onimported = vi.fn(() => Promise.resolve());
    render(TaskCsvImport, { itemId: 'i1', onimported });
    await setText('A');
    await pressImport(onimported, 1);
    server.remove('t1');
    await pressImport(onimported, 2);
    expect(screen.getByText(/0개 등록됨/)).toBeInTheDocument();
    await pressImport(onimported, 3);
    const ids = server.create.mock.calls.map((c) => c[1]);
    expect(ids[1]).toBe(ids[0]);
    expect(ids[2]).not.toBe(ids[0]);
    expect(server.made).toEqual(['t1', 't2']);
    expect(screen.getByText('업무 1개를 등록했습니다.')).toBeInTheDocument();
  });
});

// 고치기가 서버에 반영되고 응답만 잃으면 그 줄은 아직 다 넣지 못한 줄로 남아, 다시 누를 때 옛 판으로 고치기를 보내 409를
// 받는다. 서버 값이 이 줄과 같으면 다 넣은 것이다: 다른 곳에서 바뀌었다고 알리지 않고, 한 번 더 누르게 하지 않는다.
describe('TaskCsvImport lost update response (review)', () => {
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

  // 담당자는 이름순, 같은 태그는 합쳐 저장되므로 줄과 순서·개수가 달라도 같은 내용으로 본다.
  const rows = 'A\nB,설명,진행 중,높음,2026-11-02,2026-11-09,30,나래;가람,회의;보고;회의';

  async function lostUpdateOfB(
    server: ReturnType<typeof fakeServer>,
  ): Promise<() => Promise<void>> {
    server.loseUpdate('B');
    const onimported = vi.fn(() => Promise.resolve());
    render(TaskCsvImport, { itemId: 'i1', onimported });
    await setText(rows);
    await waitFor(() => {
      expect(screen.getByRole('button', { name: '등록' })).toBeEnabled();
    });
    await pressImport(onimported, 1);
    expect(screen.getByText(/1개 등록됨/)).toBeInTheDocument();
    server.update.mockClear();
    return () => pressImport(onimported, 2);
  }

  it('treats its own update whose response was lost as done, not as a conflict', async () => {
    const server = fakeServer('', new NetworkError('offline'));
    const pressAgain = await lostUpdateOfB(server);
    await pressAgain();
    expect(screen.getByText('업무 2개를 등록했습니다.')).toBeInTheDocument();
    expect(screen.queryByText(/그사이 다른 곳에서 바뀌었습니다/)).not.toBeInTheDocument();
    // 다시 누름은 B에 한 번(409) 보낼 뿐 같은 값으로 덮어쓰지 않는다.
    expect(server.update.mock.calls.map((c) => c[0])).toEqual(['t2']);
  });

  it('still reports a conflict when someone else changed the task after the lost response', async () => {
    const server = fakeServer('', new NetworkError('offline'));
    const pressAgain = await lostUpdateOfB(server);
    server.bump('t2');
    await pressAgain();
    expect(screen.getByText(/1개 등록됨/)).toHaveTextContent(
      'B: 그사이 다른 곳에서 바뀌었습니다. 다시 누르면 이 줄 내용으로 덮어씁니다.',
    );
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

  it('never hands a finished row to a row with other content', () => {
    const doneA: RowProgress = {
      requestId: 'r1',
      title: 'A',
      task: { id: 't1', version: 2 },
      applied: canonicalJson(row('A', 'x')),
    };
    const [step] = matchRows([row('A', 'z')], [doneA]);
    expect(step?.progress).not.toBe(doneA);
    expect(step?.progress.requestId).not.toBe('r1');
    expect(step?.progress.task).toBeUndefined();
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
