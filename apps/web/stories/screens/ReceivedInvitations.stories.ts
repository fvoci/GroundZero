import type { Meta, StoryObj } from '@storybook/svelte';
import { spyOn } from 'storybook/test';
import { api } from '$lib/api/client';
import ReceivedInvitations from '$lib/features/members/ReceivedInvitations.svelte';
import MainFrame from '../fixtures/MainFrame.svelte';
import { INVITATION } from '../fixtures/personal';
import { setUpScreen } from '../fixtures/screens';
const meta = {
  title: 'Screens/ReceivedInvitations',
  component: ReceivedInvitations,
  args: { onaccepted: () => Promise.resolve() },
  decorators: [() => ({ Component: MainFrame })],
  globals: { theme: 'light', fontScale: '100', viewport: { value: 'desktop' } },
  beforeEach: () => {
    const cleanup = setUpScreen('/', {
      myInvitations: () =>
        Promise.resolve([
          INVITATION,
          {
            ...INVITATION,
            kind: 'workspace',
            id: 'workspace-invite',
            workspace_id: 'new-workspace',
            workspace_name: '공동 연구실 · Shared laboratory',
          },
        ]),
    });
    spyOn(api, 'acceptInvitation').mockResolvedValue({
      kind: 'organization',
      organization_id: INVITATION.organization_id,
      workspace_id: null,
    });
    spyOn(api, 'declineInvitation').mockResolvedValue(undefined);
    return cleanup;
  },
} satisfies Meta<typeof ReceivedInvitations>;
export default meta;
type Story = StoryObj<typeof meta>;
export const BothKinds: Story = {};
export const Dark: Story = { globals: { theme: 'dark' } };
export const Narrow375: Story = { globals: { viewport: { value: 'narrow375' } } };
export const LargeText: Story = { globals: { fontScale: '200', viewport: { value: 'narrow375' } } };
