import type { Meta, StoryObj } from '@storybook/svelte';
import { expect, fn, userEvent, waitFor, within } from 'storybook/test';

import type { SummaryView } from '$lib/api/client';
import DocumentTree from '$lib/features/documents/DocumentTree.svelte';

import { EMOJI, LONG_KO, NO_SPACE } from '../fixtures/text';

// 문서 트리(src/lib/features/documents/DocumentTree.svelte): WAI-ARIA 트리 키보드 조작, 쓰기 권한에 따른 메뉴,
// 빈 목록, 옮기기 실패 알림.
function doc(
  id: string,
  title: string,
  parent: string | null,
  reference: string | null = null,
): SummaryView {
  return {
    id,
    reference,
    title,
    revision: 1,
    updated_at: '2026-10-20T01:30:00Z',
    parent_id: parent,
  };
}

const DOCS: SummaryView[] = [
  doc('d-01', '연구실 안내', null, 'W-1'),
  doc('d-02', '세미나 발표 순서', 'd-01', 'W-2'),
  doc('d-03', '회의록 2026-10', 'd-01', 'W-3'),
  doc('d-04', '참고 문헌 목록', null, 'W-4'),
];

const meta = {
  title: 'Documents/DocumentTree',
  component: DocumentTree,
  args: {
    docs: DOCS,
    label: '위키 문서',
    hrefFor: (id: string) => `#/documents/${id}`,
    navigate: fn(),
    canWrite: true,
    empty: '아직 문서가 없습니다.',
    onmove: fn(() => Promise.resolve()),
    oncreatechild: fn(() => Promise.resolve()),
  },
} satisfies Meta<typeof DocumentTree>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Writable: Story = {};

/** 읽기 권한만: 항목 메뉴와 순서 바꾸기 단축키가 없다. */
export const ReadOnly: Story = {
  args: { canWrite: false },
  play: async ({ canvasElement }) => {
    await expect(within(canvasElement).queryByRole('button')).toBeNull();
  },
};

export const Empty: Story = { args: { docs: [] } };

export const LongTitles: Story = {
  args: {
    docs: [
      doc('d-11', LONG_KO, null, 'W-11'),
      doc('d-12', NO_SPACE, 'd-11', 'W-12'),
      doc('d-13', `발표 준비 ${EMOJI}`, 'd-12', 'W-13'),
    ],
  },
};

/** 키보드: Tab으로 트리에 들어가 아래 화살표로 옮기고, 왼쪽으로 접고, Enter로 연다. */
export const KeyboardNavigate: Story = {
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);
    await userEvent.tab();
    await expect(canvas.getByRole('treeitem', { name: 'W-1 연구실 안내' })).toHaveFocus();
    await userEvent.keyboard('{ArrowDown}');
    await expect(canvas.getByRole('treeitem', { name: 'W-2 세미나 발표 순서' })).toHaveFocus();
    await userEvent.keyboard('{ArrowLeft}');
    await expect(canvas.getByRole('treeitem', { name: 'W-1 연구실 안내' })).toHaveFocus();
    await userEvent.keyboard('{ArrowLeft}');
    await waitFor(() =>
      expect(canvas.queryByRole('treeitem', { name: 'W-2 세미나 발표 순서' })).toBeNull(),
    );
    await userEvent.keyboard('{Enter}');
    await expect(args.navigate).toHaveBeenCalledWith('#/documents/d-01');
  },
};

/** 옮기기 실패(Alt+아래 화살표): 트리는 그대로 두고 알림을 남긴다. */
export const MoveFailed: Story = {
  args: { onmove: fn(() => Promise.reject(new Error('move failed'))) },
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);
    await userEvent.tab();
    await userEvent.keyboard('{Alt>}{ArrowDown}{/Alt}');
    await expect(args.onmove).toHaveBeenCalledTimes(1);
    await expect(await canvas.findByRole('alert')).toHaveTextContent('문서를 옮기지 못했습니다.');
  },
};
