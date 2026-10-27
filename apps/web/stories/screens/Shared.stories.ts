import type { Meta, StoryObj } from '@storybook/svelte';
import { expect, spyOn, within } from 'storybook/test';
import { api } from '$lib/api/client';
import SharedScreen from '$lib/features/notes/SharedScreen.svelte';
import { org } from '$lib/org.svelte';
import MainFrame from '../fixtures/MainFrame.svelte';
import { setUpScreen } from '../fixtures/screens';
const meta = {
  title: 'Screens/Shared',
  component: SharedScreen,
  decorators: [() => ({ Component: MainFrame })],
  globals: { theme: 'light', fontScale: '100', viewport: { value: 'desktop' } },
  beforeEach: () => {
    const cleanup = setUpScreen('/shared');
    spyOn(api, 'sharedWithMe').mockResolvedValue([
      {
        id: 'shared-doc',
        title: '공유받은 연구 계획 · Research plan',
        permission: 'read',
        shared_by: 'Second User',
        updated_at: '2026-10-26T00:00:00Z',
      },
    ]);
    return cleanup;
  },
} satisfies Meta<typeof SharedScreen>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Documents: Story = {
  play: async ({ canvasElement }) => {
    await expect(await within(canvasElement).findByRole('link')).toHaveTextContent('Research plan');
  },
};
export const NoOrganization: Story = {
  beforeEach: () => {
    const cleanup = setUpScreen('/shared');
    org.reset();
    return cleanup;
  },
};
export const NarrowDarkLargeText: Story = {
  ...Documents,
  globals: { theme: 'dark', fontScale: '200', viewport: { value: 'narrow375' } },
};
