import type { Meta, StoryObj } from '@storybook/svelte';
import { expect, fn, userEvent, within } from 'storybook/test';

import type { TaskView } from '$lib/api/client';
import GanttChart from '$lib/features/tasks/GanttChart.svelte';

import { fixClock } from '../fixtures/clock';
import { taskViews } from '../fixtures/synthetic';

// 간트(src/lib/features/tasks/GanttChart.svelte): 긴 이름·연결선·기간 경계·키보드 일정 옮기기. 업무는 개발 seed의
// "UI 상태 예제" 업무(긴 한국어 제목·URL 제목·이모지·마일스톤 포함)이고, seed에 없는 선행 관계만 여기서 더한다.
// 일정 저장(후속 업무 영향 미리 보기·승인)은 부르는 쪽(TaskBoard)이 맡으므로 이 story는 `reschedule`을 바꿔 끼운다.
const BASE = taskViews();

function at(index: number): TaskView {
  const task = BASE[index];
  if (task === undefined) throw new RangeError(`no task ${String(index)}`);
  return task;
}

/**
 * 끝나야 시작(FS)·시작해야 시작(SS) 관계: 장비 점검 → 예산 정산 → 회의 준비, 주간 회의 ⇒(SS+2) 긴 제목 → 중간 발표.
 * 모두 날짜가 관계를 지키는 상태다.
 */
function linked(): TaskView[] {
  const fs = (from: number) => ({ task_id: at(from).id, kind: 'FS' as const, lag_days: 0 });
  const preds: Record<number, TaskView['predecessors']> = {
    1: [{ task_id: at(0).id, kind: 'SS', lag_days: 2 }],
    3: [fs(4)],
    4: [fs(5)],
    6: [fs(1)],
  };
  return BASE.map((t, i) => ({ ...t, predecessors: preds[i] ?? [] }));
}

const meta = {
  title: 'Tasks/GanttChart',
  component: GanttChart,
  args: { tasks: linked(), onopen: fn(), reschedule: fn(() => Promise.resolve()) },
  beforeEach: () => fixClock(),
} satisfies Meta<typeof GanttChart>;

export default meta;
type Story = StoryObj<typeof meta>;

/** 쓸 수 있음: 막대·◆·연결선과 끌기·키보드 안내가 보인다. 오늘(2026-10-20) 열이 강조된다. */
export const Editable: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getAllByRole('button', { name: /^DEVU-\d+ / })).toHaveLength(7);
    await expect(
      canvas.getByTestId('gantt-links').querySelectorAll('[data-gantt-link]'),
    ).toHaveLength(4);
    await expect(canvas.getByRole('button', { name: '오늘' })).toBeEnabled();
  },
};

/** 읽기 전용(일정 바꾸기 없음): 줄 단추는 열기만 하고 단축키를 알리지 않는다. */
export const ReadOnly: Story = {
  args: { reschedule: undefined },
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);
    const row = canvas.getByRole('button', { name: /^DEVU-2 주간 회의/ });
    await expect(row).not.toHaveAttribute('aria-keyshortcuts');
    await userEvent.click(row);
    await expect(args.onopen).toHaveBeenCalledWith(at(0).id);
  },
};

/** 긴 이름(한국어 문장·공백 없는 URL)은 줄 이름 칸에서 말줄임으로 끝나고 가리키면 전체가 보인다(title). */
export const LongNames: Story = {
  args: { tasks: [at(1), at(2), at(3)] },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    for (const t of [at(1), at(2), at(3)]) {
      await expect(
        canvas.getByRole('button', { name: new RegExp(`^${t.reference ?? ''} `) }),
      ).toHaveAttribute('title', `${t.reference ?? ''} ${t.title}`);
    }
  },
};

/** 375px 휴대 폭: 이름 칸이 줄고(최소 8rem) 날짜 칸은 가로 스크롤 안에 남는다. 주 머리글은 짧게 쓴다. */
export const Narrow: Story = {
  globals: { viewport: { value: 'narrow375', isRotated: false } },
};

/**
 * 기간 경계: 표시 기간(최대 400일)보다 늦게 시작하는 업무는 막대 대신 개수로 알리고, 그 경계를 넘어 끝나는 업무는
 * 끝이 잘린다(잘린 쪽 연결선은 숨기고 개수를 알린다). 날짜 없는 업무도 개수로 알린다.
 */
export const RangeBoundaries: Story = {
  args: {
    tasks: [
      { ...at(0), start_date: '2026-10-18', due_date: '2026-10-22' },
      { ...at(1), start_date: '2026-11-01', due_date: '2028-01-31' },
      {
        ...at(6),
        start_date: null,
        due_date: '2028-03-01',
        predecessors: [{ task_id: at(1).id, kind: 'FS', lag_days: 0 }],
      },
      { ...at(2), start_date: null, due_date: null },
    ],
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByText(/400일 뒤에 시작하는 업무 1개/)).toBeInTheDocument();
    await expect(canvas.getByText(/날짜 없는 업무 1개/)).toBeInTheDocument();
  },
};

/** 같은 제목의 업무가 많아도 줄마다 번호가 달라 구별된다(40줄, 세로 스크롤은 화면이 맡는다). */
export const ManySameName: Story = {
  args: {
    tasks: Array.from({ length: 40 }, (_, i) => ({
      ...at(0),
      id: `${at(0).id}-${String(i)}`,
      reference: `DEVU-${String(100 + i)}`,
      start_date: `2026-10-${String(10 + (i % 15)).padStart(2, '0')}`,
      due_date: `2026-10-${String(12 + (i % 15)).padStart(2, '0')}`,
    })),
  },
};

/** 날짜 있는 업무가 없으면 빈 안내만 보인다. */
export const NoDatedTasks: Story = {
  args: { tasks: BASE.map((t) => ({ ...t, start_date: null, due_date: null })) },
  play: async ({ canvasElement }) => {
    await expect(
      within(canvasElement).getByText(/날짜가 있는 업무가 없습니다/),
    ).toBeInTheDocument();
  },
};

/** 키보드: 줄 단추에서 Alt+→는 하루 뒤로, Alt+Shift+→는 마감만 하루 뒤로 옮겨 달라고 부른다. */
export const KeyboardReschedule: Story = {
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);
    const row = canvas.getByRole('button', { name: /^DEVU-2 주간 회의/ });
    row.focus();
    await userEvent.keyboard('{Alt>}{ArrowRight}{/Alt}');
    await expect(args.reschedule).toHaveBeenLastCalledWith(at(0), {
      start_date: '2026-10-19',
      due_date: '2026-10-23',
    });
    await userEvent.keyboard('{Alt>}{Shift>}{ArrowRight}{/Shift}{/Alt}');
    await expect(args.reschedule).toHaveBeenLastCalledWith(at(0), {
      start_date: '2026-10-18',
      due_date: '2026-10-23',
    });
  },
};

/** 저장 응답 지연: 끝날 때까지 막대는 옮긴 자리를 보이고, 그사이 다른 옮기기는 받지 않는다. */
export const SavePending: Story = {
  args: { reschedule: fn(() => new Promise<void>(() => undefined)) },
  play: async ({ canvasElement, args }) => {
    const row = within(canvasElement).getByRole('button', { name: /^DEVU-2 주간 회의/ });
    row.focus();
    await userEvent.keyboard('{Alt>}{ArrowRight}{/Alt}');
    await userEvent.keyboard('{Alt>}{ArrowRight}{/Alt}');
    await expect(args.reschedule).toHaveBeenCalledTimes(1);
  },
};
