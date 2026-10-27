import type { Meta, StoryObj } from '@storybook/svelte';
import { spyOn, screen, userEvent } from 'storybook/test';
import { api, ApiError } from '$lib/api/client';
import ItemProfilePanel from '$lib/features/modules/ItemProfilePanel.svelte';
import MainFrame from '../fixtures/MainFrame.svelte';
import { TASK_ITEM } from '../fixtures/synthetic';
import { me } from '$lib/me.svelte';

const meta = {
  title: 'Screens/ItemProfilePanel',
  component: ItemProfilePanel,
  args: { item: TASK_ITEM, kind: 'project', onchanged: () => undefined },
  decorators: [() => ({ Component: MainFrame })],
  beforeEach: () => {
    const previousUser = me.userId;
    me.userId = 'story-profile-user';
    spyOn(api, 'itemProfile').mockResolvedValue({
      leader: null,
      goal: '정확한 권한 표시',
      scope: '공유와 공개 링크',
      starts_on: null,
      ends_on: null,
      status: 'active',
      details: {},
      members: [],
      groups: [],
      can_edit: true,
      can_manage_members: false,
      version: 1,
    });
    spyOn(api, 'assignable').mockResolvedValue([]);
    spyOn(api, 'setItemProfile').mockRejectedValue(new ApiError(403, 'forbidden', 'forbidden'));
    return () => {
      me.userId = previousUser;
    };
  },
} satisfies Meta<typeof ItemProfilePanel>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Light: Story = { globals: { theme: 'light', viewport: { value: 'desktop' } } };
export const Dark: Story = { globals: { theme: 'dark', viewport: { value: 'desktop' } } };
export const FailedEdit: Story = {
  globals: { theme: 'dark', viewport: { value: 'narrow375' }, fontScale: '200' },
  play: async () => {
    await userEvent.click(await screen.findByRole('button', { name: '고치기' }));
    await userEvent.click(screen.getByRole('button', { name: '저장' }));
    await screen.findByRole('alert');
  },
};
