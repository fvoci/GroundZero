import type { Meta, StoryObj } from '@storybook/svelte';
import { userEvent, within } from 'storybook/test';
import TaskTimePanel from '$lib/features/time/TaskTimePanel.svelte';
import MainFrame from '../fixtures/MainFrame.svelte';
import { setUpTime } from './setup';
const meta = {
  globals: { theme: 'light', fontScale: '100', viewport: { value: 'desktop' } },
  title: 'Time/TaskTimePanel',
  component: TaskTimePanel,
  args: { taskId: 'task-2' },
  decorators: [() => ({ Component: MainFrame })],
  beforeEach: () => setUpTime(),
} satisfies Meta<typeof TaskTimePanel>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Detail: Story = {};
export const Start: Story = {
  play: async ({ canvasElement }) => {
    await userEvent.click(within(canvasElement).getByRole('button', { name: '이 업무 시간 재기' }));
  },
};
export const Compact: Story = { args: { compact: true } };
export const NarrowDark200: Story = {
  globals: { theme: 'dark', fontScale: '200', viewport: { value: 'narrow375' } },
};
