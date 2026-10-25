import type { Meta, StoryObj } from '@storybook/svelte';
import { userEvent, within } from 'storybook/test';
import StudyPanel from '$lib/features/study/StudyPanel.svelte';
import { m } from '$lib/paraglide/messages.js';
import MainFrame from '../fixtures/MainFrame.svelte';
import { setUpTime } from '../time/setup';
const meta = {
  globals: { theme: 'light', fontScale: '100', viewport: { value: 'desktop' } },
  title: 'Study/StudyPanel',
  component: StudyPanel,
  args: { itemId: 'item-1' },
  decorators: [() => ({ Component: MainFrame })],
  beforeEach: () => setUpTime(),
} satisfies Meta<typeof StudyPanel>;
export default meta;
type Story = StoryObj<typeof meta>;
export const StartUnit: Story = {
  play: async ({ canvasElement }) => {
    await userEvent.click(
      await within(canvasElement).findByRole('button', {
        name: m.study_track_aria({ title: '연습 문제' }),
      }),
    );
  },
};
export const NarrowDark200: Story = {
  globals: { theme: 'dark', fontScale: '200', viewport: { value: 'narrow375' } },
};
