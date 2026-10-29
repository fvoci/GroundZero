import type { Meta, StoryObj } from '@storybook/svelte';
import { expect, spyOn, userEvent, within } from 'storybook/test';
import CoursePanel from '$lib/features/courses/CoursePanel.svelte';
import { storeFormDraft } from '$lib/features/common/form-draft';
import { api } from '$lib/api/client';
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
    storeFormDraft('course:item-1:import', {
      savedAt: '2026-10-27T00:00:00Z',
      input: {
        text: '다음 강의 / Next lecture,2장,12:30',
        sentText: '',
        ids: [],
      },
    });
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await userEvent.click(await canvas.findByText(m.courses_csv_summary()));
    await userEvent.click(await canvas.findByRole('button', { name: m.common_draft_restore() }));
    await expect(canvas.getByLabelText(m.courses_csv_help())).toHaveValue(
      '다음 강의 / Next lecture,2장,12:30',
    );
    await expect(canvas.getByLabelText(m.courses_new_title())).toHaveValue('');
  },
};

/** 보관 항목은 개인 진도를 읽기만 한다. 읽기 전용 사용자의 개인 진도는 허용한다(D-113 SR1-11). */
export const Archived: Story = {
  beforeEach: () => {
    spyOn(api, 'course').mockResolvedValue({ ...course, access: 'archived' });
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
    await expect(canvas.getByLabelText(m.courses_my_note())).toHaveAttribute('readonly');
    await expect(canvas.queryByRole('button', { name: m.common_save() })).toBeNull();
  },
};
