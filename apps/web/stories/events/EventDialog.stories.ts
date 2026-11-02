import { eventsApi } from '$lib/features/events/api';
import type { Meta, StoryObj } from '@storybook/svelte';
import { expect, spyOn, userEvent, within } from 'storybook/test';
import EventDialog from '$lib/features/events/EventDialog.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setUpScreen } from '../fixtures/screens';
import { recurring } from '../time/fixtures';
import { me } from '$lib/me.svelte';

const meta = {
  title: 'Events/EventDialog',
  component: EventDialog,
  args: { eventId: recurring.id, onclose: () => undefined, onchanged: () => undefined },
  beforeEach: () => {
    const previousUser = me.userId;
    me.userId = 'event-story-user';
    for (const key of Object.keys(localStorage))
      if (key.startsWith('fvoci:event-story-user:-:form-draft:')) localStorage.removeItem(key);
    const restore = setUpScreen('/events');
    spyOn(eventsApi, 'event').mockResolvedValue({
      ...recurring,
      viewer_id: me.userId,
      archived: false,
    });
    spyOn(eventsApi, 'updateEvent').mockResolvedValue({
      kind: 'conflict',
      current: {
        ...recurring,
        title: '다른 탭에서 바꾼 제목',
        description: '서버 설명\n'.repeat(40),
        version: recurring.version + 1,
      },
    });
    return () => {
      restore();
      me.userId = previousUser;
    };
  },
  play: async ({ canvasElement }) => {
    const page = within(canvasElement.ownerDocument.body);
    await userEvent.clear(await page.findByLabelText(m.events_title()));
    await userEvent.type(page.getByLabelText(m.events_title()), '내가 바꾼 제목');
    await userEvent.type(page.getByLabelText(m.events_description()), '긴 설명\n'.repeat(40));
    await userEvent.click(page.getByRole('button', { name: m.common_save() }));
    const panel = await page.findByRole('region', { name: m.common_conflict_heading() });
    await expect(
      within(panel).getByRole('radio', { name: '제목: 내 입력 사용' }),
    ).not.toBeChecked();
    await expect(within(panel).getByRole('textbox', { name: '설명: 내 입력' })).toHaveValue(
      '긴 설명\n'.repeat(40),
    );
  },
} satisfies Meta<typeof EventDialog>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Default: Story = {
  globals: { theme: 'light', fontScale: '100', viewport: { value: 'desktop', isRotated: false } },
};
export const Dark375Font200: Story = {
  globals: { theme: 'dark', fontScale: '200', viewport: { value: 'narrow375', isRotated: false } },
};
