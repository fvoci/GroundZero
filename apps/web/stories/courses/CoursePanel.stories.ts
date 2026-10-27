import type { Meta, StoryObj } from '@storybook/svelte';
import { expect, spyOn, userEvent, within } from 'storybook/test';
import CoursePanel from '$lib/features/courses/CoursePanel.svelte';
import { storeFormDraft } from '$lib/features/common/form-draft';
import { api, newRequestId } from '$lib/api/client';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import MainFrame from '../fixtures/MainFrame.svelte';
import { course } from '../time/fixtures';
import { setUpTime } from '../time/setup';
const meta = {
  globals: { theme: 'light', fontScale: '100', viewport: { value: 'desktop' } },
  title: 'Courses/CoursePanel',
  component: CoursePanel,
  args: { itemId: 'item-1' },
  decorators: [() => ({ Component: MainFrame })],
  beforeEach: async () => {
    const restore = await setUpTime();
    me.userId = 'story-course-learner';
    return async () => {
      for (const key of Object.keys(localStorage)) {
        if (key.startsWith('fvoci:story-course-learner:')) localStorage.removeItem(key);
      }
      me.userId = '';
      await restore();
    };
  },
} satisfies Meta<typeof CoursePanel>;
export default meta;
type Story = StoryObj<typeof meta>;
export const StartEpisode: Story = {
  play: async ({ canvasElement }) => {
    await userEvent.click(
      await within(canvasElement).findByRole('button', {
        name: m.courses_track_aria({ number: 1 }),
      }),
    );
  },
};
export const NarrowDark200: Story = {
  globals: { theme: 'dark', fontScale: '200', viewport: { value: 'narrow375' } },
};

export const RestoreDraft: Story = {
  beforeEach: () => {
    storeFormDraft('course:item-1:new', {
      savedAt: '2026-10-27T00:00:00Z',
      input: {
        title: '다음 강의 / Next lecture',
        chapter: '2장',
        duration: '12:xx',
        planned_on: '2026-11-01',
        requestId: newRequestId(),
        requestPayload: '',
      },
    });
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await userEvent.click(await canvas.findByRole('button', { name: m.common_draft_restore() }));
    await expect(canvas.getByLabelText(m.courses_new_title())).toHaveValue(
      '다음 강의 / Next lecture',
    );
    await expect(canvas.getByLabelText(m.courses_duration())).toHaveValue('12:xx');
  },
};
export const RestoreDraftDark: Story = { ...RestoreDraft, globals: { theme: 'dark' } };
export const RestoreDraftNarrow: Story = {
  ...RestoreDraft,
  globals: { viewport: { value: 'narrow375' } },
};
export const RestoreDraftLarge: Story = {
  ...RestoreDraft,
  globals: { fontScale: '200', viewport: { value: 'narrow375' } },
};

/** 읽기 전용·보관 항목: 진도 단추는 없고, 내 메모·날짜는 저장 단추 없는 비활성 폼으로 볼 수 있다(D-20 ⑧, D3B-F-2). */
export const ReadOnly: Story = {
  beforeEach: () => {
    spyOn(api, 'course').mockResolvedValue({ ...course, can_write: false });
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await canvas.findByRole('button', { name: m.courses_track_aria({ number: 1 }) });
    await expect(
      canvas.queryByRole('button', { name: m.courses_mark_done_aria({ number: 1 }) }),
    ).toBeNull();
    await userEvent.click(
      canvas.getByRole('button', { name: m.courses_notes_aria({ number: 1 }) }),
    );
    await expect(canvas.getByLabelText(m.courses_my_note())).toBeDisabled();
    await expect(canvas.queryByRole('button', { name: m.common_save() })).toBeNull();
  },
};
export const ReadOnlyNarrowDark: Story = {
  ...ReadOnly,
  globals: { theme: 'dark', viewport: { value: 'narrow375' } },
};
