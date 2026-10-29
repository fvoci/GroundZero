import { activityApi } from '$lib/features/activity/api';
import type { Meta, StoryObj } from '@storybook/svelte';
import { spyOn } from 'storybook/test';
import { api } from '$lib/api/client';
import DocumentScreen from '$lib/features/documents/DocumentScreen.svelte';
import MainFrame from '../fixtures/MainFrame.svelte';
import { ACCESS_DOCUMENT, ACCESS_FILE } from '../fixtures/access';
import { THREAD } from '../fixtures/comments';
import { setUpScreen } from '../fixtures/screens';

const meta = {
  title: 'Documents/DocumentScreen',
  component: DocumentScreen,
  args: { documentId: ACCESS_DOCUMENT.id, workspaces: [] },
  decorators: [() => ({ Component: MainFrame })],
  beforeEach: () => {
    const cleanup = setUpScreen('/');
    spyOn(api, 'document').mockResolvedValue(ACCESS_DOCUMENT);
    spyOn(api, 'documentFiles').mockResolvedValue([ACCESS_FILE]);
    spyOn(api, 'documentBacklinks').mockResolvedValue([]);
    spyOn(activityApi, 'comments').mockResolvedValue(THREAD);
    return cleanup;
  },
} satisfies Meta<typeof DocumentScreen>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Owner: Story = {};
export const NarrowDarkLargeText: Story = {
  globals: { theme: 'dark', fontScale: '200', viewport: { value: 'narrow375' } },
};
