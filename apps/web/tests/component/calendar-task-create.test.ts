import type { Module, Workspace } from '@fvoci/api-client';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { CalendarView, ItemView, TaskDetailView } from '$lib/api/client';
import { api, NetworkError } from '$lib/api/client';
import CalendarScreen from '$lib/features/events/CalendarScreen.svelte';
import CalendarTaskDialog from '$lib/features/events/CalendarTaskDialog.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

// 달력의 날짜에서 업무 만들기: 대상(항목)을 분명히 하고, 만들기 뒤 마감일을 넣으며, 날짜만 실패하면 다시 만들지 않는다.
const workspace: Workspace = {
  id: 'ws-1',
  name: '공간',
  role: 'writer',
  is_owner: false,
  realtime_enabled: false,
};
const empty: CalendarView = {
  tasks: [],
  projected_tasks: [],
  events: [],
  occurrences: [],
  dates: [],
  items: [],
};

function itemView(id: string, title: string, over: Partial<ItemView> = {}): ItemView {
  return {
    id,
    module_id: 'm1',
    workspace_id: 'ws-1',
    reference: `KED-${id}`,
    title,
    description: '',
    visibility: 'workspace',
    access: 'write',
    archived: false,
    updated_at: '2026-10-21T00:00:00Z',
    status: 'active',
    is_template: false,
    starts_on: null,
    ends_on: null,
    ...over,
  };
}
function created(version = 1, due: string | null = null): TaskDetailView {
  return {
    id: 't-new',
    item_id: 'i1',
    module_id: 'm1',
    workspace_id: 'ws-1',
    parent_id: null,
    reference: 'KED-9',
    title: '자료 준비',
    description: '',
    status: 'todo',
    priority: 'none',
    start_date: null,
    due_date: due,
    estimate_minutes: null,
    tags: [],
    milestone: false,
    predecessor_ids: [],
    series: null,
    occurrence_date: null,
    assignees: [],
    checklist_done: 0,
    checklist_total: 0,
    version,
    completed_at: null,
    access: 'write',
    updated_at: '2026-10-21T00:00:00Z',
    checklist: [],
  };
}
const module: Module = {
  id: 'm1',
  workspace_id: 'ws-1',
  kind: 'project',
  name: '프로젝트',
  key: 'KED',
  description: '',
  icon: '',
  archived: false,
  previous_keys: [],
};

describe('creating a task from a calendar day', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
  });
  afterEach(() => {
    vi.restoreAllMocks();
  });

  async function fill(title: string): Promise<void> {
    const dialog = await screen.findByRole('dialog');
    await fireEvent.input(within(dialog).getByLabelText('업무 제목'), { target: { value: title } });
    await fireEvent.click(within(dialog).getByRole('button', { name: '업무 만들기' }));
  }

  it('creates it in the fixed item and puts the chosen day as the due date', async () => {
    const create = vi.spyOn(api, 'createTask').mockResolvedValue(created());
    const update = vi
      .spyOn(api, 'updateTask')
      .mockResolvedValue({ kind: 'updated', task: created(2, '2026-10-23') });
    const oncreated = vi.fn();
    render(CalendarTaskDialog, {
      props: {
        workspaceId: 'ws-1',
        day: '2026-10-23',
        item: { id: 'i1', title: '대표 항목' },
        onclose: () => undefined,
        oncreated,
      },
    });
    expect(await screen.findByText('만들 항목: 대표 항목')).toBeTruthy();
    await fill('자료 준비');
    await waitFor(() => {
      expect(oncreated).toHaveBeenCalled();
    });
    expect(create.mock.calls[0]?.[0]).toBe('i1');
    expect(create.mock.calls[0]?.[2]).toBe('자료 준비');
    expect(update.mock.calls[0]?.[1]).toBe(1);
    expect(update.mock.calls[0]?.[2]).toMatchObject({ title: '자료 준비', due_date: '2026-10-23' });
  });

  it('when only the date fails, keeps the task and sets the date again without creating twice', async () => {
    const create = vi.spyOn(api, 'createTask').mockResolvedValue(created());
    const update = vi
      .spyOn(api, 'updateTask')
      .mockRejectedValueOnce(new NetworkError('offline'))
      .mockResolvedValueOnce({ kind: 'updated', task: created(2, '2026-10-23') });
    const read = vi.spyOn(api, 'task').mockResolvedValue(created());
    const oncreated = vi.fn();
    render(CalendarTaskDialog, {
      props: {
        workspaceId: 'ws-1',
        day: '2026-10-23',
        item: { id: 'i1', title: '대표 항목' },
        onclose: () => undefined,
        oncreated,
      },
    });
    await fill('자료 준비');
    const alert = await screen.findByRole('alert');
    expect(alert.textContent).toContain('업무는 만들었지만 마감일을 넣지 못했습니다');
    expect(within(alert).getByRole('link', { name: '자료 준비 열기' })).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: '마감일 다시 넣기' }));
    await waitFor(() => {
      expect(oncreated).toHaveBeenCalled();
    });
    expect(create).toHaveBeenCalledTimes(1);
    expect(read).toHaveBeenCalledWith('t-new');
    expect(update).toHaveBeenCalledTimes(2);
  });

  it('tells the caller a task was created when closed after the date failed', async () => {
    vi.spyOn(api, 'createTask').mockResolvedValue(created());
    vi.spyOn(api, 'updateTask').mockRejectedValue(new NetworkError('offline'));
    const onclose = vi.fn<(made: boolean) => void>();
    render(CalendarTaskDialog, {
      props: {
        workspaceId: 'ws-1',
        day: '2026-10-23',
        item: { id: 'i1', title: '대표 항목' },
        onclose,
        oncreated: () => undefined,
      },
    });
    await fill('자료 준비');
    await screen.findByRole('alert');
    await fireEvent.click(screen.getByRole('button', { name: '취소' }));
    expect(onclose).toHaveBeenCalledWith(true);
  });

  it('in the workspace calendar asks for the module and offers only items it can add tasks to', async () => {
    vi.spyOn(api, 'modules').mockResolvedValue([module]);
    vi.spyOn(api, 'items').mockResolvedValue([
      itemView('i1', '쓸 수 있는 항목'),
      itemView('i2', '읽기만 하는 항목', { access: 'read' }),
      itemView('i3', '보관한 항목', { archived: true }),
      itemView('i4', '서식 항목', { is_template: true }),
    ]);
    const create = vi.spyOn(api, 'createTask').mockResolvedValue(created());
    vi.spyOn(api, 'updateTask').mockResolvedValue({ kind: 'updated', task: created(2) });
    render(CalendarTaskDialog, {
      props: {
        workspaceId: 'ws-1',
        day: '2026-10-23',
        onclose: () => undefined,
        oncreated: () => undefined,
      },
    });
    const dialog = await screen.findByRole('dialog');
    // 모듈을 고르기 전에는 만들 수 없다.
    await fill('자료 준비');
    expect((await within(dialog).findByRole('alert')).textContent).toContain(
      '업무를 만들 항목을 고르세요',
    );
    expect(create).not.toHaveBeenCalled();
    await waitFor(() => {
      expect(within(dialog).getByRole('option', { name: '프로젝트 (KED)' })).toBeTruthy();
    });
    await fireEvent.change(within(dialog).getByLabelText('모듈'), { target: { value: 'm1' } });
    await waitFor(() => {
      expect(
        within(within(dialog).getByLabelText('항목'))
          .getAllByRole('option')
          .map((o) => o.textContent),
      ).toEqual(['쓸 수 있는 항목']);
    });
    await fireEvent.click(within(dialog).getByRole('button', { name: '업무 만들기' }));
    await waitFor(() => {
      expect(create).toHaveBeenCalledWith('i1', expect.any(String), '자료 준비');
    });
  });

  it('does not create in the previous module item while the new module items are loading', async () => {
    const other: Module = { ...module, id: 'm2', name: '연구', key: 'RES' };
    vi.spyOn(api, 'modules').mockResolvedValue([module, other]);
    vi.spyOn(api, 'items').mockImplementation((id: string) =>
      id === 'm1'
        ? Promise.resolve([itemView('i1', '앞 모듈 항목')])
        : new Promise<ItemView[]>(() => undefined),
    );
    const create = vi.spyOn(api, 'createTask').mockResolvedValue(created());
    render(CalendarTaskDialog, {
      props: {
        workspaceId: 'ws-1',
        day: '2026-10-23',
        onclose: () => undefined,
        oncreated: () => undefined,
      },
    });
    const dialog = await screen.findByRole('dialog');
    await waitFor(() => {
      expect(within(dialog).getByRole('option', { name: '연구 (RES)' })).toBeTruthy();
    });
    await fireEvent.change(within(dialog).getByLabelText('모듈'), { target: { value: 'm1' } });
    await within(dialog).findByRole('option', { name: '앞 모듈 항목' });
    await fireEvent.change(within(dialog).getByLabelText('모듈'), { target: { value: 'm2' } });
    await fill('자료 준비');
    expect((await within(dialog).findByRole('alert')).textContent).toContain(
      '업무를 만들 항목을 고르세요',
    );
    expect(create).not.toHaveBeenCalled();
  });

  it('cannot be closed while the task is being created', async () => {
    vi.spyOn(api, 'createTask').mockReturnValue(new Promise(() => undefined));
    const onclose = vi.fn<(made: boolean) => void>();
    render(CalendarTaskDialog, {
      props: {
        workspaceId: 'ws-1',
        day: '2026-10-23',
        item: { id: 'i1', title: '대표 항목' },
        onclose,
        oncreated: () => undefined,
      },
    });
    await fill('자료 준비');
    const dialog = screen.getByRole('dialog');
    expect(within(dialog).getByRole('button', { name: '취소' })).toHaveProperty('disabled', true);
    await fireEvent.keyDown(document.activeElement ?? dialog, { key: 'Escape' });
    expect(screen.getByRole('dialog')).toBeTruthy();
    expect(onclose).not.toHaveBeenCalled();
  });

  it('offers day buttons only when the item can take tasks', async () => {
    vi.spyOn(api, 'calendar').mockResolvedValue(empty);
    const first = render(CalendarScreen, {
      props: {
        workspace,
        scope: { itemId: 'i1' },
        item: itemView('i1', '읽기 항목', { access: 'read' }),
        embedded: true,
      },
    });
    await waitFor(() => {
      expect(api.calendar).toHaveBeenCalled();
    });
    expect(document.querySelectorAll('[data-create-day]')).toHaveLength(0);
    first.unmount();
    render(CalendarScreen, {
      props: { workspace, scope: { itemId: 'i1' }, item: itemView('i1', '항목'), embedded: true },
    });
    await waitFor(() => {
      expect(document.querySelectorAll('[data-create-day]').length).toBe(42);
    });
    const day = document.querySelector<HTMLElement>('[data-create-day]');
    expect(day?.getAttribute('aria-label')).toMatch(/^\d{4}-\d{2}-\d{2}에 업무 만들기$/);
  });
});
