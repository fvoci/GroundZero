import type { Meta, StoryObj } from '@storybook/svelte';
import { expect, within } from 'storybook/test';

import CalendarScreen from '$lib/features/events/CalendarScreen.svelte';

import { fixClock } from '../fixtures/clock';
import MainFrame from '../fixtures/MainFrame.svelte';
import { setUpScreen } from '../fixtures/screens';
import { calendarView, TODAY, WORKSPACE } from '../fixtures/synthetic';

const meta = {
  title: 'Events/CalendarScreen',
  component: CalendarScreen,
  args: { workspace: WORKSPACE },
  decorators: [() => ({ Component: MainFrame })],
  globals: { theme: 'light', fontScale: '100', viewport: { value: 'desktop' } },
  beforeEach: () => {
    const restoreClock = fixClock();
    const cleanup = setUpScreen('/');
    return () => {
      cleanup();
      restoreClock();
    };
  },
} satisfies Meta<typeof CalendarScreen>;
export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  play: async ({ canvasElement }) => {
    const agenda = within(await within(canvasElement).findByTestId('calendar-agenda'));
    // 기준 달에 걸친 업무만 확인한다(다음 달 마감 업무도 같은 seed 응답에 있다).
    const month = TODAY.slice(0, 7);
    for (const task of calendarView().tasks.filter(
      (task) =>
        task.start_date?.startsWith(month) === true || task.due_date?.startsWith(month) === true,
    )) {
      await expect(
        await agenda.findAllByRole('button', { name: (name) => name.includes(task.title) }),
      ).not.toHaveLength(0);
    }
  },
};
export const NarrowDarkLargeText: Story = {
  ...Default,
  globals: { theme: 'dark', fontScale: '200', viewport: { value: 'narrow375' } },
};
