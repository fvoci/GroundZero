import type { Meta, StoryObj } from '@storybook/svelte';
import { expect, fn, within } from 'storybook/test';
import ServerAdminScreen from '$lib/features/server-admin/ServerAdminScreen.svelte';
import MainFrame from '../fixtures/MainFrame.svelte';
import { setUpServerAdmin } from '../fixtures/server-admin';

const meta = {
  title: 'Screens/ServerAdmins',
  component: ServerAdminScreen,
  args: { isServerAdmin: true, meId: 'story-user', tab: 'admins', oncreated: fn() },
  decorators: [() => ({ Component: MainFrame })],
  beforeEach: () => setUpServerAdmin('admins'),
  globals: { theme: 'light', fontScale: '100', viewport: { value: 'desktop' } },
} satisfies Meta<typeof ServerAdminScreen>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Default: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(await canvas.findAllByRole('listitem')).toHaveLength(50);
    await expect(canvas.queryByRole('button', { name: '조직 만들기' })).toBeNull();
  },
};
export const NarrowDarkLargeText: Story = {
  ...Default,
  globals: { theme: 'dark', fontScale: '200', viewport: { value: 'narrow375' } },
};
