import type { Meta, StoryObj } from '@storybook/svelte';
import { expect, spyOn, userEvent, within } from 'storybook/test';
import { api } from '$lib/api/client';
import TimerWidget from '$lib/features/time/TimerWidget.svelte';
import MainFrame from '../fixtures/MainFrame.svelte';
import { setUpTime } from './setup';
const meta = {
  globals: { theme: 'light', fontScale: '100', viewport: { value: 'desktop' } },
  title: 'Time/TimerWidget',
  component: TimerWidget,
  decorators: [() => ({ Component: MainFrame })],
  beforeEach: () => setUpTime(),
} satisfies Meta<typeof TimerWidget>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Empty: Story = {
  play: async ({ canvasElement }) => {
    await expect(within(canvasElement).queryByRole('button')).toBeNull();
  },
};
export const Running: Story = {
  beforeEach: () => setUpTime(true),
  play: async ({ canvasElement }) => {
    await within(canvasElement).findByRole('button', { name: '종료' });
    // 머리글은 "기록 중" 표시와 멈춤만(D-117).
    await expect(within(canvasElement).getByText('기록 중')).toBeInTheDocument();
  },
};
export const StopFailed: Story = {
  beforeEach: async () => {
    const restore = await setUpTime(true);
    spyOn(api, 'timerAction').mockRejectedValue(new Error('offline'));
    return restore;
  },
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    await userEvent.click(await c.findByRole('button', { name: '종료' }));
    await c.findByRole('alert');
  },
};
export const NarrowDark200: Story = {
  ...Running,
  globals: { theme: 'dark', fontScale: '200', viewport: { value: 'narrow375' } },
};
