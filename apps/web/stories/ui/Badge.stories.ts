import type { Meta, StoryObj } from '@storybook/svelte';

import { Badge } from '$lib/ui/badge';

import { EMOJI, LONG_KO, NO_SPACE, NUMBERS, text } from '../fixtures/text';

// 상태 배지(src/lib/ui/badge). 색만이 아니라 이름으로 구분한다(D-66).
const meta = {
  title: 'UI/Badge',
  component: Badge,
  args: { children: text('진행 중') },
  argTypes: {
    variant: {
      control: 'select',
      options: ['default', 'secondary', 'destructive', 'outline', 'ghost', 'link'],
    },
  },
} satisfies Meta<typeof Badge>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const Secondary: Story = { args: { variant: 'secondary', children: text('보관됨') } };

export const Destructive: Story = { args: { variant: 'destructive', children: text('기한 지남') } };

export const Outline: Story = { args: { variant: 'outline', children: text('읽기 전용') } };

/** 배지는 한 줄로 그리고 넘치면 자른다(whitespace-nowrap·overflow-hidden). 좁은 화면에서 잘림을 확인한다. */
export const LongKorean: Story = { args: { variant: 'secondary', children: text(LONG_KO) } };

export const NoSpaceString: Story = { args: { variant: 'outline', children: text(NO_SPACE) } };

export const NumbersAndEmoji: Story = { args: { children: text(`${NUMBERS} ${EMOJI}`) } };

export const BrandLight: Story = { globals: { theme: 'light', viewport: { value: 'desktop' } } };
export const BrandDark: Story = { globals: { theme: 'dark', viewport: { value: 'desktop' } } };
export const BrandNarrowLargeText: Story = {
  globals: { theme: 'dark', viewport: { value: 'narrow375' }, fontScale: '200' },
};
