import type { Meta, StoryObj } from '@storybook/svelte';
import { expect, fn, userEvent, waitFor, within } from 'storybook/test';

import RoleSelect from '$lib/features/members/RoleSelect.svelte';

import { pressEscape } from '../fixtures/keys';

// 선택 입력의 대표 사용처: 멤버 역할 선택(공통 Select를 그대로 쓴다).
const meta = {
  title: 'Members/RoleSelect',
  component: RoleSelect,
  args: { value: 'writer', label: '김연구의 역할', onchange: fn() },
} satisfies Meta<typeof RoleSelect>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const Disabled: Story = {
  args: { disabled: true },
  play: async ({ canvasElement }) => {
    await expect(
      within(canvasElement).getByRole('button', { name: '김연구의 역할: 작성자' }),
    ).toBeDisabled();
  },
};

/** 키보드: Tab으로 닿고 Enter로 열어 아래 화살표·Enter로 고른다. Escape는 고르지 않고 닫는다. */
export const KeyboardChoose: Story = {
  play: async ({ canvasElement, args }) => {
    const trigger = within(canvasElement).getByRole('button', { name: '김연구의 역할: 작성자' });
    await userEvent.tab();
    await expect(trigger).toHaveFocus();
    await userEvent.keyboard('{Enter}');
    // 목록이 다 열리면 지금 값이 강조된다(aria-activedescendant). 그 뒤에 Escape 처리도 준비된다.
    await waitFor(() => expect(trigger).toHaveAttribute('aria-activedescendant'));
    await pressEscape();
    await waitFor(() => expect(trigger).toHaveAttribute('aria-expanded', 'false'));
    await expect(args.onchange).not.toHaveBeenCalled();
    await expect(trigger).toHaveFocus();
    await userEvent.keyboard('{Enter}');
    await waitFor(() => expect(trigger).toHaveAttribute('aria-activedescendant'));
    // 지금 값(작성자)에서 시작해 한 칸 아래(독자)를 고른다.
    await userEvent.keyboard('{ArrowDown}{Enter}');
    await expect(args.onchange).toHaveBeenCalledWith('reader');
    await waitFor(() => expect(trigger).toHaveFocus());
  },
};
