import type { Meta, StoryObj } from '@storybook/svelte';
import { expect, spyOn, userEvent, within } from 'storybook/test';
import { api, ApiError } from '$lib/api/client';
import EventDialog from '$lib/features/events/EventDialog.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setUpScreen } from '../fixtures/screens';
import { recurring } from '../time/fixtures';
import { me } from '$lib/me.svelte';
import { storeFormDraft } from '$lib/features/common/form-draft';

// 일정 대화상자(features/events/EventDialog.svelte). 기본은 반복 일정이고 저장이 반복 끝 위치 오류(#76)로 거절되는 상태다.
// Light·Dark·NarrowLargeText는 반복 없는 회의 일정의 기본 모양(밝음·어두움·375px·200% 글꼴)이다.
const meta = {
  globals: { theme: 'light', fontScale: '100', viewport: { value: 'desktop' } },
  title: 'Events/EventDialog',
  component: EventDialog,
  args: { eventId: recurring.id, onclose: () => undefined, onchanged: () => undefined },
  beforeEach: () => {
    const previousUser = me.userId;
    me.userId = 'event-story-user';
    for (const key of Object.keys(localStorage)) {
      if (key.startsWith('fvoci:event-story-user:-:form-draft:')) localStorage.removeItem(key);
    }
    const restore = setUpScreen('/events');
    spyOn(api, 'event').mockResolvedValue(recurring);
    spyOn(api, 'updateEvent').mockRejectedValue(
      new ApiError(400, 'bad_request', 'until must not be before the first occurrence'),
    );
    return () => {
      restore();
      me.userId = previousUser;
    };
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

/** 반복 없는 회의 일정(반복 칸이 비어 있는 기본 모양 확인용). meta의 반복 일정 대신 이 일정을 돌려준다. */
const plainMeeting = (): void => {
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
};

export const Light: Story = {
  args: { eventId: 'event-example' },
  beforeEach: plainMeeting,
  globals: { theme: 'light', viewport: { value: 'desktop' } },
};
export const Dark: Story = {
  args: { eventId: 'event-example' },
  beforeEach: plainMeeting,
  globals: { theme: 'dark', viewport: { value: 'desktop' } },
};
export const NarrowLargeText: Story = {
  args: { eventId: 'event-example' },
  beforeEach: plainMeeting,
  globals: { theme: 'dark', viewport: { value: 'narrow375' }, fontScale: '200' },
};

/** Synthetic unfinished recurrence text; restoring never submits it. */
export const DraftAvailable: Story = {
  beforeEach: () => {
    const baseFields = {
      freq: 'weekly',
      interval: '1',
      weekdays: [1],
      end: 'never',
      until: '',
      count: '10',
    };
    storeFormDraft(`event-recurrence:${recurring.id}`, {
      savedAt: '2026-10-26T12:00:00Z',
      input: {
        fields: { ...baseFields, interval: '아직 입력 중' },
        baseFields,
        base: recurring.recurrence,
      },
    });
    return () => {
      for (const key of Object.keys(localStorage)) {
        if (key.startsWith(`fvoci:event-story-user:-:form-draft:event-recurrence:${recurring.id}:`))
          localStorage.removeItem(key);
      }
    };
  },
  play: async ({ canvasElement }) => {
    const page = within(canvasElement.ownerDocument.body);
    await page.findAllByRole('button', { name: m.common_draft_restore() });
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
