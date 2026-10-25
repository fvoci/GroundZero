import type { Meta, StoryObj } from '@storybook/svelte';
import { expect, fn, userEvent, waitFor, within } from 'storybook/test';

import DocumentTree from '$lib/features/documents/DocumentTree.svelte';

import { DOCS } from '../fixtures/synthetic';

// 문서 트리(src/lib/features/documents/DocumentTree.svelte): WAI-ARIA 트리 키보드 조작, 쓰기 권한에 따른 메뉴,
// 빈 목록, 옮기기 실패 알림. 문서는 개발 seed의 "UI 상태 예제" 위키와 같다(긴 제목·공백 없는 제목·이모지·숫자 포함).
const LONG_SUBTREE = new Set(['doc-long-title', 'doc-no-space', 'doc-emoji']);

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

/** 긴 제목은 칸 폭에서 말줄임으로 끝나지만, 가리키면 전체 제목이 보인다(title). */
export const LongTitles: Story = {
  args: { docs: DOCS.filter((d) => LONG_SUBTREE.has(d.id)) },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    for (const d of DOCS.filter((doc) => LONG_SUBTREE.has(doc.id))) {
      await expect(canvas.getByText(d.title)).toHaveAttribute('title', d.title);
    }
  },
};

/** 키보드: Tab으로 트리에 들어가 아래 화살표로 옮기고, 왼쪽으로 접고, Enter로 연다. */
export const KeyboardNavigate: Story = {
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);
    await userEvent.tab();
    await expect(canvas.getByRole('treeitem', { name: 'WIKI-1 연구실 안내' })).toHaveFocus();
    await userEvent.keyboard('{ArrowDown}');
    await expect(canvas.getByRole('treeitem', { name: 'WIKI-2 세미나 발표 순서' })).toHaveFocus();
    await userEvent.keyboard('{ArrowLeft}');
    await expect(canvas.getByRole('treeitem', { name: 'WIKI-1 연구실 안내' })).toHaveFocus();
    await userEvent.keyboard('{ArrowLeft}');
    await waitFor(() =>
      expect(canvas.queryByRole('treeitem', { name: 'WIKI-2 세미나 발표 순서' })).toBeNull(),
    );
    await userEvent.keyboard('{Enter}');
    await expect(args.navigate).toHaveBeenCalledWith('#/documents/doc-guide');
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

export const BrandLight: Story = { globals: { theme: 'light', viewport: { value: 'desktop' } } };
export const BrandDark: Story = { globals: { theme: 'dark', viewport: { value: 'desktop' } } };
export const BrandNarrowLargeText: Story = {
  globals: { theme: 'dark', viewport: { value: 'narrow375' }, fontScale: '200' },
};
