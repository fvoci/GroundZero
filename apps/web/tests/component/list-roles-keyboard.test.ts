import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { DocumentView, RevisionView, TaskView } from '$lib/api/client';
import { api } from '$lib/api/client';
import { dayIn } from '$lib/features/events/time';
import RevisionHistory from '$lib/features/documents/RevisionHistory.svelte';
import TaskCalendar from '$lib/features/tasks/TaskCalendar.svelte';
import { me } from '$lib/me.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

afterEach(() => {
  vi.restoreAllMocks();
});

function task(
  id: string,
  title: string,
  status: TaskView['status'],
  statusCategory: TaskView['status_category'],
  due: string,
): TaskView {
  return {
    id,
    item_id: 'i1',
    module_id: 'm1',
    workspace_id: 'ws-1',
    parent_id: null,
    reference: `DEV-${id}`,
    title,
    description: '',
    status,
    priority: 'none',
    start_date: null,
    due_date: due,
    estimate_minutes: null,
    tags: [],
    milestone: false,
    predecessors: [],
    series: null,
    occurrence_date: null,
    assignees: [],
    status_category: statusCategory,
    checklist_done: 0,
    checklist_total: 0,
    version: 1,
    move_version: 1,
    completed_at: null,
    access: 'write',
    updated_at: '2026-10-21T00:00:00Z',
  };
}

// 업무 달력: 방향키로 칸을 옮기지 않으므로 grid 역할을 쓰지 않고, 완료 업무는 취소선만이 아니라 이름에도 상태가 있다.
describe('task calendar roles', () => {
  it('has no grid roles and names a done task with its status', async () => {
    await setLocale('ko', { reload: false });
    const today = dayIn(new Date().toISOString(), me.timeZone === '' ? 'UTC' : me.timeZone);
    render(TaskCalendar, {
      props: { tasks: [task('1', '발표 자료', 'done', 'done', today)], onopen: () => undefined },
    });

    expect(screen.queryByRole('grid')).toBeNull();
    expect(screen.queryAllByRole('gridcell')).toHaveLength(0);
    expect(screen.getByRole('button', { name: /^발표 자료 · done$/ })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /^발표 자료 · done$/ })).toHaveClass('line-through');
  });
});

function revisionAt(revision: number): RevisionView {
  return {
    revision,
    title: '문서',
    author_name: '나',
    created_at: '2026-10-19T00:00:00Z',
    restored_from: null,
    edited_by_names: null,
    body: { type: 'doc', content: [] },
    schema_version: 1,
  };
}

const doc: DocumentView = {
  id: 'd1',
  workspace_id: 'ws-1',
  placement: 'wiki',
  item_id: null,
  parent_id: null,
  reference: 'WIKI-1',
  title: '문서',
  body: { type: 'doc', content: [] },
  schema_version: 1,
  revision: 4,
  updated_at: '2026-10-19T00:00:00Z',
  access: 'owner',
  can_edit: true,
  visibility: 'workspace',
  allow_copy: true,
  can_manage: true,
  can_delete: true,
  is_template: false,
  realtime: false,
  can_export: true,
};

// 리비전 목록은 목록 상자다: Tab으로는 한 칸에만 들어가고 위·아래·Home·End로 옮기며 고른다.
describe('revision list keyboard', () => {
  it('moves and selects with the arrow keys', async () => {
    await setLocale('ko', { reload: false });
    vi.spyOn(api, 'revisions').mockResolvedValue([revisionAt(3), revisionAt(2), revisionAt(1)]);
    const load = vi
      .spyOn(api, 'revision')
      .mockImplementation((_id: string, r: number) => Promise.resolve(revisionAt(r)));
    render(RevisionHistory, { props: { doc, onrestored: () => undefined } });
    await fireEvent.click(screen.getByRole('button', { name: '기록' }));

    const options = await screen.findAllByRole('option');
    const [r3, r2, r1] = options;
    if (r3 === undefined || r2 === undefined || r1 === undefined) throw new Error('options');
    expect(options.map((o) => o.tabIndex)).toEqual([0, -1, -1]);
    r3.focus();
    await fireEvent.keyDown(r3, { key: 'ArrowDown' });
    expect(r2).toHaveFocus();
    await waitFor(() => {
      expect(load).toHaveBeenCalledWith('d1', 2);
    });
    await waitFor(() => {
      expect(r2).toHaveAttribute('aria-selected', 'true');
    });
    expect(r2.tabIndex).toBe(0);

    await fireEvent.keyDown(r2, { key: 'End' });
    expect(r1).toHaveFocus();
    await fireEvent.keyDown(r1, { key: 'Home' });
    expect(r3).toHaveFocus();
  });
});
