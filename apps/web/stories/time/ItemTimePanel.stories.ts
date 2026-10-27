import type { Meta, StoryObj } from '@storybook/svelte';
import ItemTimePanel from '$lib/features/time/ItemTimePanel.svelte';
import MainFrame from '../fixtures/MainFrame.svelte';
import { setUpTime } from './setup';
const meta = {
  globals: { theme: 'light', fontScale: '100', viewport: { value: 'desktop' } },
  title: 'Time/ItemTimePanel',
  component: ItemTimePanel,
  args: { itemId: 'item-1' },
  decorators: [() => ({ Component: MainFrame })],
  beforeEach: () => setUpTime(),
} satisfies Meta<typeof ItemTimePanel>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Mine: Story = {};
export const NarrowDark200: Story = {
  globals: { theme: 'dark', fontScale: '200', viewport: { value: 'narrow375' } },
};
