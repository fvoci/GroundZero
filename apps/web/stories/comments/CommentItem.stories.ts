import type { Meta, StoryObj } from '@storybook/svelte';
import { expect, fn, userEvent, within } from 'storybook/test';

import CommentItem from '$lib/features/comments/CommentItem.svelte';

import { comment, WRITER_NAME } from '../fixtures/comments';
import { EMOJI, LONG_KO, NO_SPACE, NUMBERS } from '../fixtures/text';

// 댓글 하나(src/lib/features/comments/CommentItem.svelte): 고치기·지우기 확인과 저장 실패 때 입력 보존.
const meta = {
  title: 'Comments/CommentItem',
  component: CommentItem,
  args: {
    comment: comment(),
    busy: false,
    onedit: fn(() => Promise.resolve(true)),
    ondelete: fn(),
  },
} satisfies Meta<typeof CommentItem>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Mine: Story = {};

/** 다른 사람의 댓글: 고치기·지우기 단추가 없다(읽기 전용). */
export const OthersReadOnly: Story = {
  args: { comment: comment({ author_name: WRITER_NAME, can_edit: false, can_delete: false }) },
  play: async ({ canvasElement }) => {
    await expect(within(canvasElement).queryByRole('button')).toBeNull();
  },
};

export const Edited: Story = { args: { comment: comment({ edited: true }) } };

export const Deleted: Story = {
  args: {
    comment: comment({
      body: '삭제된 댓글입니다.',
      deleted: true,
      can_edit: false,
      can_delete: false,
    }),
  },
};

export const UnknownAuthor: Story = { args: { comment: comment({ author_name: null }) } };

export const LongBody: Story = {
  args: {
    comment: comment({ body: `${LONG_KO}\n${NO_SPACE}\n${NUMBERS}\n${EMOJI}`, can_edit: false }),
  },
};

/** 저장 중(다른 요청 진행 중): 고치기 저장 단추를 막는다. */
export const Saving: Story = {
  args: { busy: true },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await userEvent.click(canvas.getByRole('button', { name: '고치기' }));
    await expect(canvas.getByRole('button', { name: '저장' })).toBeDisabled();
  },
};

/** 저장 실패: 고친 글이 편집 칸에 그대로 남아 다시 저장할 수 있다. */
export const SaveFailedKeepsInput: Story = {
  args: { onedit: fn(() => Promise.resolve(false)) },
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);
    await userEvent.click(canvas.getByRole('button', { name: '고치기' }));
    const box = canvas.getByRole('textbox', { name: '댓글 고치기' });
    await userEvent.clear(box);
    await userEvent.type(box, '고친 내용');
    await userEvent.click(canvas.getByRole('button', { name: '저장' }));
    await expect(args.onedit).toHaveBeenCalledWith('고친 내용');
    await expect(canvas.getByRole('textbox', { name: '댓글 고치기' })).toHaveValue('고친 내용');
  },
};

/** 고치다 취소하면 원래 본문으로 돌아간다. */
export const EditCancel: Story = {
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);
    await userEvent.click(canvas.getByRole('button', { name: '고치기' }));
    await userEvent.type(canvas.getByRole('textbox', { name: '댓글 고치기' }), ' 추가');
    await userEvent.click(canvas.getByRole('button', { name: '취소' }));
    await expect(canvas.queryByRole('textbox')).toBeNull();
    await expect(args.onedit).not.toHaveBeenCalled();
  },
};

/** 지우기 확인: 초점이 확인 단추로 가고, 취소하면 지우지 않는다. */
export const DeleteConfirmCancel: Story = {
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);
    await userEvent.click(canvas.getByRole('button', { name: '지우기' }));
    const group = canvas.getByRole('group');
    await expect(within(group).getByRole('button', { name: '지우기' })).toHaveFocus();
    await userEvent.click(within(group).getByRole('button', { name: '취소' }));
    await expect(args.ondelete).not.toHaveBeenCalled();
  },
};
