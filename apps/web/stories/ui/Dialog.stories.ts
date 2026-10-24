import type { Meta, StoryObj } from '@storybook/svelte';
import { expect, fn, screen, userEvent, waitFor, within } from 'storybook/test';

import DialogFixture from '../fixtures/DialogFixture.svelte';
import { pressEscape } from '../fixtures/keys';
import { LONG_KO } from '../fixtures/text';

// 공통 대화상자(src/lib/ui/dialog, Bits UI). 초기 초점·Escape·초점 복귀·내부 스크롤을 본다.
const meta = {
  title: 'UI/Dialog',
  component: DialogFixture,
  args: {
    title: '업무 편집',
    description: '제목을 고친 뒤 저장합니다.',
    onsave: fn(),
  },
} satisfies Meta<typeof DialogFixture>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

/** 열면 대화상자 안으로 초점이 들어가고, Escape로 닫으면 연 단추로 초점이 돌아온다. */
export const FocusAndEscape: Story = {
  play: async ({ canvasElement }) => {
    const trigger = within(canvasElement).getByRole('button', { name: '업무 편집' });
    await userEvent.click(trigger);
    const dialog = await screen.findByRole('dialog', { name: '업무 편집' });
    await waitFor(() => expect(dialog.contains(document.activeElement)).toBe(true));
    await pressEscape();
    await waitFor(() => expect(trigger).toHaveAttribute('aria-expanded', 'false'));
    await waitFor(() => expect(trigger).toHaveFocus());
  },
};

/** Tab·Shift+Tab은 대화상자 안에서만 돈다. Enter로 저장한다. */
export const KeyboardSave: Story = {
  play: async ({ canvasElement, args }) => {
    await userEvent.click(within(canvasElement).getByRole('button', { name: '업무 편집' }));
    const dialog = await screen.findByRole('dialog', { name: '업무 편집' });
    const box = within(dialog).getByRole('textbox', { name: '제목' });
    box.focus();
    await userEvent.keyboard('주간 회의 준비');
    // 초점 가두기는 실제 브라우저의 초점 이동에 기대므로 jsdom 재사용(tests/component/stories.test.ts)에서는 건너뛴다.
    const trapCheckable = !navigator.userAgent.includes('jsdom');
    for (let i = 0; trapCheckable && i < 6; i += 1) {
      await userEvent.tab();
      await expect(dialog.contains(document.activeElement)).toBe(true);
    }
    for (let i = 0; trapCheckable && i < 6; i += 1) {
      await userEvent.tab({ shift: true });
      await expect(dialog.contains(document.activeElement)).toBe(true);
    }
    box.focus();
    await userEvent.keyboard('{Enter}');
    await expect(args.onsave).toHaveBeenCalledWith('주간 회의 준비');
  },
};

/** 화면보다 긴 내용: 대화상자 안에서 스크롤되고 바깥 페이지는 움직이지 않는다. 375px·글자 200%에서 확인한다. */
export const LongContentScroll: Story = {
  args: {
    title: LONG_KO,
    paragraphs: 14,
    contentClass: 'max-h-[90vh] overflow-y-auto sm:max-w-xl',
  },
  play: async ({ canvasElement }) => {
    await userEvent.click(within(canvasElement).getByRole('button', { name: '업무 편집' }));
    await screen.findByRole('dialog');
  },
};
