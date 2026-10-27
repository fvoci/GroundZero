import type { Meta, StoryObj } from '@storybook/svelte';
import { expect, screen } from 'storybook/test';
import { toast } from 'svelte-sonner';

import FeedbackHost from '$lib/features/feedback/FeedbackHost.svelte';
import {
  auth,
  accountSafety,
  connection,
  notifyDone,
  notifyError,
} from '$lib/features/feedback/feedback.svelte';

// 알림(D-82): 완료 토스트와 연결·로그인 배너(src/lib/features/feedback/FeedbackHost.svelte). 배너 상태는 제품의
// 공유 상태(`connection`·`auth`)를 story 동안만 바꾸고 끝나면 되돌린다.
const meta = {
  title: 'Feedback/FeedbackHost',
  component: FeedbackHost,
  args: { reload: () => undefined },
  beforeEach: () => () => {
    connection.lost = false;
    auth.expired = false;
    accountSafety.switching = false;
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
    await expect((await screen.findByTestId('connection-banner')).parentElement).toHaveAttribute(
      'role',
      'status',
    );
  },
};

export const BrandLight: Story = {
  ...ConnectionLost,
  globals: { theme: 'light', viewport: { value: 'desktop' } },
};
export const BrandDark: Story = {
  ...ConnectionLost,
  globals: { theme: 'dark', viewport: { value: 'desktop' } },
};
export const BrandNarrowLargeText: Story = {
  ...ConnectionLost,
  globals: { theme: 'dark', viewport: { value: 'narrow375' }, fontScale: '200' },
};

export const AccountChanged: Story = {
  beforeEach: () => {
    accountSafety.switching = true;
  },
  globals: { viewport: { value: 'narrow375' }, fontScale: '200' },
};

export const RepeatedError: Story = {
  play: async () => {
    notifyError('연결 실패', 'connection');
    notifyError('다시 연결하지 못했습니다.', 'connection');
    await screen.findByText('다시 연결하지 못했습니다.');
    // 같은 key는 첫 토스트를 고쳐 쓴다(토스트가 쌓이지 않는다, D-82). 앞 story의 닫힌 토스트가 사라지는 중일 수 있어
    // 이 연결 오류의 토스트만 센다.
    await expect(screen.queryByText('연결 실패')).toBeNull();
    await expect(
      [...document.querySelectorAll('[data-sonner-toast]')].filter((item) =>
        item.textContent.includes('연결'),
      ),
    ).toHaveLength(1);
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
