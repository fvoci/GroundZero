import type { Meta, StoryObj } from '@storybook/svelte';
import { expect, fireEvent, screen, spyOn, userEvent } from 'storybook/test';
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
export const OutsideMemberShare: Story = {
  beforeEach: () => {
    const recipient = {
      user_id: 'outside-1',
      email: 'outside@example.test',
      display_name: 'Outside',
      recipient_role: 'outside_member' as const,
      max_permission: 'read' as const,
    };
    spyOn(api, 'shareRecipient').mockResolvedValue(recipient);
    spyOn(api, 'shares').mockResolvedValue([{ ...recipient, id: 'share-1', permission: 'write' }]);
  },
  play: async () => {
    await userEvent.click(screen.getByRole('button', { name: '공유·설정' }));
    await expect(await screen.findByText('공간 밖 조직원')).toBeInTheDocument();
    await expect(screen.getByText('공유 상한: 읽기')).toBeInTheDocument();
    await userEvent.type(screen.getByLabelText('이메일'), 'outside@example.test');
    await fireEvent.keyDown(screen.getByRole('button', { name: '공유 권한' }), {
      key: 'ArrowDown',
    });
    await expect(await screen.findByRole('option', { name: '읽기' })).toBeInTheDocument();
    await expect(screen.queryByRole('option', { name: '수정' })).toBeNull();
  },
};
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
