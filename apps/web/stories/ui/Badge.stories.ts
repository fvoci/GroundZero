import type { Meta, StoryObj } from '@storybook/svelte';

import { Badge } from '$lib/ui/badge';

import { text } from '../fixtures/text';

// 상태 배지(src/lib/ui/badge). 색만이 아니라 이름으로 구분한다(D-66). 부품은 기본 스토리 하나만 둔다(D-108 개정).
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
