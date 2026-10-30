import type { Meta, StoryObj } from '@storybook/svelte';
import { expect, spyOn, userEvent, within } from 'storybook/test';

import { api } from '$lib/api/client';
import MembersScreen from '$lib/features/members/MembersScreen.svelte';
import { workspaceLimitsApi } from '$lib/features/members/limits-api';
import { me } from '$lib/me.svelte';

import MainFrame from '../fixtures/MainFrame.svelte';
import { setUpScreen } from '../fixtures/screens';
import {
  LIMITS_MEMBERS,
  LIMITS_ORGANIZATION,
  LIMITS_WORKSPACE,
  WORKSPACE_LIMITS,
} from '../fixtures/workspace-limits';

const meta = {
  title: 'Screens/WorkspaceLimits',
  component: MembersScreen,
  args: { workspace: LIMITS_WORKSPACE, meId: 'story-user', onchanged: () => undefined },
  decorators: [() => ({ Component: MainFrame })],
  globals: { theme: 'light', fontScale: '100', viewport: { value: 'desktop' } },
  beforeEach: () => {
    const cleanup = setUpScreen(`/w/${LIMITS_WORKSPACE.id}/members`, {
      members: () => Promise.resolve(LIMITS_MEMBERS),
    });
    me.userId = 'story-user';
    spyOn(api, 'groups').mockResolvedValue([]);
    spyOn(api, 'invitations').mockResolvedValue([]);
    spyOn(api, 'organization').mockResolvedValue(LIMITS_ORGANIZATION);
    let saved = { ...WORKSPACE_LIMITS };
    spyOn(workspaceLimitsApi, 'get').mockImplementation(() => Promise.resolve(saved));
    spyOn(workspaceLimitsApi, 'set').mockImplementation((_id, patch) => {
      saved = { ...saved, ...patch };
      return Promise.resolve(saved);
    });
    return cleanup;
  },
} satisfies Meta<typeof MembersScreen>;
export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(await canvas.findByLabelText('활동 보존 일수', { exact: true })).toHaveValue('60');
    await expect(within(canvas.getByTestId('member-list')).getAllByRole('listitem')).toHaveLength(
      50,
    );
    await expect(canvas.getByRole('heading', { name: '멤버 50명' })).toBeVisible();
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
    await userEvent.clear(filter);
    await expect(within(canvas.getByTestId('member-list')).getAllByRole('listitem')).toHaveLength(
      50,
    );
    await expect(canvas.getAllByRole('checkbox')).toHaveLength(5);
  },
};
export const NarrowDarkLargeText: Story = {
  ...Default,
  globals: { theme: 'dark', fontScale: '200', viewport: { value: 'narrow375' } },
};
