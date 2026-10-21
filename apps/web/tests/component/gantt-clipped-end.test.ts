import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { ItemView, TaskDetailView, TaskView } from '$lib/api/client';
import { api } from '$lib/api/client';
import TaskBoard from '$lib/features/tasks/TaskBoard.svelte';
import { MAX_GANTT_DAYS } from '$lib/features/tasks/gantt';
import { setLocale } from '$lib/paraglide/runtime.js';

const item: ItemView = {
  id: 'i1',
  module_id: 'm1',
  workspace_id: 'ws-1',
  reference: 'KED-1',
  title: '항목',
  description: '',
  visibility: 'workspace',
  access: 'write',
  archived: false,
  updated_at: '2026-10-21T00:00:00Z',
  status: 'active',
  is_template: false,
  starts_on: null,
  ends_on: null,
};

function task(id: string, title: string, start: string, due: string): TaskView {
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
    start_date: start,
    due_date: due,
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
    version: 1,
    move_version: 1,
    completed_at: null,
    access: 'write',
    updated_at: '2026-10-21T00:00:00Z',
  };
}

// 400일 제한으로 끝이 잘린 막대(2026-01-01~2028-01-01은 2027-02-04에서 잘림): 끝 손잡이를 끌면 미리 보기는 잘린 끝
// 기준, 저장은 실제 마감(2027-12-31) 기준이라 보이는 날짜와 저장될 날짜가 달랐다. 잘린 끝에는 손잡이를 두지 않고,
// 막대 옮기기의 미리 보기는 실제 날짜로 계산해 표시 범위에 맞춰 자른다.
describe('gantt bar clipped at the end of the shown period', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
    window.sessionStorage.setItem('fvoci:board:i1', JSON.stringify({ view: 'gantt' }));
    vi.spyOn(api, 'savedViews').mockResolvedValue([]);
    vi.spyOn(api, 'syncRecurrences').mockResolvedValue(0);
    Element.prototype.setPointerCapture = (): void => undefined;
  });
  afterEach(() => {
    vi.restoreAllMocks();
    window.sessionStorage.clear();
  });

  const long = task('a', '장기', '2026-01-01', '2028-01-01');
  const short = task('b', '단기', '2026-01-05', '2026-01-10');

  async function mount(): Promise<{ gantt: HTMLElement; bar: HTMLElement }> {
    vi.spyOn(api, 'tasks').mockResolvedValue([long, short]);
    render(TaskBoard, { props: { item, workspaces: [], openTaskId: undefined } });
    const gantt = await screen.findByTestId('task-gantt');
    const bar = gantt.querySelector<HTMLElement>('[data-gantt-bar="a"]');
    if (bar === null) throw new Error('bar');
    return { gantt, bar };
  }

  it('offers no end handle on the clipped end, but still the start handle and both on an unclipped bar', async () => {
    const { gantt, bar } = await mount();
    expect(bar.querySelector('[data-gantt-edge="start"]')).not.toBeNull();
    expect(bar.querySelector('[data-gantt-edge="end"]')).toBeNull();
    const other = gantt.querySelector<HTMLElement>('[data-gantt-bar="b"]');
    expect(other?.querySelector('[data-gantt-edge="start"]')).not.toBeNull();
    expect(other?.querySelector('[data-gantt-edge="end"]')).not.toBeNull();
  });

  it('keeps the clipped bar end at the last column while the bar is dragged', async () => {
    const { bar } = await mount();
    // 열 너비 1.75rem, 막대는 열 수 × 1.75rem − 0.25rem(jsdom은 calc를 계산해 둔다).
    const COL = 1.75;
    const widthOf = (cols: number): string => `calc(${String(cols * COL - 0.25)}rem)`;
    const width = bar.style.width;
    expect(width).toBe(widthOf(MAX_GANTT_DAYS));
    const px =
      1.75 * (Number.parseFloat(getComputedStyle(document.documentElement).fontSize) || 16);
    await fireEvent.pointerDown(bar, { pointerId: 1, button: 0, buttons: 1, clientX: 100 });
    await fireEvent.pointerMove(bar, { pointerId: 1, buttons: 1, clientX: 100 + 2 * px });
    // 두 칸 오른쪽으로: 시작은 두 칸 뒤, 끝은 여전히 표시 기간 끝(실제 마감은 여전히 그 뒤)이라 막대가 두 칸 짧아진다.
    expect(bar.style.left).toBe(`calc(${String(2 * COL + 0.125)}rem)`);
    expect(bar.style.width).toBe(widthOf(MAX_GANTT_DAYS - 2));
    await fireEvent.keyDown(window, { key: 'Escape' });
    expect(bar.style.width).toBe(width);
  });

  it('saves the real dates when the clipped bar is moved, not the clipped column', async () => {
    // 미리 보기는 잘라 그리지만 저장은 실제 날짜로: 2026-01-01~2028-01-01을 두 칸 옮기면 2026-01-03~2028-01-03.
    const detail = (t: TaskView): TaskDetailView => ({ ...t, checklist: [] });
    vi.spyOn(api, 'task').mockResolvedValue(detail(long));
    vi.spyOn(api, 'previewSchedule').mockResolvedValue({ shifts: [], undated: [], blocked: null });
    const update = vi
      .spyOn(api, 'updateTask')
      .mockResolvedValue({ kind: 'updated', task: detail(long) });
    const { bar } = await mount();
    const px =
      1.75 * (Number.parseFloat(getComputedStyle(document.documentElement).fontSize) || 16);
    await fireEvent.pointerDown(bar, { pointerId: 2, button: 0, buttons: 1, clientX: 100 });
    await fireEvent.pointerMove(bar, { pointerId: 2, buttons: 1, clientX: 100 + 2 * px });
    await fireEvent.pointerUp(bar, { pointerId: 2, clientX: 100 + 2 * px });
    await waitFor(() => {
      expect(update).toHaveBeenCalled();
    });
    expect(update.mock.calls[0]?.[2]).toMatchObject({
      start_date: '2026-01-03',
      due_date: '2028-01-03',
    });
  });
});
