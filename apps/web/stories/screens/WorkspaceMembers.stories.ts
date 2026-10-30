import type { Meta, StoryObj } from '@storybook/svelte';
import { expect, userEvent, within } from 'storybook/test';
import MembersScreen from '$lib/features/members/MembersScreen.svelte';
import MainFrame from '../fixtures/MainFrame.svelte';
import { setUpWorkspaceManagement } from '../fixtures/workspace-management';
import { LIMITS_WORKSPACE } from '../fixtures/workspace-limits';

const meta = {
  title: 'Screens/WorkspaceMembers',
  component: MembersScreen,
  args: {
    workspace: LIMITS_WORKSPACE,
    tab: 'members',
    meId: 'story-user',
    onchanged: () => undefined,
  },
  decorators: [() => ({ Component: MainFrame })],
  beforeEach: () => setUpWorkspaceManagement('members'),
  globals: { theme: 'light', fontScale: '100', viewport: { value: 'desktop' } },
} satisfies Meta<typeof MembersScreen>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Default: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(
      within(await canvas.findByTestId('member-list')).getAllByRole('listitem'),
    ).toHaveLength(50);
    await expect(canvas.getByRole('button', { name: '역할별 공유 상한' })).toHaveAttribute(
      'aria-expanded',
      'false',
    );
    await expect(canvas.getAllByRole('tab')).toHaveLength(4);
    await expect(canvas.getByRole('tab', { name: '멤버 50명' })).toBeVisible();
    const filter = canvas.getByRole('searchbox', { name: '이름·이메일로 멤버 검색' });
    const status = canvas.getByTestId('workspace-member-search-status');
    await expect(status).toHaveAttribute('role', 'status');
    await expect(status.textContent).toBe('');
    await userEvent.type(filter, '없는 이름');
    await expect(canvas.getByTestId('workspace-member-search-status')).toBe(status);
    await expect(status).toHaveTextContent('검색 조건에 맞는 멤버가 없습니다.');
    await expect(status).toBeVisible();
    await userEvent.clear(filter);
    await expect(canvas.getByTestId('workspace-member-search-status')).toBe(status);
    await expect(status.textContent).toBe('');
    await userEvent.type(filter, 'research-collaborator-50.');
    await expect(within(canvas.getByTestId('member-list')).getAllByRole('listitem')).toHaveLength(
      1,
    );
    await expect(canvas.getByRole('tab', { name: '멤버 50명' })).toBeVisible();
    await userEvent.clear(filter);
    await expect(within(canvas.getByTestId('member-list')).getAllByRole('listitem')).toHaveLength(
      50,
    );
  },
};
export const NarrowDarkLargeText: Story = {
  ...Default,
  globals: { theme: 'dark', fontScale: '200', viewport: { value: 'narrow375' } },
};
