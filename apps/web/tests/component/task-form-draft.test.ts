import { workApi as work } from '$lib/features/tasks/api';
import './fixtures/task-statuses';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { TaskDetailView } from '$lib/api/client';
import { api } from '$lib/api/client';
import { accountSafety, onAccountSwitch } from '$lib/features/feedback/feedback.svelte';
import TaskDetail from '$lib/features/tasks/TaskDetail.svelte';
import { me } from '$lib/me.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

// D-96·D-128(PR #141 r4372766851): 업무 편집 폼의 저장하지 않은 입력은 편집한 계정의 브라우저 초안(사용자별·탭별 키)으로 남고,
// 다시 읽은 뒤 같은 계정에게만 되살릴지 묻는다. 되살린 입력은 사용자가 "저장"을 눌러야 보내며(자동 재전송 없음) 남길 때의
// 판으로 보낸다. 문서 초안 시험(document-save-account-changed)과 별개의 업무 폼 시험이다. 아래 409는 서버 기대 사용자
// 계약에 대한 클라이언트 반응을 흉내 내며, 서버 자체 검증은 서버 통합 시험이 담당한다.

const A = 'user-a';
const B = 'user-b';
const EDIT = '앞 계정이 치고 저장하지 않은 설명';

const prefix = (user: string): string => `fvoci:${user}:-:form-draft:task:t-1:`;
const keysOf = (user: string): string[] =>
  Object.keys(window.localStorage).filter((k) => k.startsWith(prefix(user)));

/** 남은 초안의 설명과 기준 판. */
function stored(user: string): { description: string; baseVersion: number }[] {
  return keysOf(user).map((k) => {
    const value: unknown = JSON.parse(window.localStorage.getItem(k) ?? 'null');
    if (
      typeof value !== 'object' ||
      value === null ||
      !('form' in value) ||
      !('baseVersion' in value) ||
      typeof value.baseVersion !== 'number'
    )
      throw new Error('draft shape');
    const { form } = value;
    if (
      typeof form !== 'object' ||
      form === null ||
      !('description' in form) ||
      typeof form.description !== 'string'
    )
      throw new Error('draft form shape');
    return { description: form.description, baseVersion: value.baseVersion };
  });
}

function detail(over: Partial<TaskDetailView> = {}): TaskDetailView {
  return {
    id: 't-1',
    item_id: 'i1',
    module_id: 'm1',
    workspace_id: 'ws-1',
    parent_id: null,
    reference: 'KED-1',
    title: '설계',
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
    version: 1,
    move_version: 0,
    completed_at: null,
    access: 'write',
    updated_at: '2026-10-26T00:00:00Z',
    checklist: [],
    ...over,
  };
}

function mount(
  server: TaskDetailView,
  onunsaved: (dirty: boolean) => void = (): void => undefined,
): void {
  vi.spyOn(work, 'task').mockResolvedValue(server);
  vi.spyOn(work, 'assignable').mockResolvedValue([]);
  vi.spyOn(api, 'taskBacklinks').mockResolvedValue([]);
  vi.spyOn(api, 'comments').mockResolvedValue([]);
  render(TaskDetail, {
    props: {
      taskId: 't-1',
      tasks: [server],
      workspaces: [],
      onchanged: () => undefined,
      onopen: () => undefined,
      onunsaved,
    },
  });
}

async function descriptionBox(): Promise<HTMLTextAreaElement> {
  const box = await screen.findByLabelText('설명', { exact: true });
  if (!(box instanceof HTMLTextAreaElement)) throw new Error('설명');
  return box;
}

async function typeDescription(text: string): Promise<void> {
  await fireEvent.input(await descriptionBox(), { target: { value: text } });
}

async function titleValue(): Promise<string> {
  const box = await screen.findByLabelText('제목', { exact: true });
  if (!(box instanceof HTMLInputElement)) throw new Error('제목');
  return box.value;
}

/**
 * 새로고침처럼 저장소는 남기고 화면만 새로 그린다. 언마운트는 사용자가 버리고 닫은 것으로 보고 이 탭의 입력을 지우므로,
 * 새로고침(여기를 거치지 않는다)을 흉내 낼 때는 저장소를 되돌린다.
 */
function reloadLike(): void {
  const kept = Object.keys(window.localStorage).map(
    (k) => [k, window.localStorage.getItem(k) ?? ''] as const,
  );
  cleanup();
  window.localStorage.clear();
  for (const [k, v] of kept) window.localStorage.setItem(k, v);
}

const json = (status: number, body: unknown): Response =>
  new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });

describe('task form input across reloads and account switches', () => {
  it('keeps the dirty warning when a save discovers read-only access', async () => {
    const changed = vi.fn();
    mount(detail(), changed);
    await typeDescription('권한 철회 전 입력');
    vi.mocked(work.task).mockResolvedValue(detail({ access: 'read', version: 2 }));
    vi.spyOn(work, 'addChecklist').mockResolvedValue(detail());
    await fireEvent.input(screen.getByLabelText('새 항목'), { target: { value: '항목' } });
    await fireEvent.click(screen.getByRole('button', { name: '추가' }));
    await waitFor(() => {
      expect(screen.getByLabelText('제목', { exact: true })).toBeDisabled();
    });
    expect(changed).toHaveBeenLastCalledWith(true);
    expect(stored(A)[0]?.description).toBe('권한 철회 전 입력');
  });
  it.each(['새 댓글', '새 항목'])(
    'includes the %s child editor in the parent leave warning',
    async (label) => {
      const changed = vi.fn();
      mount(detail(), changed);
      await fireEvent.input(await screen.findByLabelText(new RegExp(`^${label}`)), {
        target: { value: '입력 보존' },
      });
      await waitFor(() => {
        expect(changed).toHaveBeenLastCalledWith(true);
      });
    },
  );
  it('keeps a subtask title and request identity for explicit restoration and marks it unsaved', async () => {
    const changed = vi.fn();
    mount(detail(), changed);
    await fireEvent.input(await screen.findByLabelText('하위 업무 제목'), {
      target: { value: '하위 초안' },
    });
    await waitFor(() => {
      expect(changed).toHaveBeenLastCalledWith(true);
    });
    expect(keysOf(A)).toHaveLength(1);
    reloadLike();
    mount(detail());
    await screen.findByTestId('task-draft-offer');
    expect(screen.getByLabelText('하위 업무 제목')).toHaveValue('');
    await fireEvent.click(screen.getByRole('button', { name: '입력 복원' }));
    expect(screen.getByLabelText('하위 업무 제목')).toHaveValue('하위 초안');
  });
  it('treats a changed relationship lag as unsaved even without editing the task body', async () => {
    const changed = vi.fn();
    mount(detail({ predecessors: [{ task_id: 't-0', kind: 'FS', lag_days: 0 }] }), changed);
    await fireEvent.click(await screen.findByRole('button', { name: /관계 고치기/ }));
    await fireEvent.input(await screen.findByLabelText('지연(일)'), { target: { value: '3' } });
    await waitFor(() => {
      expect(changed).toHaveBeenLastCalledWith(true);
    });
  });
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
    window.localStorage.clear();
    me.userId = A;
    accountSafety.switching = false;
    vi.spyOn(globalThis, 'fetch').mockImplementation(() =>
      Promise.resolve(json(404, { error: 'not_found', message: 'not found' })),
    );
  });
  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
    me.userId = '';
    accountSafety.switching = false;
    window.localStorage.clear();
  });

  it('keeps a changed input under the editing account only and drops it once it matches the server again', async () => {
    mount(detail());
    await typeDescription(EDIT);
    await waitFor(() => {
      expect(stored(A)).toEqual([{ description: EDIT, baseVersion: 1 }]);
    });
    expect(keysOf(B)).toEqual([]);
    await typeDescription('');
    await waitFor(() => {
      expect(keysOf(A)).toEqual([]);
    });
  });

  it('a save refused with account_changed starts the global switch, sends nothing more and keeps the hidden input stored', async () => {
    let puts = 0;
    vi.spyOn(globalThis, 'fetch').mockImplementation((input, init) => {
      if (input === '/api/tasks/t-1' && init?.method === 'PUT') {
        puts += 1;
        return Promise.resolve(json(409, { error: 'account_changed', message: 'account changed' }));
      }
      return Promise.resolve(json(404, { error: 'not_found', message: 'not found' }));
    });
    const reload = vi.fn<() => void>();
    const stop = onAccountSwitch(reload);
    try {
      mount(detail());
      await typeDescription(EDIT);
      await fireEvent.click(await screen.findByRole('button', { name: /^저장$/ }));
      await waitFor(() => {
        expect(accountSafety.switching).toBe(true);
      });
      expect(reload).toHaveBeenCalledTimes(1);
      expect(puts).toBe(1);
      // 폼은 숨고 입력은 앞 계정의 초안으로 남는다. 다른 계정의 키에는 아무것도 없다.
      // 폼의 숨김 알림은 오류가 폼에 닿은 뒤에 뜬다(전환 표시는 그 전에 이미 켜졌다).
      await screen.findByText(/계정이 바뀌어 입력을 숨겼습니다/);
      expect(stored(A)).toEqual([{ description: EDIT, baseVersion: 1 }]);
      expect(keysOf(B)).toEqual([]);
      await new Promise((resolve) => setTimeout(resolve, 20));
      expect(puts).toBe(1);
    } finally {
      stop();
    }
  });

  it('offers the kept input to the same account after a reload and saves it once, with the kept base version, only after an explicit restore and save', async () => {
    mount(detail());
    await typeDescription(EDIT);
    await waitFor(() => {
      expect(stored(A)).toHaveLength(1);
    });
    reloadLike();

    const update = vi.spyOn(work, 'updateTask').mockResolvedValue({
      kind: 'updated',
      task: detail({ description: EDIT, version: 2 }),
    });
    mount(detail());
    const offer = await screen.findByTestId('task-draft-offer');
    expect(offer).toHaveTextContent('저장하지 않은 입력');
    // 제안만 한다: 폼은 서버 값이고 보낸 것은 없다.
    expect((await descriptionBox()).value).toBe('');
    expect(update).not.toHaveBeenCalled();

    await fireEvent.click(screen.getByRole('button', { name: /^입력 복원$/ }));
    expect((await descriptionBox()).value).toBe(EDIT);
    expect(screen.queryByTestId('task-draft-offer')).toBeNull();
    expect(update).not.toHaveBeenCalled();

    await fireEvent.click(screen.getByRole('button', { name: /^저장$/ }));
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(1);
    });
    expect(update).toHaveBeenCalledWith(
      A,
      't-1',
      1,
      expect.objectContaining({ description: EDIT, title: '설계' }),
      [],
    );
    await waitFor(() => {
      expect(keysOf(A)).toEqual([]);
    });
  });

  it('shows nothing to another account and leaves the first account’s input untouched', async () => {
    mount(detail());
    await typeDescription(EDIT);
    await waitFor(() => {
      expect(stored(A)).toHaveLength(1);
    });
    reloadLike();

    me.userId = B;
    mount(detail());
    expect((await descriptionBox()).value).toBe('');
    expect(screen.queryByTestId('task-draft-offer')).toBeNull();
    expect(stored(A)).toEqual([{ description: EDIT, baseVersion: 1 }]);
    expect(keysOf(B)).toEqual([]);
  });

  it('drops a kept input silently when the server already has it', async () => {
    mount(detail());
    await typeDescription(EDIT);
    await waitFor(() => {
      expect(stored(A)).toHaveLength(1);
    });
    reloadLike();

    mount(detail({ description: EDIT, version: 2 }));
    expect((await descriptionBox()).value).toBe(EDIT);
    expect(screen.queryByTestId('task-draft-offer')).toBeNull();
    expect(keysOf(A)).toEqual([]);
  });

  it('offers only discarding when the account can no longer edit the task', async () => {
    mount(detail());
    await typeDescription(EDIT);
    await waitFor(() => {
      expect(stored(A)).toHaveLength(1);
    });
    reloadLike();

    mount(detail({ access: 'read' }));
    const offer = await screen.findByTestId('task-draft-offer');
    expect(offer).toHaveTextContent('편집할 수 없어');
    expect(screen.queryByRole('button', { name: /^입력 복원$/ })).toBeNull();
    expect(stored(A)).toHaveLength(1);
    await fireEvent.click(screen.getByRole('button', { name: /^입력 버리기$/ }));
    expect(screen.queryByTestId('task-draft-offer')).toBeNull();
    expect(keysOf(A)).toEqual([]);
  });

  it('merges the kept input onto a newer server version and saves against that version', async () => {
    mount(detail());
    await typeDescription(EDIT);
    await waitFor(() => {
      expect(stored(A)).toHaveLength(1);
    });
    reloadLike();

    const update = vi.spyOn(work, 'updateTask').mockResolvedValue({
      kind: 'updated',
      task: detail({ title: '설계 (3판)', description: EDIT, version: 4 }),
    });
    mount(detail({ title: '설계 (3판)', version: 3 }));
    await screen.findByTestId('task-draft-offer');
    await fireEvent.click(screen.getByRole('button', { name: /^입력 복원$/ }));
    // 내가 고친 설명은 내 값, 고치지 않은 제목은 서버의 새 값.
    expect((await descriptionBox()).value).toBe(EDIT);
    expect(await titleValue()).toBe('설계 (3판)');
    await fireEvent.click(screen.getByRole('button', { name: /^저장$/ }));
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(1);
    });
    expect(update).toHaveBeenCalledWith(
      A,
      't-1',
      3,
      expect.objectContaining({ description: EDIT, title: '설계 (3판)' }),
      [],
    );
  });

  it('clears the input when the form is closed by the user but keeps it through an account switch', async () => {
    mount(detail());
    await typeDescription(EDIT);
    await waitFor(() => {
      expect(stored(A)).toHaveLength(1);
    });
    // 버리고 닫기(언마운트): 지운다.
    cleanup();
    expect(keysOf(A)).toEqual([]);

    mount(detail());
    await typeDescription(EDIT);
    await waitFor(() => {
      expect(stored(A)).toHaveLength(1);
    });
    // 다른 계정으로 바뀌어 다시 읽는 중: 남긴다.
    accountSafety.switching = true;
    cleanup();
    expect(stored(A)).toEqual([{ description: EDIT, baseVersion: 1 }]);
  });
});

// D-128 대표 경로의 "선행 지정": 관계 편집 칸(고치는 관계·선행 업무·종류·지연일)도 같은 초안에 남고, 되살린 뒤 사용자가
// "관계 저장"을 눌러야 보낸다. 되살린 기준 관계가 그사이 바뀌었으면 서버의 409가 최신 관계를 보이고 입력은 남는다.
describe('relation editor input across reloads', () => {
  const predecessor: TaskDetailView = detail({
    id: 't-0',
    reference: 'KED-0',
    title: '설계',
    start_date: '2026-10-19',
    due_date: '2026-10-21',
  });
  const linked = (): TaskDetailView =>
    detail({
      predecessors: [{ task_id: 't-0', kind: 'FS', lag_days: 0 }],
      start_date: '2026-10-22',
      due_date: '2026-10-24',
    });
  function mountLinked(server: TaskDetailView = linked()): void {
    vi.spyOn(work, 'task').mockResolvedValue(server);
    vi.spyOn(work, 'assignable').mockResolvedValue([]);
    vi.spyOn(api, 'taskBacklinks').mockResolvedValue([]);
    vi.spyOn(api, 'comments').mockResolvedValue([]);
    render(TaskDetail, {
      props: {
        taskId: 't-1',
        tasks: [predecessor, server],
        workspaces: [],
        onchanged: () => undefined,
        onopen: () => undefined,
      },
    });
  }
  async function lagBox(): Promise<HTMLInputElement> {
    const box = await screen.findByLabelText('지연(일)');
    if (!(box instanceof HTMLInputElement)) throw new Error('지연(일)');
    return box;
  }
  function storedLink(): unknown[] {
    return keysOf(A).map((k) => {
      const value: unknown = JSON.parse(window.localStorage.getItem(k) ?? 'null');
      if (typeof value !== 'object' || value === null || !('link' in value))
        throw new Error('draft');
      return value.link;
    });
  }

  beforeEach(async () => {
    await setLocale('ko', { reload: false });
    window.localStorage.clear();
    me.userId = A;
    accountSafety.switching = false;
    vi.spyOn(globalThis, 'fetch').mockImplementation(() =>
      Promise.resolve(json(404, { error: 'not_found', message: 'not found' })),
    );
  });
  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
    me.userId = '';
    accountSafety.switching = false;
    window.localStorage.clear();
  });

  it('keeps a relation being edited even when the form itself is unchanged, and restores it for an explicit save', async () => {
    mountLinked();
    await fireEvent.click(await screen.findByRole('button', { name: 'KED-0 설계 관계 고치기' }));
    await fireEvent.input(await lagBox(), { target: { value: '3' } });
    await waitFor(() => {
      expect(storedLink()).toEqual([
        {
          editing: 't-0',
          predecessor: '',
          kind: 'FS',
          lag: '3',
          base: { kind: 'FS', lag_days: 0 },
        },
      ]);
    });
    reloadLike();

    // 폼 본문은 서버와 같지만 관계 입력이 남아 있으므로 제안한다.
    mountLinked();
    await screen.findByTestId('task-draft-offer');
    const preview = vi
      .spyOn(work, 'previewLink')
      .mockResolvedValue({ shifts: [], undated: [], blocked: null });
    const set = vi.spyOn(work, 'setPredecessor').mockResolvedValue({
      kind: 'updated',
      task: detail({ predecessors: [{ task_id: 't-0', kind: 'FS', lag_days: 3 }], version: 2 }),
    });
    await fireEvent.click(screen.getByRole('button', { name: /^입력 복원$/ }));
    expect((await lagBox()).value).toBe('3');
    expect(document.getElementById('predecessor-label')).toHaveTextContent('고치기');
    expect(set).not.toHaveBeenCalled();

    await fireEvent.click(screen.getByRole('button', { name: /^관계 저장$/ }));
    await waitFor(() => {
      expect(set).toHaveBeenCalledTimes(1);
    });
    expect(preview).toHaveBeenCalledTimes(1);
    expect(set).toHaveBeenCalledWith(A, 't-1', 't-0', 'FS', 3, { kind: 'FS', lag_days: 0 }, []);
    await waitFor(() => {
      expect(keysOf(A)).toEqual([]);
    });
  });

  it('after restoring, a relation changed elsewhere is reported by the server and the input stays', async () => {
    mountLinked();
    await fireEvent.click(await screen.findByRole('button', { name: 'KED-0 설계 관계 고치기' }));
    await fireEvent.input(await lagBox(), { target: { value: '3' } });
    await waitFor(() => {
      expect(storedLink()).toHaveLength(1);
    });
    reloadLike();

    mountLinked();
    await screen.findByTestId('task-draft-offer');
    vi.spyOn(work, 'previewLink').mockResolvedValue({ shifts: [], undated: [], blocked: null });
    const set = vi.spyOn(work, 'setPredecessor').mockResolvedValue({
      kind: 'link_conflict',
      current: detail({ predecessors: [{ task_id: 't-0', kind: 'SS', lag_days: 1 }], version: 2 }),
    });
    await fireEvent.click(screen.getByRole('button', { name: /^입력 복원$/ }));
    await fireEvent.click(screen.getByRole('button', { name: /^관계 저장$/ }));
    await waitFor(() => {
      expect(set).toHaveBeenCalledTimes(1);
    });
    // 서버의 최신 관계를 알리고 내 입력(지연 3)은 그대로다. 다음 저장은 최신 관계를 기준으로 보낸다.
    expect(await screen.findByRole('alert')).toHaveTextContent('관계가');
    expect((await lagBox()).value).toBe('3');
  });

  it('shows no relation offer to another account and clears the relation input on close', async () => {
    mountLinked();
    await fireEvent.click(await screen.findByRole('button', { name: 'KED-0 설계 관계 고치기' }));
    await fireEvent.input(await lagBox(), { target: { value: '3' } });
    await waitFor(() => {
      expect(storedLink()).toHaveLength(1);
    });
    reloadLike();
    me.userId = B;
    mountLinked();
    await descriptionBox();
    expect(screen.queryByTestId('task-draft-offer')).toBeNull();
    expect(keysOf(A)).toHaveLength(1);
    cleanup();

    me.userId = A;
    mountLinked();
    await screen.findByTestId('task-draft-offer');
    // 사용자가 닫으면(언마운트) 이 탭의 입력은 지운다.
    await fireEvent.click(screen.getByRole('button', { name: /^입력 복원$/ }));
    cleanup();
    expect(keysOf(A)).toEqual([]);
  });
});
