import type { Meta, StoryObj } from '@storybook/svelte';
import DisplaySettingsDialog from '$lib/features/settings/DisplaySettingsDialog.svelte';

const meta = {
  title: 'Settings/DisplaySettingsDialog',
  component: DisplaySettingsDialog,
  args: { open: true },
} satisfies Meta<typeof DisplaySettingsDialog>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Light: Story = { globals: { theme: 'light', viewport: { value: 'desktop' } } };
export const NarrowLargeText: Story = {
  globals: { theme: 'dark', viewport: { value: 'narrow375' }, fontScale: '200' },
};
