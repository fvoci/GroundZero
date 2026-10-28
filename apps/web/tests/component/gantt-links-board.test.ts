import { savedViewsApi } from '$lib/features/events/saved-views-api';
import { taskData as work } from '$lib/features/tasks/detail-data';
import { org } from '$lib/org.svelte';
import './fixtures/task-statuses';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { ItemView, ShiftProposalView, TaskDetailView, TaskView } from '$lib/api/client';
import { ApiError } from '$lib/api/client';
import TaskBoard from '$lib/features/tasks/TaskBoard.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';
import { router } from '$lib/router.svelte';

// 간트 연결선이 막대와 같은 상태를 보이는지: 처음, 끄는 중(미리 보기), 끌기 취소, 미리보기 승인 뒤, 취소, 서버 실패 뒤.
// 연결선은 저장 요청을 만들지 않고, 누르기·끌기·열기·키보드 조작을 막지 않으며, 낭독하지 않는다.
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
  version: 1,
  starts_on: null,
  ends_on: null,
};

function task(
  id: string,
  title: string,
  start: string | null,
  due: string | null,
  preds: string[] = [],
  version = 1,
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
    start_date: start,
    due_date: due,
    estimate_minutes: null,
    tags: [],
    milestone: false,
    predecessors: preds.map((id) => ({ task_id: id, kind: 'FS' as const, lag_days: 0 })),
    series: null,
    occurrence_date: null,
    assignees: [],
    status_category: 'todo',
    checklist_done: 0,
    checklist_total: 0,
    version,
    move_version: version,
    completed_at: null,
    access: 'write',
    updated_at: '2026-10-21T00:00:00Z',
  };
}
const detail = (t: TaskView): TaskDetailView => ({ ...t, checklist: [] });
const design = task('a', '설계', '2026-10-20', '2026-10-22');
const build = task('b', '구현', '2026-10-25', '2026-10-27', ['a']);
const shift: ShiftProposalView = {
  causes: [],
  task_id: 'b',
  reference: 'KED-b',
  title: '구현',
  base_version: 1,
  from_start_date: '2026-10-25',
  from_due_date: '2026-10-27',
  start_date: '2026-10-26',
  due_date: '2026-10-28',
};

/** 막대의 왼쪽·오른쪽 끝(rem). style의 calc(…)는 브라우저가 rem 하나로 줄여 둔다. */
function barEdges(bar: HTMLElement): { left: number; right: number } {
  const rem = (v: string): number => {
    const m = /([-\d.]+)rem/.exec(v);
    if (m === null) throw new Error(`bar style ${bar.style.cssText}`);
    return Number(m[1]);
  };
  const left = rem(bar.style.left);
  return { left, right: left + rem(bar.style.width) };
}
/** 연결선의 첫 점(선행 끝)과 마지막 점(후속 시작)의 x. */
function linkEnds(path: Element): [number, number] {
  const d = path.getAttribute('d') ?? '';
  const start = /^M([-\d.]+)/.exec(d);
  const end = /H([-\d.]+)$/.exec(d);
  return [Number(start?.[1]), Number(end?.[1])];
}

describe('gantt dependency lines on the board', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
    window.sessionStorage.setItem(org.key('board:i1'), JSON.stringify({ view: 'gantt' }));
    vi.spyOn(savedViewsApi, 'savedViews').mockResolvedValue([]);
    vi.spyOn(work, 'syncRecurrences').mockResolvedValue(0);
    Element.prototype.setPointerCapture = (): void => undefined;
  });
  afterEach(() => {
    vi.restoreAllMocks();
    window.sessionStorage.clear();
  });

  async function mount(): Promise<HTMLElement> {
    render(TaskBoard, { props: { item, workspaces: [], openTaskId: undefined } });
    return screen.findByTestId('task-gantt');
  }
  const q = (root: HTMLElement, selector: string): HTMLElement => {
    const el = root.querySelector<HTMLElement>(selector);
    if (el === null) throw new Error(selector);
    return el;
  };
  /** 선행 a의 막대 끝과 후속 b의 막대 시작이 연결선의 두 끝과 같은지. */
  function expectLineOnBars(gantt: HTMLElement): void {
    const a = barEdges(q(gantt, '[data-gantt-bar="a"]'));
    const b = barEdges(q(gantt, '[data-gantt-bar="b"]'));
    const path = q(gantt, '[data-gantt-link="a>b"] path');
    const [x1, x2] = linkEnds(path);
    expect(x1).toBeCloseTo(a.right, 3);
    expect(x2).toBeCloseTo(b.left, 3);
  }

  it('draws the dependency without a tab stop or a second announcement, and keeps rows usable', async () => {
    vi.spyOn(work, 'tasks').mockResolvedValue([design, build]);
    const navigate = vi.spyOn(router, 'navigate').mockImplementation(() => undefined);
    const gantt = await mount();
    const layer = q(gantt, '[data-testid="gantt-links"]');
    expect(layer.className).toContain('pointer-events-none');
    const svg = q(layer, 'svg');
    expect(svg.getAttribute('aria-hidden')).toBe('true');
    expect(svg.querySelector('[tabindex]')).toBeNull();
    expectLineOnBars(gantt);
    // 줄 단추의 설명이 그대로 선행 관계를 말한다(선은 같은 내용을 다시 읽지 않는다).
    expect(q(gantt, '[data-task-card="b"]').getAttribute('aria-label')).toContain('선행 KED-a');
    expect(screen.getByText(/선은 관계 종류에 따라 선행 업무의 시작 또는 끝에서/)).toBeTruthy();
    // 줄을 눌러 열 수 있다(연결선이 가로막지 않음).
    await fireEvent.click(q(gantt, '[data-task-card="b"]'));
    await waitFor(() => {
      expect(navigate).toHaveBeenCalledWith('/w/ws-1/i/i1/tasks/board/t/b');
    });
  });

  it('makes the lines of a focused or hovered row bold, by width and not by colour alone', async () => {
    vi.spyOn(work, 'tasks').mockResolvedValue([
      design,
      build,
      task('c', '정리', '2026-10-28', '2026-10-29', ['b']),
    ]);
    const gantt = await mount();
    const strong = (): string[] =>
      [...gantt.querySelectorAll('[data-gantt-link][data-strong="true"]')].map(
        (g) => g.getAttribute('data-gantt-link') ?? '',
      );
    expect(strong()).toEqual([]);
    await fireEvent.focus(q(gantt, '[data-task-card="a"]'));
    expect(strong()).toEqual(['a>b']);
    expect(q(gantt, '[data-gantt-link="a>b"] path').getAttribute('stroke-width')).toBe('2.5');
    expect(q(gantt, '[data-gantt-link="b>c"] path').getAttribute('stroke-width')).toBe('1.5');
    await fireEvent.blur(q(gantt, '[data-task-card="a"]'));
    await fireEvent.mouseEnter(q(gantt, '[data-task-card="b"]'));
    expect(strong().sort()).toEqual(['a>b', 'b>c']);
    await fireEvent.mouseLeave(q(gantt, '[data-task-card="b"]'));
    expect(strong()).toEqual([]);
    // 키보드 초점이 a에 있는 채로 b를 가리켰다 떼면 a의 강조가 다시 보인다(가리킴과 초점을 따로 둔다).
    await fireEvent.focus(q(gantt, '[data-task-card="a"]'));
    await fireEvent.mouseEnter(q(gantt, '[data-task-card="c"]'));
    expect(strong()).toEqual(['b>c']);
    await fireEvent.mouseLeave(q(gantt, '[data-task-card="c"]'));
    expect(strong()).toEqual(['a>b']);
  });

  it('follows the bar while dragging, goes back on Escape, and sends nothing for the line itself', async () => {
    vi.spyOn(work, 'tasks').mockResolvedValue([design, build]);
    const update = vi.spyOn(work, 'updateTask');
    const preview = vi.spyOn(work, 'previewSchedule');
    const gantt = await mount();
    const bar = q(gantt, '[data-gantt-bar="a"]');
    const before = linkEnds(q(gantt, '[data-gantt-link="a>b"] path'));
    const px =
      1.75 * (Number.parseFloat(getComputedStyle(document.documentElement).fontSize) || 16);
    await fireEvent.pointerDown(bar, { pointerId: 1, button: 0, buttons: 1, clientX: 100 });
    await fireEvent.pointerMove(bar, { pointerId: 1, buttons: 1, clientX: 100 + 2 * px });
    // 끄는 동안의 미리 보기 막대(두 칸 뒤)와 선의 시작이 같다.
    expectLineOnBars(gantt);
    expect(linkEnds(q(gantt, '[data-gantt-link="a>b"] path'))[0]).toBeCloseTo(
      before[0] + 2 * 1.75,
      3,
    );
    await fireEvent.keyDown(window, { key: 'Escape' });
    await fireEvent.pointerUp(bar, { pointerId: 1, clientX: 100 + 2 * px });
    expect(linkEnds(q(gantt, '[data-gantt-link="a>b"] path'))).toEqual(before);
    expectLineOnBars(gantt);
    expect(update).not.toHaveBeenCalled();
    expect(preview).not.toHaveBeenCalled();
  });

  it('hides the line while the follower is dragged before the first column, and brings it back on Escape', async () => {
    vi.spyOn(work, 'tasks').mockResolvedValue([design, build]);
    const update = vi.spyOn(work, 'updateTask');
    const gantt = await mount();
    const bar = q(gantt, '[data-gantt-bar="b"]');
    const px =
      1.75 * (Number.parseFloat(getComputedStyle(document.documentElement).fontSize) || 16);
    // b(열 5~7)를 일곱 칸 앞으로: 미리 보기 시작이 열 -2(표 왼쪽 밖)다.
    await fireEvent.pointerDown(bar, { pointerId: 3, button: 0, buttons: 1, clientX: 400 });
    await fireEvent.pointerMove(bar, { pointerId: 3, buttons: 1, clientX: 400 - 7 * px });
    expect(gantt.querySelector('[data-gantt-link]')).toBeNull();
    expect(screen.getByTestId('gantt-links-hidden').textContent).toContain('관계 1개는');
    await fireEvent.keyDown(window, { key: 'Escape' });
    await fireEvent.pointerUp(bar, { pointerId: 3, clientX: 400 - 7 * px });
    expectLineOnBars(gantt);
    expect(screen.queryByTestId('gantt-links-hidden')).toBeNull();
    expect(update).not.toHaveBeenCalled();
  });

  it('hides the line while the predecessor end is dragged past the last column, instead of pinning it to the edge', async () => {
    // 표는 a 시작(10-20)부터 b 끝(10-27)까지 8열이다. a(열 0~2)를 여섯 칸 뒤로 끌면 끝이 열 8(표 오른쪽 밖)이다.
    // 막대는 표 안에 잘라 그리되, 선은 가장자리에 붙여 긋지 않고 보이지 않는 관계로 센다(#39와 같은 규칙).
    vi.spyOn(work, 'tasks').mockResolvedValue([design, build]);
    const gantt = await mount();
    const bar = q(gantt, '[data-gantt-bar="a"]');
    const px =
      1.75 * (Number.parseFloat(getComputedStyle(document.documentElement).fontSize) || 16);
    await fireEvent.pointerDown(bar, { pointerId: 4, button: 0, buttons: 1, clientX: 100 });
    await fireEvent.pointerMove(bar, { pointerId: 4, buttons: 1, clientX: 100 + 6 * px });
    expect(gantt.querySelector('[data-gantt-link]')).toBeNull();
    expect(screen.getByTestId('gantt-links-hidden').textContent).toContain('관계 1개는');
    // 막대는 마지막 열에서 끝나게 그린다(표 밖으로 넘치지 않음): 열 6~7.
    expect(bar.style.left).toBe(`calc(${String(6 * 1.75 + 0.125)}rem)`);
    expect(bar.style.width).toBe(`calc(${String(2 * 1.75 - 0.25)}rem)`);
    // 일부(열 6~7)는 기간 안이므로 숨기지 않는다.
    expect(bar.hasAttribute('data-gantt-offchart')).toBe(false);
    await fireEvent.keyDown(window, { key: 'Escape' });
    await fireEvent.pointerUp(bar, { pointerId: 4, clientX: 100 + 6 * px });
    expectLineOnBars(gantt);
  });

  it('keeps a bar dragged before the first column inside the chart while its line stays hidden', async () => {
    vi.spyOn(work, 'tasks').mockResolvedValue([design, build]);
    const gantt = await mount();
    const bar = q(gantt, '[data-gantt-bar="b"]');
    const px =
      1.75 * (Number.parseFloat(getComputedStyle(document.documentElement).fontSize) || 16);
    // b(열 5~7)를 일곱 칸 앞으로: 미리 보기 -2~0. 막대는 열 0부터 그리고 왼쪽 밖으로 넘치지 않는다.
    await fireEvent.pointerDown(bar, { pointerId: 5, button: 0, buttons: 1, clientX: 400 });
    await fireEvent.pointerMove(bar, { pointerId: 5, buttons: 1, clientX: 400 - 7 * px });
    expect(bar.style.left).toBe('calc(0.125rem)');
    expect(bar.style.width).toBe(`calc(${String(1.75 - 0.25)}rem)`);
    expect(bar.hasAttribute('data-gantt-offchart')).toBe(false);
    expect(gantt.querySelector('[data-gantt-link]')).toBeNull();
    await fireEvent.keyDown(window, { key: 'Escape' });
    await fireEvent.pointerUp(bar, { pointerId: 5, clientX: 400 - 7 * px });
  });

  it('hides a bar whose preview left the chart entirely instead of drawing it on the edge column', async () => {
    // 표는 10-20~10-27의 8열이다. b(열 5~7)를 아홉 칸 앞으로 끌면 미리 보기는 열 -4~-2(10-16~10-18)라 보이는 열이 없다.
    // 첫 열에 하루짜리 막대로 그리면 실제와 다른 날짜(10-20 하루)를 보이므로 숨긴다. 요소는 남아 끌기가 이어진다.
    vi.spyOn(work, 'tasks').mockResolvedValue([design, build]);
    const update = vi.spyOn(work, 'updateTask');
    const gantt = await mount();
    const bar = q(gantt, '[data-gantt-bar="b"]');
    const px =
      1.75 * (Number.parseFloat(getComputedStyle(document.documentElement).fontSize) || 16);
    await fireEvent.pointerDown(bar, { pointerId: 6, button: 0, buttons: 1, clientX: 400 });
    await fireEvent.pointerMove(bar, { pointerId: 6, buttons: 1, clientX: 400 - 9 * px });
    expect(bar.getAttribute('data-gantt-offchart')).toBe('true');
    expect(bar.classList.contains('invisible')).toBe(true);
    expect(screen.getByTestId('gantt-links-hidden').textContent).toContain('관계 1개는');
    // 같은 끌기로 다시 표 안(-2~0)으로 오면 같은 막대가 다시 보인다(요소를 빼지 않아 포인터를 놓치지 않음).
    await fireEvent.pointerMove(bar, { pointerId: 6, buttons: 1, clientX: 400 - 7 * px });
    expect(q(gantt, '[data-gantt-bar="b"]')).toBe(bar);
    expect(bar.hasAttribute('data-gantt-offchart')).toBe(false);
    expect(bar.classList.contains('invisible')).toBe(false);
    expect(bar.style.left).toBe('calc(0.125rem)');
    await fireEvent.keyDown(window, { key: 'Escape' });
    await fireEvent.pointerUp(bar, { pointerId: 6, clientX: 400 - 7 * px });
    expect(bar.hasAttribute('data-gantt-offchart')).toBe(false);
    expectLineOnBars(gantt);
    expect(update).not.toHaveBeenCalled();
  });

  it('hides a bar dragged entirely past the last column', async () => {
    // a(열 0~2)를 여덟 칸 뒤로: 미리 보기 열 8~10은 마지막 열(7) 뒤라 마지막 열에 한 칸짜리로 그리지 않는다.
    vi.spyOn(work, 'tasks').mockResolvedValue([design, build]);
    const update = vi.spyOn(work, 'updateTask');
    const gantt = await mount();
    const bar = q(gantt, '[data-gantt-bar="a"]');
    const px =
      1.75 * (Number.parseFloat(getComputedStyle(document.documentElement).fontSize) || 16);
    await fireEvent.pointerDown(bar, { pointerId: 7, button: 0, buttons: 1, clientX: 100 });
    await fireEvent.pointerMove(bar, { pointerId: 7, buttons: 1, clientX: 100 + 8 * px });
    expect(bar.getAttribute('data-gantt-offchart')).toBe('true');
    expect(bar.classList.contains('invisible')).toBe(true);
    await fireEvent.keyDown(window, { key: 'Escape' });
    await fireEvent.pointerUp(bar, { pointerId: 7, clientX: 100 + 8 * px });
    expect(bar.hasAttribute('data-gantt-offchart')).toBe(false);
    expect(update).not.toHaveBeenCalled();
  });

  it('hides a milestone dragged past the last column instead of leaving it on that column', async () => {
    // 마일스톤 m(마감 10-27)은 마지막 열 7의 ◆다. 한 칸 뒤로 끌면 10-28이라 표 밖인데, 전에는 ◆가 그 자리에 그대로 남아
    // 아무것도 움직이지 않은 것처럼 보였다(후속이 없으면 놓자마자 묻지 않고 저장된다).
    const release = { ...task('m', '출시', null, '2026-10-27'), milestone: true };
    vi.spyOn(work, 'tasks').mockResolvedValue([design, release]);
    const update = vi.spyOn(work, 'updateTask');
    const gantt = await mount();
    const diamond = q(gantt, '[data-gantt-bar="m"]');
    expect(diamond.hasAttribute('data-gantt-offchart')).toBe(false);
    const px =
      1.75 * (Number.parseFloat(getComputedStyle(document.documentElement).fontSize) || 16);
    await fireEvent.pointerDown(diamond, { pointerId: 8, button: 0, buttons: 1, clientX: 400 });
    await fireEvent.pointerMove(diamond, { pointerId: 8, buttons: 1, clientX: 400 + px });
    expect(diamond.getAttribute('data-gantt-offchart')).toBe('true');
    expect(diamond.classList.contains('invisible')).toBe(true);
    await fireEvent.keyDown(window, { key: 'Escape' });
    await fireEvent.pointerUp(diamond, { pointerId: 8, clientX: 400 + px });
    expect(diamond.hasAttribute('data-gantt-offchart')).toBe(false);
    expect(diamond.classList.contains('invisible')).toBe(false);
    expect(update).not.toHaveBeenCalled();
  });

  it('matches the bars after an approved preview, a cancelled one, and a failed save', async () => {
    const movedA = task('a', '설계', '2026-10-20', '2026-10-23', [], 2);
    const movedB = task('b', '구현', '2026-10-26', '2026-10-28', ['a'], 2);
    const read = vi
      .spyOn(work, 'tasks')
      .mockResolvedValueOnce([design, build]) // 처음
      .mockResolvedValueOnce([movedA, movedB]) // 승인 뒤
      .mockResolvedValue([movedA, movedB]);
    vi.spyOn(work, 'task').mockResolvedValue(detail(design));
    vi.spyOn(work, 'previewSchedule').mockResolvedValue({
      shifts: [shift],
      undated: [],
      blocked: null,
    });
    const update = vi
      .spyOn(work, 'updateTask')
      .mockResolvedValueOnce({ kind: 'updated', task: detail(movedA) });
    const gantt = await mount();
    expectLineOnBars(gantt);
    // 승인: 마감 하루 늦추기 → 후속도 하루 → 다시 읽은 막대와 선이 같다.
    await fireEvent.keyDown(q(gantt, '[data-task-card="a"]'), {
      key: 'ArrowRight',
      altKey: true,
      shiftKey: true,
    });
    let dialog = await screen.findByRole('dialog');
    expectLineOnBars(gantt); // 묻는 동안은 그대로(저장 전)
    await fireEvent.click(within(dialog).getByRole('button', { name: '후속 업무도 옮겨 저장' }));
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(1);
    });
    await waitFor(() => {
      expect(barEdges(q(gantt, '[data-gantt-bar="b"]')).left).toBeCloseTo(6 * 1.75 + 0.125, 3);
    });
    expectLineOnBars(gantt);
    // 취소: 저장하지 않고 다시 읽음 → 그대로 같다.
    vi.spyOn(work, 'task').mockResolvedValue(detail(movedA));
    await fireEvent.keyDown(q(gantt, '[data-task-card="a"]'), { key: 'ArrowRight', altKey: true });
    dialog = await screen.findByRole('dialog');
    await fireEvent.click(within(dialog).getByRole('button', { name: '취소' }));
    await waitFor(() => {
      expect(screen.queryByRole('dialog')).toBeNull();
    });
    expectLineOnBars(gantt);
    // 서버 실패: 저장되지 않고 다시 읽음 → 같다.
    update.mockRejectedValueOnce(new ApiError(500, 'internal', 'boom'));
    vi.spyOn(work, 'previewSchedule').mockResolvedValue({ shifts: [], undated: [], blocked: null });
    await fireEvent.keyDown(q(gantt, '[data-task-card="a"]'), { key: 'ArrowRight', altKey: true });
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(2);
    });
    await waitFor(() => {
      expect(read.mock.calls.length).toBeGreaterThanOrEqual(3);
    });
    expectLineOnBars(gantt);
  });

  it('says when some relations cannot be drawn, without guessing an end', async () => {
    // 'c'는 날짜가 없어 막대가 없고, 'b'의 다른 선행 'zz'는 이 목록에 없다(볼 수 없거나 거른 업무).
    vi.spyOn(work, 'tasks').mockResolvedValue([
      design,
      task('b', '구현', '2026-10-25', '2026-10-27', ['a', 'zz']),
      task('c', '정리', null, null, ['a']),
    ]);
    const gantt = await mount();
    expect(
      [...gantt.querySelectorAll('[data-gantt-link]')].map((g) =>
        g.getAttribute('data-gantt-link'),
      ),
    ).toEqual(['a>b']);
    expect(screen.getByTestId('gantt-links-hidden').textContent).toContain(
      '관계 2개는 한쪽 연결 지점을 이 간트에 놓을 수 없어',
    );
  });

  it('draws no line when the filter leaves only one end', async () => {
    window.sessionStorage.setItem(
      org.key('board:i1'),
      JSON.stringify({ view: 'gantt', sort: 'position', filter: { text: '구현' } }),
    );
    vi.spyOn(work, 'tasks').mockResolvedValue([design, build]);
    const gantt = await mount();
    expect(gantt.querySelector('[data-testid="gantt-links"]')).toBeNull();
    expect(screen.getByTestId('gantt-links-hidden').textContent).toContain('관계 1개는');
  });

  it('uses the singular and plural in English', async () => {
    await setLocale('en', { reload: false });
    vi.spyOn(work, 'tasks').mockResolvedValue([
      design,
      task('b', '구현', '2026-10-25', '2026-10-27', ['a', 'zz']),
    ]);
    render(TaskBoard, { props: { item, workspaces: [], openTaskId: undefined } });
    expect((await screen.findByTestId('gantt-links-hidden')).textContent).toContain(
      '1 dependency is not drawn because one of its ends cannot be placed',
    );
  });
});
