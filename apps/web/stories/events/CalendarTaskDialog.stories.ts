import type { Meta, StoryObj } from '@storybook/svelte';
import { within } from 'storybook/test';
import CalendarTaskDialog from '$lib/features/events/CalendarTaskDialog.svelte';
import { storeFormDraft } from '$lib/features/common/form-draft';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';

const meta = {
  title: 'Events/CalendarTaskDialog',
  component: CalendarTaskDialog,
  globals: { theme: 'light', fontScale: '100', viewport: { value: 'desktop' } },
  args: {
    workspaceId: 'calendar-story-workspace',
    day: '2026-10-26',
    item: { id: 'calendar-story-item', title: '발표 준비' },
    onclose: () => undefined,
    oncreated: () => undefined,
  },
  beforeEach: () => {
    const previous = me.userId;
    me.userId = 'calendar-story-user';
    storeFormDraft('calendar-task:calendar-story-workspace:calendar-story-item:2026-10-26', {
      savedAt: '2026-10-26T12:00:00Z',
      input: {
        title: '아직 저장하지 않은 발표 자료',
        date: '2026-10-27',
        module: '',
        item: 'calendar-story-item',
        retry: null,
      },
    });
    return () => {
      me.userId = previous;
      for (const key of Object.keys(localStorage)) {
        if (key.startsWith('fvoci:calendar-story-user:-:form-draft:calendar-task:'))
          localStorage.removeItem(key);
      }
    };
  },
} satisfies Meta<typeof CalendarTaskDialog>;
export default meta;
type Story = StoryObj<typeof meta>;
export const DraftAvailable: Story = {
  play: async ({ canvasElement }) => {
    await within(canvasElement.ownerDocument.body).findByRole('button', {
      name: m.common_draft_restore(),
    });
  },
};
export const DraftAvailableDark: Story = {
  ...DraftAvailable,
  globals: { theme: 'dark', fontScale: '100', viewport: { value: 'desktop' } },
};
export const DraftAvailableNarrow200: Story = {
  ...DraftAvailable,
  globals: { theme: 'dark', fontScale: '200', viewport: { value: 'narrow375' } },
};
