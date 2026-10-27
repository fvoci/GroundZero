import type { Meta, StoryObj } from '@storybook/svelte';

import { Input } from '$lib/ui/input';

// 공통 한 줄 입력(src/lib/ui/input). 제품 화면은 Label이나 aria-label로 이름을 붙인다. 부품은 기본 스토리 하나만 둔다(D-108 개정).
const meta = {
  title: 'UI/Input',
  component: Input,
  args: { 'aria-label': '업무 제목', value: '' },
} satisfies Meta<typeof Input>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = { args: { placeholder: '제목을 입력하세요' } };
