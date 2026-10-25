import type { Meta, StoryObj } from '@storybook/svelte';
import { expect, fn, userEvent, within } from 'storybook/test';

import { Button } from '$lib/ui/button';

import { EMOJI, LONG_KO, NO_SPACE, NUMBERS, text } from '../fixtures/text';

// 공통 버튼(src/lib/ui/button). 제품 전체에서 가장 많이 쓰는 조작 요소다.
const meta = {
  title: 'UI/Button',
  component: Button,
  args: { children: text('저장'), onclick: fn() },
  argTypes: {
    variant: {
      control: 'select',
      options: ['default', 'outline', 'secondary', 'ghost', 'destructive', 'link'],
    },
    size: {
      control: 'select',
      options: ['default', 'xs', 'sm', 'lg', 'icon', 'icon-xs', 'icon-sm', 'icon-lg'],
    },
  },
} satisfies Meta<typeof Button>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const Outline: Story = { args: { variant: 'outline', children: text('취소') } };

export const Ghost: Story = { args: { variant: 'ghost', size: 'sm', children: text('고치기') } };

export const Destructive: Story = {
  args: { variant: 'destructive', children: text('변경 버리고 닫기') },
};

export const Disabled: Story = {
  args: { disabled: true },
  play: async ({ canvasElement, args }) => {
    const button = within(canvasElement).getByRole('button', { name: '저장' });
    await expect(button).toBeDisabled();
    await userEvent.click(button, { pointerEventsCheck: 0 });
    await expect(args.onclick).not.toHaveBeenCalled();
  },
};

/** Tab으로 초점이 오면 초점 고리가 보이고, Enter로 누른다. */
export const KeyboardFocus: Story = {
  play: async ({ canvasElement, args }) => {
    const button = within(canvasElement).getByRole('button', { name: '저장' });
    await userEvent.tab();
    await expect(button).toHaveFocus();
    await userEvent.keyboard('{Enter}');
    await expect(args.onclick).toHaveBeenCalledTimes(1);
  },
};

/** 링크 모양(href). 비활성이면 href를 빼서 이동하지 않는다. */
export const AsLink: Story = {
  args: { href: '#/documents', variant: 'link', children: text('문서로 가기') },
};

export const LongKorean: Story = { args: { children: text(LONG_KO) } };

export const NoSpaceString: Story = { args: { variant: 'outline', children: text(NO_SPACE) } };

export const NumbersAndEmoji: Story = { args: { children: text(`${NUMBERS} ${EMOJI}`) } };

export const BrandLight: Story = { globals: { theme: 'light', viewport: { value: 'desktop' } } };
export const BrandDark: Story = { globals: { theme: 'dark', viewport: { value: 'desktop' } } };
export const BrandNarrowLargeText: Story = {
  globals: { theme: 'dark', viewport: { value: 'narrow375' }, fontScale: '200' },
};
