import type { Meta, StoryObj } from '@storybook/svelte';
import { fn } from 'storybook/test';

import { Button } from '$lib/ui/button';

import { text } from '../fixtures/text';

// 공통 버튼(src/lib/ui/button). 제품 전체에서 가장 많이 쓰는 조작 요소다. 부품은 기본 스토리 하나만 둔다(D-108 개정).
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
