import type { Meta, StoryObj } from '@storybook/svelte';
import { expect, fn, within } from 'storybook/test';
import OrganizationScreen from '$lib/features/organizations/OrganizationScreen.svelte';
import MainFrame from '../fixtures/MainFrame.svelte';
import { ORGANIZATION_MANAGEMENT, setUpOrganizations } from '../fixtures/organizations';

const meta = {
  title: 'Screens/OrganizationInvitations',
  component: OrganizationScreen,
  args: {
    organizationId: ORGANIZATION_MANAGEMENT.id,
    tab: 'invitations',
    meId: 'story-user',
    onchanged: fn(),
  },
  decorators: [() => ({ Component: MainFrame })],
  beforeEach: () => setUpOrganizations('invitations'),
  globals: { theme: 'light', fontScale: '100', viewport: { value: 'desktop' } },
} satisfies Meta<typeof OrganizationScreen>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Default: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(await canvas.findAllByRole('listitem')).toHaveLength(2);
    await expect(canvas.getByText('만료됨')).toBeInTheDocument();
  },
};
export const NarrowDarkLargeText: Story = {
  ...Default,
  globals: { theme: 'dark', fontScale: '200', viewport: { value: 'narrow375' } },
};
