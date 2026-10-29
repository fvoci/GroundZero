import type { Meta, StoryObj } from '@storybook/svelte';
import { expect, screen, spyOn, userEvent } from 'storybook/test';
import { api } from '$lib/api/client';
import DocumentActions from '$lib/features/documents/DocumentActions.svelte';
import { ACCESS_DOCUMENT } from '../fixtures/access';
import { setUpScreen } from '../fixtures/screens';

const meta = {
  title: 'Documents/AccessAndSharing',
  component: DocumentActions,
  args: { doc: ACCESS_DOCUMENT, workspaces: [], onchanged: () => undefined },
  beforeEach: () => {
    const cleanup = setUpScreen('/');
    spyOn(api, 'document').mockResolvedValue(ACCESS_DOCUMENT);
    spyOn(api, 'shares').mockResolvedValue([]);
    spyOn(api, 'publicLinks').mockResolvedValue([]);
    return cleanup;
  },
} satisfies Meta<typeof DocumentActions>;
export default meta;
type Story = StoryObj<typeof meta>;

export const Owner: Story = {};
/** D-108: 기본과 좁은 폭·어두움·큰 글꼴을 합친 화면만 둔다. */
export const ArchivedItemAdminNarrowDarkLargeText: Story = {
  globals: { theme: 'dark', viewport: { value: 'narrow375' }, fontScale: '200' },
  args: {
    doc: {
      ...ACCESS_DOCUMENT,
      access: 'read',
      can_edit: false,
      can_manage: true,
      can_delete: false,
    },
  },
  play: async () => {
    await userEvent.click(screen.getByRole('button', { name: '공유·설정' }));
    await expect(await screen.findByLabelText('이메일')).toBeEnabled();
    await expect(screen.getByRole('button', { name: '공개 링크 만들기' })).toBeInTheDocument();
  },
};
