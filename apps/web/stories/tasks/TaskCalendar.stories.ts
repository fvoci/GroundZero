import type { Meta, StoryObj } from '@storybook/svelte';
import { fn } from 'storybook/test';
import TaskCalendar from '$lib/features/tasks/TaskCalendar.svelte';
import { taskViews } from '../fixtures/synthetic';
import { fixClock } from '../fixtures/clock';
const meta = {
  title: 'Tasks/TaskCalendar',
  component: TaskCalendar,
  args: { tasks: taskViews(), onopen: fn(), oncreate: fn() },
  beforeEach: () => fixClock(),
} satisfies Meta<typeof TaskCalendar>;

export default meta;
type Story = StoryObj<typeof meta>;

export const LightDesktop: Story = {
  globals: { theme: 'light', fontScale: '100', viewport: { value: 'desktop', isRotated: false } },
};
export const Dark375Font200: Story = {
  globals: { theme: 'dark', fontScale: '200', viewport: { value: 'narrow375', isRotated: false } },
};
