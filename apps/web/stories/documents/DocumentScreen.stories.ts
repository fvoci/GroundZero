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
    spyOn(api, 'comments').mockResolvedValue(THREAD);
    return cleanup;
  },
} satisfies Meta<typeof DocumentScreen>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Owner: Story = { globals: { theme: 'light', viewport: { value: 'desktop' } } };
export const Writer: Story = {
  beforeEach: () => {
    spyOn(api, 'document').mockResolvedValue({
      ...ACCESS_DOCUMENT,
      access: 'write',
      can_manage: false,
      can_delete: false,
      can_export: false,
    });
  },
  globals: { theme: 'dark', viewport: { value: 'desktop' } },
};
export const ReadOnly: Story = {
  beforeEach: () => {
    spyOn(api, 'document').mockResolvedValue({
      ...ACCESS_DOCUMENT,
      access: 'read',
      can_edit: false,
      can_manage: false,
      can_delete: false,
      can_export: false,
    });
  },
  globals: { theme: 'dark', viewport: { value: 'narrow375' }, fontScale: '200' },
};
