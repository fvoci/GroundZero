import type { Meta, StoryObj } from '@storybook/svelte';
import { expect, spyOn, within } from 'storybook/test';
import { api, NetworkError } from '$lib/api/client';
import NotificationsScreen from '$lib/features/references/NotificationsScreen.svelte';
import MainFrame from '../fixtures/MainFrame.svelte';
import { INVITATION, INVITE_NOTIFICATION } from '../fixtures/personal';
import { setUpScreen } from '../fixtures/screens';
import { notificationsView, WORKSPACE } from '../fixtures/synthetic';

function setup(fail = false): () => void {
  const notes = notificationsView().map((n, index) => ({
    ...n,
    id: `00000000-0000-4000-8000-${String(index + 1).padStart(12, '0')}`,
    read: false,
  }));
  const cleanup = setUpScreen(`/notifications/${notes[0]?.id ?? ''}`, {
    notifications: (_org, cursor) =>
      Promise.resolve({
        notifications: cursor === undefined ? notes.slice(0, 2) : notes.slice(2),
        next:
          cursor === undefined ? { before: '2026-10-26T00:00:00Z', before_id: 'cursor-1' } : null,
      }),
  });
  spyOn(api, 'markNotificationsRead').mockImplementation(() =>
    fail ? Promise.reject(new NetworkError('offline')) : Promise.resolve(1),
  );
  return cleanup;
}
const meta = {
  title: 'Screens/Notifications',
  component: NotificationsScreen,
  args: {
    workspaces: [WORKSPACE],
    onchanged: () => undefined,
    onaccepted: () => Promise.resolve(),
  },
  decorators: [() => ({ Component: MainFrame })],
  globals: { theme: 'light', fontScale: '100', viewport: { value: 'desktop' } },
  beforeEach: () => setup(),
} satisfies Meta<typeof NotificationsScreen>;
export default meta;
type Story = StoryObj<typeof meta>;
export const TwoPanes: Story = {
  play: async ({ canvasElement }) => {
    await expect(
      await within(canvasElement).findByTestId('notifications'),
    ).not.toBeEmptyDOMElement();
  },
};
export const Invitation: Story = {
  beforeEach: () => {
    const cleanup = setUpScreen(`/notifications/${INVITE_NOTIFICATION.id}`, {
      notifications: () => Promise.resolve({ notifications: [INVITE_NOTIFICATION], next: null }),
      myInvitations: () => Promise.resolve([INVITATION]),
    });
    spyOn(api, 'markNotificationsRead').mockResolvedValue(1);
    spyOn(api, 'acceptInvitation').mockResolvedValue({
      kind: 'organization',
      organization_id: INVITATION.organization_id,
      workspace_id: null,
    });
    spyOn(api, 'declineInvitation').mockResolvedValue(undefined);
    return cleanup;
  },
  play: async ({ canvasElement }) => {
    await expect(await within(canvasElement).findByRole('button', { name: '수락' })).toBeEnabled();
  },
};
export const ReadFailed: Story = {
  beforeEach: () => setup(true),
  play: async ({ canvasElement }) => {
    await expect(await within(canvasElement).findByRole('alert')).toHaveTextContent('읽음');
  },
};
export const NarrowDarkLargeText: Story = {
  ...TwoPanes,
  globals: { theme: 'dark', fontScale: '200', viewport: { value: 'narrow375' } },
};
