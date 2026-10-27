import type { Meta, StoryObj } from '@storybook/svelte';
import { expect, within } from 'storybook/test';
import MyTasksScreen from '$lib/features/home/MyTasksScreen.svelte';
import MainFrame from '../fixtures/MainFrame.svelte';
import { myTasksView } from '../fixtures/personal';
import { setUpScreen } from '../fixtures/screens';

const meta = {
  title: 'Screens/MyTasks',
  component: MyTasksScreen,
  decorators: [() => ({ Component: MainFrame })],
  globals: { theme: 'light', fontScale: '100', viewport: { value: 'desktop' } },
  beforeEach: () =>
    setUpScreen('/tasks', {
      myTasks: () => {
        const view = myTasksView();
        const first = view.workspaces[0];
        const task = first?.tasks[0];
        if (first !== undefined && task !== undefined) {
          first.tasks.push({
            ...task,
            id: 'undated',
            due_date: null,
            bucket: 'none',
            title: '날짜 없는 내 업무 · Undated task',
          });
          view.counts.undated = 1;
        }
        return Promise.resolve(view);
      },
    }),
} satisfies Meta<typeof MyTasksScreen>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Workspaces: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const task = await canvas.findByText('날짜 없는 내 업무 · Undated task');
    // 워크스페이스 안의 "날짜 없음" 묶음 제목 아래에 있다(D-120).
    const group = task.closest('section');
    await expect(group?.querySelector('h3')?.textContent).toContain('날짜 없음');
  },
};
export const NarrowDarkLargeText: Story = {
  ...Workspaces,
  globals: { theme: 'dark', fontScale: '200', viewport: { value: 'narrow375' } },
};
