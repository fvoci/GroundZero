import { activityApi, type CommentView } from '$lib/features/activity/api';
import type { Meta, StoryObj } from '@storybook/svelte';
import { expect, spyOn, userEvent, waitFor, within } from 'storybook/test';

import { NetworkError } from '$lib/api/client';
import CommentsPanel from '$lib/features/comments/CommentsPanel.svelte';
import { formDraftTabKey } from '$lib/features/common/form-draft';
import { me } from '$lib/me.svelte';

import { comment, THREAD } from '../fixtures/comments';

// 댓글 영역(src/lib/features/comments/CommentsPanel.svelte). 불러오기·빈 목록·달기 중·응답 지연·실패와 재시도·입력
// 보존을 실제 책임 컴포넌트로 본다. 서버 대신 `api`의 함수를 story마다 바꿔 끼운다(tests/component/comment-retry
// .test.ts와 같은 방식). Storybook은 story가 바뀔 때 바꿔 끼운 함수를 원래대로 돌린다.
const meta = {
  title: 'Comments/CommentsPanel',
  component: CommentsPanel,
  args: { target: { kind: 'document', id: 'doc-0001' }, canWrite: true },
  beforeEach: () => {
    me.userId = 'story-comments';
    for (const key of Object.keys(localStorage)) {
      if (key.startsWith('fvoci:story-comments:')) localStorage.removeItem(key);
    }
  },
} satisfies Meta<typeof CommentsPanel>;

export default meta;
type Story = StoryObj<typeof meta>;

const NEW_BOX = '새 댓글(Ctrl·⌘+Enter로 달기)';

function never<T>(): Promise<T> {
  return new Promise<T>(() => undefined);
}

export const Threads: Story = {
  beforeEach: () => {
    spyOn(activityApi, 'comments').mockResolvedValue(THREAD);
  },
};

export const Empty: Story = {
  beforeEach: () => {
    spyOn(activityApi, 'comments').mockResolvedValue([]);
  },
  play: async ({ canvasElement }) => {
    await within(canvasElement).findByText('댓글이 없습니다.');
  },
};

/** 불러오는 중: 목록을 받기 전에는 빈 목록 글 대신 불러오는 중이라고 보인다. */
export const Loading: Story = {
  beforeEach: () => {
    spyOn(activityApi, 'comments').mockImplementation(() => never());
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByText('불러오는 중…')).toBeInTheDocument();
    await expect(canvas.queryByText('댓글이 없습니다.')).toBeNull();
  },
};

export const LoadFailed: Story = {
  beforeEach: () => {
    spyOn(activityApi, 'comments').mockRejectedValue(new NetworkError('offline'));
  },
  play: async ({ canvasElement }) => {
    await within(canvasElement).findByText(/댓글을 불러오지 못했습니다\./);
  },
};

/** 달기 중·응답 지연: 응답이 오지 않는 동안 단추를 막고 쓴 글을 지우지 않는다. */
export const PostingSlow: Story = {
  beforeEach: () => {
    spyOn(activityApi, 'comments').mockResolvedValue([]);
    spyOn(activityApi, 'addComment').mockImplementation(() => never());
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const box = canvas.getByRole('textbox', { name: NEW_BOX });
    await userEvent.type(box, '응답을 기다리는 댓글');
    await userEvent.click(canvas.getByRole('button', { name: '댓글 달기' }));
    await waitFor(() => expect(canvas.getByRole('button', { name: '댓글 달기' })).toBeDisabled());
    await expect(box).toHaveValue('응답을 기다리는 댓글');
  },
};

/** 달기 실패 뒤 다시 시도: 실패하면 글이 남고, 다시 누르면 같은 요청 id로 보내 성공한 뒤 칸을 비운다. */
const sentIds: string[] = [];
export const PostFailedThenRetry: Story = {
  beforeEach: () => {
    let saved: CommentView[] = [];
    sentIds.length = 0;
    spyOn(activityApi, 'comments').mockImplementation(() => Promise.resolve(saved));
    spyOn(activityApi, 'addComment').mockImplementation((_target, body, _parent, requestId) => {
      sentIds.push(requestId);
      if (sentIds.length === 1) return Promise.reject(new NetworkError('lost'));
      saved = [comment({ id: 'c-0100', body })];
      return Promise.resolve('c-0100');
    });
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const box = canvas.getByRole('textbox', { name: NEW_BOX });
    await userEvent.type(box, '다시 보낼 댓글');
    await userEvent.click(canvas.getByRole('button', { name: '댓글 달기' }));
    await canvas.findByText(/처리하지 못했습니다\./);
    await expect(box).toHaveValue('다시 보낼 댓글');

    await userEvent.click(canvas.getByRole('button', { name: '댓글 달기' }));
    await waitFor(() => expect(box).toHaveValue(''));
    await expect(sentIds).toHaveLength(2);
    await expect(sentIds[1]).toBe(sentIds[0]);
    await canvas.findByText('다시 보낼 댓글');
  },
};

export const ReadOnly: Story = {
  ...Threads,
  args: { canWrite: false },
};

export const Archived: Story = {
  ...Threads,
  args: { canWrite: false, archived: true },
};

export const BrandNarrowLargeText: Story = {
  ...Threads,
  globals: { theme: 'dark', viewport: { value: 'narrow375' }, fontScale: '200' },
};

export const RestoreOffer: Story = {
  beforeEach: () => {
    spyOn(activityApi, 'comments').mockResolvedValue([]);
    const key = formDraftTabKey('comment-compose:document:doc-0001:new');
    if (key === undefined) throw new Error('missing story user');
    localStorage.setItem(
      key,
      JSON.stringify({
        savedAt: '2026-10-26T12:00:00Z',
        input: {
          text: '검토하면서 남긴 댓글 초안입니다. 새로고침 뒤 사용자가 되살릴 수 있습니다.',
          request: null,
        },
      }),
    );
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByTestId('comment-draft-offer')).toBeVisible();
    await expect(canvas.getByRole('textbox', { name: NEW_BOX })).toHaveValue('');
  },
};
