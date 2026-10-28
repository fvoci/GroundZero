import type { Meta, StoryObj } from '@storybook/svelte';
import { expect, spyOn, within, userEvent } from 'storybook/test';
import { api } from '$lib/api/client';
import ItemActivityTab from '$lib/features/activity/ItemActivityTab.svelte';
import MainFrame from '../fixtures/MainFrame.svelte';
import { ACTIVITY } from '../fixtures/personal';
import { setUpScreen } from '../fixtures/screens';
import { WORKSPACE } from '../fixtures/synthetic';
const meta = {
  title: 'Screens/Activity',
  component: ItemActivityTab,
  args: { itemId: 'item-1', workspaceId: WORKSPACE.id },
  decorators: [() => ({ Component: MainFrame })],
  globals: { theme: 'light', fontScale: '100', viewport: { value: 'desktop' } },
  beforeEach: () => {
    const cleanup = setUpScreen('/');
    spyOn(api, 'targetActivity').mockResolvedValue({
      activity: ACTIVITY,
      next_before: null,
      next_before_id: null,
    });
    return cleanup;
  },
} satisfies Meta<typeof ItemActivityTab>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Feed: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await userEvent.click(canvas.getByRole('button', { name: '활동' }));
    await expect(await canvas.findByRole('list')).not.toBeEmptyDOMElement();
  },
};
export const NarrowDarkLargeText: Story = {
  ...Feed,
  globals: { theme: 'dark', fontScale: '200', viewport: { value: 'narrow375' } },
};
