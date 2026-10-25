import type { Meta, StoryObj } from '@storybook/svelte';
import { userEvent, within } from 'storybook/test';
import CoursePanel from '$lib/features/courses/CoursePanel.svelte';
import { m } from '$lib/paraglide/messages.js';
import MainFrame from '../fixtures/MainFrame.svelte';
import { setUpTime } from '../time/setup';
const meta = {
  globals: { theme: 'light', fontScale: '100', viewport: { value: 'desktop' } },
  title: 'Courses/CoursePanel',
  component: CoursePanel,
  args: { itemId: 'item-1' },
  decorators: [() => ({ Component: MainFrame })],
  beforeEach: () => setUpTime(),
} satisfies Meta<typeof CoursePanel>;
export default meta;
type Story = StoryObj<typeof meta>;
export const StartEpisode: Story = {
  play: async ({ canvasElement }) => {
    await userEvent.click(
      await within(canvasElement).findByRole('button', {
        name: m.courses_track_aria({ number: 1 }),
      }),
    );
  },
};
export const NarrowDark200: Story = {
  globals: { theme: 'dark', fontScale: '200', viewport: { value: 'narrow375' } },
};
