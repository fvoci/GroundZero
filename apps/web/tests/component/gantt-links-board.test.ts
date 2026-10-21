import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { ItemView, ShiftProposalView, TaskDetailView, TaskView } from '$lib/api/client';
import { api, ApiError } from '$lib/api/client';
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
    predecessor_ids: preds,
    series: null,
    occurrence_date: null,
    assignees: [],
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
    window.sessionStorage.setItem('fvoci:board:i1', JSON.stringify({ view: 'gantt' }));
    vi.spyOn(api, 'savedViews').mockResolvedValue([]);
    vi.spyOn(api, 'syncRecurrences').mockResolvedValue(0);
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
    vi.spyOn(api, 'tasks').mockResolvedValue([design, build]);
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
    expect(screen.getByText(/선은 선행 업무의 끝에서 후속 업무의 시작으로/)).toBeTruthy();
    // 줄을 눌러 열 수 있다(연결선이 가로막지 않음).
    await fireEvent.click(q(gantt, '[data-task-card="b"]'));
    await waitFor(() => {
      expect(navigate).toHaveBeenCalledWith('/w/ws-1/i/i1/t/b');
    });
  });

  it('makes the lines of a focused or hovered row bold, by width and not by colour alone', async () => {
    vi.spyOn(api, 'tasks').mockResolvedValue([
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
  });

  it('follows the bar while dragging, goes back on Escape, and sends nothing for the line itself', async () => {
    vi.spyOn(api, 'tasks').mockResolvedValue([design, build]);
    const update = vi.spyOn(api, 'updateTask');
    const preview = vi.spyOn(api, 'previewSchedule');
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

  it('matches the bars after an approved preview, a cancelled one, and a failed save', async () => {
    const movedA = task('a', '설계', '2026-10-20', '2026-10-23', [], 2);
    const movedB = task('b', '구현', '2026-10-26', '2026-10-28', ['a'], 2);
    const read = vi
      .spyOn(api, 'tasks')
      .mockResolvedValueOnce([design, build]) // 처음
      .mockResolvedValueOnce([movedA, movedB]) // 승인 뒤
      .mockResolvedValue([movedA, movedB]);
    vi.spyOn(api, 'task').mockResolvedValue(detail(design));
    vi.spyOn(api, 'previewSchedule').mockResolvedValue([shift]);
    const update = vi
      .spyOn(api, 'updateTask')
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
    vi.spyOn(api, 'task').mockResolvedValue(detail(movedA));
    await fireEvent.keyDown(q(gantt, '[data-task-card="a"]'), { key: 'ArrowRight', altKey: true });
    dialog = await screen.findByRole('dialog');
    await fireEvent.click(within(dialog).getByRole('button', { name: '취소' }));
    await waitFor(() => {
      expect(screen.queryByRole('dialog')).toBeNull();
    });
    expectLineOnBars(gantt);
    // 서버 실패: 저장되지 않고 다시 읽음 → 같다.
    update.mockRejectedValueOnce(new ApiError(500, 'internal', 'boom'));
    vi.spyOn(api, 'previewSchedule').mockResolvedValue([]);
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
    vi.spyOn(api, 'tasks').mockResolvedValue([
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
      '관계 2개는 한쪽 업무가 이 간트에 없어',
    );
  });

  it('draws no line when the filter leaves only one end', async () => {
    window.sessionStorage.setItem(
      'fvoci:board:i1',
      JSON.stringify({ view: 'gantt', sort: 'position', filter: { text: '구현' } }),
    );
    vi.spyOn(api, 'tasks').mockResolvedValue([design, build]);
    const gantt = await mount();
    expect(gantt.querySelector('[data-testid="gantt-links"]')).toBeNull();
    expect(screen.getByTestId('gantt-links-hidden').textContent).toContain('관계 1개는');
  });
});
