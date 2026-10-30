import type { Meta, StoryObj } from '@storybook/svelte';
import { expect, spyOn, within } from 'storybook/test';

import { api } from '$lib/api/client';
import TrashScreen from '$lib/features/trash/TrashScreen.svelte';

import MainFrame from '../fixtures/MainFrame.svelte';
import { TRASH } from '../fixtures/screen-lists';
import { setUpScreen } from '../fixtures/screens';
import { WORKSPACE } from '../fixtures/synthetic';

const meta = {
  title: 'Trash/TrashScreen',
  component: TrashScreen,
  args: { workspace: WORKSPACE },
  decorators: [() => ({ Component: MainFrame })],
  globals: { theme: 'light', fontScale: '100', viewport: { value: 'desktop' } },
  beforeEach: () => {
    const cleanup = setUpScreen('/');
    spyOn(api, 'trash').mockResolvedValue(TRASH);
    return cleanup;
  },
} satisfies Meta<typeof TrashScreen>;
export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  play: async ({ canvasElement }) => {
    const list = within(await within(canvasElement).findByTestId('trash-list'));
    await expect(await list.findAllByRole('listitem')).toHaveLength(TRASH.entries.length);
  },
};
export const NarrowDarkLargeText: Story = {
  ...Default,
  globals: { theme: 'dark', fontScale: '200', viewport: { value: 'narrow375' } },
};
