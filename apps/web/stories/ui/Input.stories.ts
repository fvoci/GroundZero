import type { Meta, StoryObj } from '@storybook/svelte';
import { expect, userEvent, within } from 'storybook/test';

import { Input } from '$lib/ui/input';

import { EMOJI, LONG_KO, NO_SPACE, NUMBERS } from '../fixtures/text';

// 공통 한 줄 입력(src/lib/ui/input). 제품 화면은 Label이나 aria-label로 이름을 붙인다.
const meta = {
  title: 'UI/Input',
  component: Input,
  args: { 'aria-label': '업무 제목', value: '' },
} satisfies Meta<typeof Input>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const Placeholder: Story = { args: { placeholder: '제목을 입력하세요' } };

export const Disabled: Story = {
  args: { value: '바꿀 수 없는 값', disabled: true },
  play: async ({ canvasElement }) => {
    await expect(within(canvasElement).getByRole('textbox', { name: '업무 제목' })).toBeDisabled();
  },
};

/** 읽기 전용: 초점은 받지만 고칠 수 없다. */
export const ReadOnly: Story = {
  args: { value: '읽기 전용 값', readonly: true },
  play: async ({ canvasElement }) => {
    const box = within(canvasElement).getByRole('textbox', { name: '업무 제목' });
    await userEvent.click(box);
    await userEvent.keyboard('추가');
    await expect(box).toHaveValue('읽기 전용 값');
  },
};

/** 입력 오류(aria-invalid). 오류 글은 부르는 화면이 칸 옆에 둔다. */
export const Invalid: Story = { args: { value: '2026-13-40', 'aria-invalid': true } };

/** Tab으로 초점을 받고 입력한다. */
export const KeyboardFocus: Story = {
  play: async ({ canvasElement }) => {
    const box = within(canvasElement).getByRole('textbox', { name: '업무 제목' });
    await userEvent.tab();
    await expect(box).toHaveFocus();
    await userEvent.keyboard('주간 회의 준비');
    await expect(box).toHaveValue('주간 회의 준비');
  },
};

export const LongKorean: Story = { args: { value: LONG_KO } };

export const NoSpaceString: Story = { args: { value: NO_SPACE } };

export const NumbersAndEmoji: Story = { args: { value: `${NUMBERS} ${EMOJI}` } };
