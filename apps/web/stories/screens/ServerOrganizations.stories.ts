import type { Meta, StoryObj } from '@storybook/svelte';
import { expect, fn, within } from 'storybook/test';
import ServerAdminScreen from '$lib/features/server-admin/ServerAdminScreen.svelte';
import MainFrame from '../fixtures/MainFrame.svelte';
import { setUpServerAdmin } from '../fixtures/server-admin';

const meta = {
  title: 'Screens/ServerOrganizations',
  component: ServerAdminScreen,
  args: { isServerAdmin: true, meId: 'story-user', tab: 'organizations', oncreated: fn() },
  decorators: [() => ({ Component: MainFrame })],
  beforeEach: () => setUpServerAdmin('organizations'),
  globals: { theme: 'light', fontScale: '100', viewport: { value: 'desktop' } },
} satisfies Meta<typeof ServerAdminScreen>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Default: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(await canvas.findAllByRole('listitem')).toHaveLength(2);
    await expect(canvas.getAllByRole('tab')).toHaveLength(2);
    await expect(canvas.getByRole('button', { name: '조직 만들기' })).toBeEnabled();
  },
};
export const NarrowDarkLargeText: Story = {
  ...Default,
  globals: { theme: 'dark', fontScale: '200', viewport: { value: 'narrow375' } },
};
