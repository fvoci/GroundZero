import type { Meta, StoryObj } from '@storybook/svelte';
import { expect, within } from 'storybook/test';

import { NetworkError } from '$lib/api/client';
import HomeDashboard from '$lib/features/home/HomeDashboard.svelte';

import { never, setUpScreen } from '../fixtures/screens';
import {
  emptyCalendarView,
  emptyHomeView,
  homeView,
  notificationsView,
  TASKS,
  WORKSPACE,
} from '../fixtures/synthetic';

// 홈(src/lib/features/home/HomeDashboard.svelte): 다음 할 일·이번 주·고정·즐겨찾기·진행 중인 항목·최근·알림. 홈은
// `api.home`·워크스페이스마다 `api.calendar`·`api.notifications`를 부른다. 확인은 보이는 글과 오류만 한다.
const meta = {
  title: 'Screens/Home',
  component: HomeDashboard,
  args: { workspaces: [WORKSPACE] },
  beforeEach: () => setUpScreen('/'),
} satisfies Meta<typeof HomeDashboard>;

export default meta;
type Story = StoryObj<typeof meta>;

/** 일정·알림·업무가 있는 홈. 지난 마감, 이번 주 업무, 읽지 않은 알림, 진행 중인 항목. */
export const DataRich: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const home = homeView();
    const overdue = home.up_next.filter((t) => t.bucket === 'overdue').length;
    await canvas.findByRole('heading', { level: 3, name: `기한 지남 ${String(overdue)}` });
    for (const t of home.up_next)
      await expect(canvas.getAllByText(t.title).length).toBeGreaterThan(0);
    // 이번 주: 날짜가 있는 업무 제목이 그날 줄에 보인다(오늘 시작·마감인 이모지 업무).
    const emoji = TASKS.find((t) => t.due === 1);
    if (emoji === undefined) throw new Error('no task due tomorrow');
    const week = canvas.getByRole('heading', { name: '이번 주' }).closest('section');
    if (week === null) throw new Error('no week section');
    await expect(week).toHaveTextContent(emoji.title);
    // 알림: 읽지 않은 것 다섯 개까지.
    const notes = canvas.getByRole('heading', { name: '알림' }).closest('section');
    if (notes === null) throw new Error('no notifications section');
    await expect(notes).toHaveTextContent('Second User');
    await expect(canvas.queryByText('읽지 않은 알림이 없습니다.')).toBeNull();
    await expect(notificationsView().length).toBeGreaterThan(0);
    await expect(canvas.queryByRole('alert')).toBeNull();
  },
};

/** 처음 쓰는 사람: 워크스페이스도 업무도 없다. 빈 칸마다 안내가 보인다. */
export const EmptyNewUser: Story = {
  args: { workspaces: [] },
  beforeEach: () =>
    setUpScreen('/', {
      home: () => Promise.resolve(emptyHomeView()),
      calendar: () => Promise.resolve(emptyCalendarView()),
      notifications: () => Promise.resolve([]),
    }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await canvas.findByText('이번 주까지 마감인 내 업무가 없습니다.');
    await expect(canvas.getByText('진행 중인 항목이 없습니다.')).toBeInTheDocument();
    await expect(canvas.getByText('최근에 고친 문서·업무가 없습니다.')).toBeInTheDocument();
    await expect(canvas.getByText('읽지 않은 알림이 없습니다.')).toBeInTheDocument();
  },
};

/** 받기 전에는 "없습니다"라고 말하지 않는다(불러오는 중·실패를 빈 홈으로 읽지 않게). */
async function expectNoEmptyClaims(canvasElement: HTMLElement): Promise<void> {
  const canvas = within(canvasElement);
  await expect(canvas.queryByText('진행 중인 항목이 없습니다.')).toBeNull();
  await expect(canvas.queryByText('읽지 않은 알림이 없습니다.')).toBeNull();
  await expect(canvas.queryByText(/“홈에 고정”으로 더할 수 있습니다/)).toBeNull();
}

/** 불러오는 중: 홈 응답을 기다린다(날짜가 아직 없다). */
export const Loading: Story = {
  beforeEach: () => setUpScreen('/', { home: () => never() }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await canvas.findByRole('heading', { level: 1, name: '오늘' });
    await expect(canvas.queryByRole('alert')).toBeNull();
    await expectNoEmptyClaims(canvasElement);
  },
};

/** 불러오지 못함: 서버에 닿지 않았다는 오류를 알린다. */
export const LoadFailed: Story = {
  beforeEach: () => setUpScreen('/', { home: () => Promise.reject(new NetworkError('offline')) }),
  play: async ({ canvasElement }) => {
    const alert = await within(canvasElement).findByRole('alert');
    await expect(alert).toHaveTextContent('홈을 불러오지 못했습니다.');
    await expectNoEmptyClaims(canvasElement);
  },
};
