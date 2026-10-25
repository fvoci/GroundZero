import type { Meta, StoryObj } from '@storybook/svelte';
import { spyOn } from 'storybook/test';
import { api } from '$lib/api/client';
import EventDialog from '$lib/features/events/EventDialog.svelte';

const meta = {
  title: 'Events/EventDialog',
  component: EventDialog,
  args: { eventId: 'event-example', onclose: () => undefined, onchanged: () => undefined },
  beforeEach: () => {
    spyOn(api, 'event').mockResolvedValue({
      id: 'event-example',
      item_id: 'item-example',
      workspace_id: 'ws-1',
      kind: 'meeting',
      title: '공유 정책 점검 회의',
      description: '문서와 첨부의 접근 범위를 확인합니다.',
      all_day: false,
      start_date: null,
      end_date: null,
      starts_at: '2026-10-26T01:00:00Z',
      ends_at: '2026-10-26T02:00:00Z',
      timezone: 'Asia/Seoul',
      recurrence: null,
      excluded_dates: [],
      version: 1,
      access: 'write',
    });
  },
} satisfies Meta<typeof EventDialog>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Light: Story = { globals: { theme: 'light', viewport: { value: 'desktop' } } };
export const Dark: Story = { globals: { theme: 'dark', viewport: { value: 'desktop' } } };
export const NarrowLargeText: Story = {
  globals: { theme: 'dark', viewport: { value: 'narrow375' }, fontScale: '200' },
};
