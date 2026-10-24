import type { Meta, StoryObj } from '@storybook/svelte';
import { expect, screen } from 'storybook/test';
import { toast } from 'svelte-sonner';

import FeedbackHost from '$lib/features/feedback/FeedbackHost.svelte';
import { auth, connection, notifyDone } from '$lib/features/feedback/feedback.svelte';

// 알림(D-82): 완료 토스트와 연결·로그인 배너(src/lib/features/feedback/FeedbackHost.svelte). 배너 상태는 제품의
// 공유 상태(`connection`·`auth`)를 story 동안만 바꾸고 끝나면 되돌린다.
const meta = {
  title: 'Feedback/FeedbackHost',
  component: FeedbackHost,
  args: { reload: () => undefined },
  beforeEach: () => () => {
    connection.lost = false;
    auth.expired = false;
    toast.dismiss();
  },
} satisfies Meta<typeof FeedbackHost>;

export default meta;
type Story = StoryObj<typeof meta>;

/** 완료 알림. 같은 문구는 쌓지 않고 하나로 고친다. 넓은 화면은 오른쪽 아래, 좁은 화면은 위쪽 가운데. */
export const DoneToast: Story = {
  play: async () => {
    notifyDone('저장했습니다.');
    notifyDone('저장했습니다.');
    await screen.findByText('저장했습니다.');
    await expect(screen.getAllByText('저장했습니다.')).toHaveLength(1);
  },
};

export const ConnectionLost: Story = {
  beforeEach: () => {
    connection.lost = true;
  },
  play: async () => {
    await expect(await screen.findByTestId('connection-banner')).toHaveAttribute('role', 'status');
  },
};

export const SessionExpired: Story = {
  beforeEach: () => {
    auth.expired = true;
  },
  play: async () => {
    await expect(await screen.findByRole('link', { name: /로그인/ })).toHaveAttribute(
      'target',
      '_blank',
    );
  },
};
