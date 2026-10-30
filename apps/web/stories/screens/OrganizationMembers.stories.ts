import type { Meta, StoryObj } from '@storybook/svelte';
import { expect, fn, userEvent, within } from 'storybook/test';
import OrganizationScreen from '$lib/features/organizations/OrganizationScreen.svelte';
import MainFrame from '../fixtures/MainFrame.svelte';
import { ORGANIZATION_MANAGEMENT, setUpOrganizations } from '../fixtures/organizations';

const meta = {
  title: 'Screens/OrganizationMembers',
  component: OrganizationScreen,
  args: {
    organizationId: ORGANIZATION_MANAGEMENT.id,
    tab: 'members',
    meId: 'story-user',
    onchanged: fn(),
  },
  decorators: [() => ({ Component: MainFrame })],
  beforeEach: () => setUpOrganizations('members'),
  globals: { theme: 'light', fontScale: '100', viewport: { value: 'desktop' } },
} satisfies Meta<typeof OrganizationScreen>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Default: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(await canvas.findAllByRole('listitem')).toHaveLength(50);
    await expect(canvas.getAllByRole('tab')).toHaveLength(4);
    await expect(canvas.getByRole('tab', { name: '멤버 50명' })).toBeVisible();
    const filter = canvas.getByRole('searchbox', { name: '이름·이메일로 멤버 검색' });
    await userEvent.type(filter, 'address.49@');
    await expect(canvas.getAllByRole('listitem')).toHaveLength(1);
    await expect(canvas.getByRole('tab', { name: '멤버 50명' })).toBeVisible();
    await userEvent.clear(filter);
    await expect(canvas.getAllByRole('listitem')).toHaveLength(50);
  },
};
export const NarrowDarkLargeText: Story = {
  ...Default,
  globals: { theme: 'dark', fontScale: '200', viewport: { value: 'narrow375' } },
};
