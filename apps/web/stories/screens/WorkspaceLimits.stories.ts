import type { Meta, StoryObj } from '@storybook/svelte';
import { expect, within } from 'storybook/test';

import MembersScreen from '$lib/features/members/MembersScreen.svelte';

import MainFrame from '../fixtures/MainFrame.svelte';
import { setUpWorkspaceManagement } from '../fixtures/workspace-management';
import { LIMITS_WORKSPACE } from '../fixtures/workspace-limits';

const meta = {
  title: 'Screens/WorkspaceLimits',
  component: MembersScreen,
  args: {
    workspace: LIMITS_WORKSPACE,
    tab: 'settings',
    meId: 'story-user',
    onchanged: () => undefined,
  },
  decorators: [() => ({ Component: MainFrame })],
  globals: { theme: 'light', fontScale: '100', viewport: { value: 'desktop' } },
  beforeEach: () => setUpWorkspaceManagement('settings'),
} satisfies Meta<typeof MembersScreen>;
export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(await canvas.findByLabelText('활동 보존 일수', { exact: true })).toHaveValue('60');
    await expect(canvas.getAllByRole('tab')).toHaveLength(4);
    await expect(canvas.getAllByRole('checkbox')).toHaveLength(5);
    await expect(canvas.getByRole('heading', { name: '위험 구역' })).toBeVisible();
    await expect(canvas.queryByTestId('member-list')).toBeNull();
  },
};
export const NarrowDarkLargeText: Story = {
  ...Default,
  globals: { theme: 'dark', fontScale: '200', viewport: { value: 'narrow375' } },
};
