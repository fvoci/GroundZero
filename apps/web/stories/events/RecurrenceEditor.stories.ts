import type { Meta, StoryObj } from '@storybook/svelte';
import { userEvent, within } from 'storybook/test';
import { ApiError } from '$lib/api/client';
import RecurrenceEditor from '$lib/features/events/RecurrenceEditor.svelte';
import MainFrame from '../fixtures/MainFrame.svelte';
import { recurring } from '../time/fixtures';
import { m } from '$lib/paraglide/messages.js';
const meta = {
  globals: { theme: 'light', fontScale: '100', viewport: { value: 'desktop' } },
  title: 'Events/RecurrenceEditor',
  component: RecurrenceEditor,
  args: {
    idPrefix: 'rule',
    rule: recurring.recurrence,
    canEdit: true,
    zoneNote: 'Asia/Seoul',
    onsave: () => Promise.resolve(),
  },
  decorators: [() => ({ Component: MainFrame })],
} satisfies Meta<typeof RecurrenceEditor>;
export default meta;
type Story = StoryObj<typeof meta>;
export const UntilError: Story = {
  args: {
    onsave: () =>
      Promise.reject(
        new ApiError(
          400,
          'bad_request',
          'until must not be before the first occurrence',
          undefined,
          'until_before_first_occurrence',
          'until',
        ),
      ),
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await userEvent.click(canvas.getByRole('button', { name: m.events_recurrence_save() }));
    await canvas.findByText(m.events_until_before_start());
  },
};
export const NarrowDark200: Story = {
  ...UntilError,
  globals: { theme: 'dark', fontScale: '200', viewport: { value: 'narrow375' } },
};
