import type { Meta, StoryObj } from '@storybook/svelte';
import { expect, spyOn, within } from 'storybook/test';

import { activityApi } from '$lib/features/activity/api';
import AuditScreen from '$lib/features/audit/AuditScreen.svelte';

import MainFrame from '../fixtures/MainFrame.svelte';
import { AUDIT } from '../fixtures/screen-lists';
import { setUpScreen } from '../fixtures/screens';
import { WORKSPACE } from '../fixtures/synthetic';

const meta = {
  title: 'Audit/AuditScreen',
  component: AuditScreen,
  args: { workspace: WORKSPACE },
  decorators: [() => ({ Component: MainFrame })],
  globals: { theme: 'light', fontScale: '100', viewport: { value: 'desktop' } },
  beforeEach: () => {
    const cleanup = setUpScreen('/');
    spyOn(activityApi, 'audit').mockResolvedValue(AUDIT);
    return cleanup;
  },
} satisfies Meta<typeof AuditScreen>;
export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  play: async ({ canvasElement }) => {
    const list = within(await within(canvasElement).findByTestId('audit-list'));
    await expect(await list.findAllByRole('listitem')).toHaveLength(AUDIT.length);
  },
};
export const NarrowDarkLargeText: Story = {
  ...Default,
  globals: { theme: 'dark', fontScale: '200', viewport: { value: 'narrow375' } },
};
