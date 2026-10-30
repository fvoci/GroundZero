import type { Meta, StoryObj } from '@storybook/svelte';
import { expect, spyOn, within } from 'storybook/test';

import { api } from '$lib/api/client';
import NotesScreen from '$lib/features/notes/NotesScreen.svelte';
import { me } from '$lib/me.svelte';

import MainFrame from '../fixtures/MainFrame.svelte';
import { MY_NOTES } from '../fixtures/screen-lists';
import { setUpScreen } from '../fixtures/screens';

const meta = {
  title: 'Notes/NotesScreen',
  component: NotesScreen,
  decorators: [() => ({ Component: MainFrame })],
  globals: { theme: 'light', fontScale: '100', viewport: { value: 'desktop' } },
  beforeEach: () => {
    const previousUser = me.userId;
    me.userId = 'story-user';
    const cleanup = setUpScreen('/');
    // /me/notes만 모의한다. 다른 사용자나 관리자 관점의 개인 목록은 없다.
    spyOn(api, 'myNotes').mockResolvedValue(MY_NOTES);
    spyOn(api, 'documentTemplates').mockResolvedValue([]);
    return () => {
      cleanup();
      me.userId = previousUser;
    };
  },
} satisfies Meta<typeof NotesScreen>;
export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  play: async ({ canvasElement }) => {
    const tree = within(await within(canvasElement).findByRole('tree'));
    await expect(tree.getAllByRole('treeitem')).toHaveLength(MY_NOTES.length);
  },
};
export const NarrowDarkLargeText: Story = {
  ...Default,
  globals: { theme: 'dark', fontScale: '200', viewport: { value: 'narrow375' } },
};
