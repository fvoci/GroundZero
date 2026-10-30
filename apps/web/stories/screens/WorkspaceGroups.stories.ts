import type { Meta, StoryObj } from '@storybook/svelte';
import { expect, within } from 'storybook/test';
import MembersScreen from '$lib/features/members/MembersScreen.svelte';
import MainFrame from '../fixtures/MainFrame.svelte';
import { setUpWorkspaceManagement } from '../fixtures/workspace-management';
import { LIMITS_WORKSPACE } from '../fixtures/workspace-limits';

const meta = {
  title: 'Screens/WorkspaceGroups',
  component: MembersScreen,
  args: {
    workspace: LIMITS_WORKSPACE,
    tab: 'groups',
    meId: 'story-user',
    onchanged: () => undefined,
  },
  decorators: [() => ({ Component: MainFrame })],
  beforeEach: () => setUpWorkspaceManagement('groups'),
  globals: { theme: 'light', fontScale: '100', viewport: { value: 'desktop' } },
} satisfies Meta<typeof MembersScreen>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Default: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(await canvas.findByLabelText('새 그룹 이름')).toBeVisible();
    await expect(await canvas.findByText('연구팀')).toBeVisible();
    await expect(canvas.getAllByRole('tab')).toHaveLength(4);
  },
};
export const NarrowDarkLargeText: Story = {
  ...Default,
  globals: { theme: 'dark', fontScale: '200', viewport: { value: 'narrow375' } },
};
