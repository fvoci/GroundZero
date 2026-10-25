import type { Meta, StoryObj } from '@storybook/svelte';
import { expect, fn, screen, userEvent, waitFor, within } from 'storybook/test';

import DiscardHost from '../fixtures/DiscardHost.svelte';
import { pressEscape } from '../fixtures/keys';

// 미저장 입력을 버릴지 묻는 확인(src/lib/features/common/DiscardChangesConfirm.svelte). 확인 자체의 상태와
// 계속 편집(Escape 포함)의 입력 보존·초점 복귀를 본다. 언제 묻는지는 부르는 화면(업무·일정 대화상자)의 책임이다.
const meta = {
  title: 'Common/DiscardChangesConfirm',
  component: DiscardHost,
  args: { initial: '주간 회의 준비', ondiscard: fn() },
} satisfies Meta<typeof DiscardHost>;

export default meta;
type Story = StoryObj<typeof meta>;

async function editAndAsk(canvasElement: HTMLElement): Promise<HTMLElement> {
  const canvas = within(canvasElement);
  const box = canvas.getByRole('textbox', { name: '업무 제목' });
  await userEvent.type(box, ' (고침)');
  await userEvent.click(canvas.getByRole('button', { name: '닫기' }));
  return screen.findByRole('alertdialog', { name: '저장하지 않은 변경이 있습니다' });
}

/**
 * 입력을 바꾼 뒤 닫으려 할 때 뜬 확인. 처음 초점은 확인 상자 자체(Bits UI 기본값)라 제목·설명이 먼저 읽히고,
 * Tab의 첫 단추는 안전한 쪽(계속 편집)이다.
 */
export const Asking: Story = {
  play: async ({ canvasElement }) => {
    const dialog = await editAndAsk(canvasElement);
    await waitFor(() => expect(dialog).toHaveFocus());
    await userEvent.tab();
    await expect(within(dialog).getByRole('button', { name: '계속 편집' })).toHaveFocus();
  },
};

/** Escape는 계속 편집과 같다: 확인만 닫고 입력과 초점을 그대로 돌려준다. */
export const EscapeKeepsInput: Story = {
  play: async ({ canvasElement, args }) => {
    const dialog = await editAndAsk(canvasElement);
    await waitFor(() => expect(dialog).toHaveFocus());
    await pressEscape();
    await waitFor(() => expect(dialog).toHaveAttribute('data-state', 'closed'));
    const box = within(canvasElement).getByRole('textbox', { name: '업무 제목' });
    await expect(box).toHaveValue('주간 회의 준비 (고침)');
    await waitFor(() => expect(box).toHaveFocus());
    await expect(args.ondiscard).not.toHaveBeenCalled();
  },
};

/** 버리기: Tab으로 버리기 단추에 닿아 Enter로 고른다. */
export const Discard: Story = {
  play: async ({ canvasElement, args }) => {
    const dialog = await editAndAsk(canvasElement);
    await waitFor(() => expect(dialog).toHaveFocus());
    await userEvent.tab();
    await userEvent.tab();
    await expect(within(dialog).getByRole('button', { name: '변경 버리고 닫기' })).toHaveFocus();
    await userEvent.keyboard('{Enter}');
    await expect(args.ondiscard).toHaveBeenCalledTimes(1);
    await expect(within(canvasElement).getByRole('textbox', { name: '업무 제목' })).toHaveValue(
      '주간 회의 준비',
    );
  },
};

/** 대화상자 안에서 다른 업무를 열 때의 문구. */
export const OpenAnother: Story = {
  args: {
    description: '다른 업무를 열면 이 업무에서 고친 내용이 사라집니다.',
    confirmLabel: '변경 버리고 열기',
  },
  play: async ({ canvasElement }) => {
    await editAndAsk(canvasElement);
  },
};

export const BrandLight: Story = { globals: { theme: 'light', viewport: { value: 'desktop' } } };
export const BrandDark: Story = { globals: { theme: 'dark', viewport: { value: 'desktop' } } };
export const BrandNarrowLargeText: Story = {
  globals: { theme: 'dark', viewport: { value: 'narrow375' }, fontScale: '200' },
};
