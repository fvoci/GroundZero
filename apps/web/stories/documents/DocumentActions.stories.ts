import type { Meta, StoryObj } from '@storybook/svelte';
import { expect, screen, spyOn, userEvent } from 'storybook/test';
import { api } from '$lib/api/client';
import DocumentActions from '$lib/features/documents/DocumentActions.svelte';
import { org } from '$lib/org.svelte';
import { ACCESS_DOCUMENT } from '../fixtures/access';
import { ORGANIZATION } from '../fixtures/synthetic';
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
export const WriterCopyDisabled: Story = {
  args: {
    doc: {
      ...ACCESS_DOCUMENT,
      access: 'write',
      can_manage: false,
      can_delete: false,
      can_export: false,
    },
  },
};
export const ReadOnly: Story = {
  args: {
    doc: {
      ...ACCESS_DOCUMENT,
      access: 'read',
      can_edit: false,
      can_manage: false,
      can_delete: false,
      can_export: false,
    },
  },
};
export const OrganizationLinksDisabled: Story = {
  // 조직 정책은 문서의 워크스페이스(ws-1)가 어느 조직인지 알아야 닿는다(R5 B 최종 FB-1).
  args: {
    workspaces: [
      {
        access: 'owner',
        id: ACCESS_DOCUMENT.workspace_id ?? 'ws-1',
        organization_id: ORGANIZATION.id,
        name: '운영',
        role: 'admin',
        is_owner: true,
        realtime_enabled: false,
      },
    ],
  },
  beforeEach: () => {
    org.organizations = org.organizations.map((o) => ({ ...o, public_links_enabled: false }));
  },
  play: async () => {
    await userEvent.click(screen.getByRole('button', { name: '공유·설정' }));
    await expect(await screen.findByText('조직이 공개 링크를 껐습니다.')).toBeInTheDocument();
    await expect(screen.queryByRole('button', { name: '공개 링크 만들기' })).toBeNull();
  },
};
/** 보관된 항목의 문서를 연 워크스페이스 관리자: 등급은 read지만 공유·공개 링크·설정을 넓히는 변경까지 할 수 있다(D-113 ⑯). */
export const ArchivedItemAdmin: Story = {
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
export const ArchivedItemAdminNarrowDark: Story = {
  ...ArchivedItemAdmin,
  globals: { theme: 'dark', viewport: { value: 'narrow375' }, fontScale: '200' },
};
