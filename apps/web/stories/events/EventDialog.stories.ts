import type { Meta, StoryObj } from '@storybook/svelte';
import { expect, spyOn, userEvent, within } from 'storybook/test';
import { api, ApiError } from '$lib/api/client';
import EventDialog from '$lib/features/events/EventDialog.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setUpScreen } from '../fixtures/screens';
import { recurring } from '../time/fixtures';

const meta = {
  globals: { theme: 'light', fontScale: '100', viewport: { value: 'desktop' } },
  title: 'Events/EventDialog',
  component: EventDialog,
  args: { eventId: recurring.id, onclose: () => undefined, onchanged: () => undefined },
  beforeEach: () => {
    const restore = setUpScreen('/events');
    spyOn(api, 'event').mockResolvedValue(recurring);
    spyOn(api, 'updateEvent').mockRejectedValue(
      new ApiError(400, 'bad_request', 'until must not be before the first occurrence'),
    );
    return restore;
  },
} satisfies Meta<typeof EventDialog>;
export default meta;
type Story = StoryObj<typeof meta>;
export const UntilRejected: Story = {
  play: async ({ canvasElement }) => {
    const page = within(canvasElement.ownerDocument.body);
    const title = await page.findByLabelText(m.events_title());
    await userEvent.type(title, ' 변경');
    await userEvent.click(page.getByRole('button', { name: m.common_save() }));
    const error = await page.findByText(m.events_until_before_start());
    await expect(page.getByLabelText(m.events_end_until_aria())).toHaveAttribute(
      'aria-describedby',
      error.id,
    );
  },
};
export const NarrowDark200: Story = {
  ...UntilRejected,
  globals: { theme: 'dark', fontScale: '200', viewport: { value: 'narrow375' } },
};
