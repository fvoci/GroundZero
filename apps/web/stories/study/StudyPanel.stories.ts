import type { Meta, StoryObj } from '@storybook/svelte';
import { expect, spyOn, userEvent, within } from 'storybook/test';
import StudyPanel from '$lib/features/study/StudyPanel.svelte';
import { storeFormDraft } from '$lib/features/common/form-draft';
import { api, newRequestId } from '$lib/api/client';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import MainFrame from '../fixtures/MainFrame.svelte';
import { study } from '../time/fixtures';
import { setUpTime } from '../time/setup';
const meta = {
  globals: { theme: 'light', fontScale: '100', viewport: { value: 'desktop' } },
  title: 'Study/StudyPanel',
  component: StudyPanel,
  args: { itemId: 'item-1' },
  decorators: [() => ({ Component: MainFrame })],
  beforeEach: async () => {
    const restore = await setUpTime();
    me.userId = 'story-study-learner';
    return async () => {
      for (const key of Object.keys(localStorage)) {
        if (key.startsWith('fvoci:story-study-learner:')) localStorage.removeItem(key);
      }
      me.userId = '';
      await restore();
    };
  },
} satisfies Meta<typeof StudyPanel>;
export default meta;
type Story = StoryObj<typeof meta>;
export const StartUnit: Story = {
  play: async ({ canvasElement }) => {
    await userEvent.click(
      await within(canvasElement).findByRole('button', {
        name: m.study_track_aria({ title: '연습 문제' }),
      }),
    );
  },
};
export const NarrowDark200: Story = {
  globals: { theme: 'dark', fontScale: '200', viewport: { value: 'narrow375' } },
};

export const RestoreDraft: Story = {
  beforeEach: () => {
    storeFormDraft('study:item-1:new', {
      savedAt: '2026-10-27T00:00:00Z',
      input: {
        title: '다음 학습 / Next unit',
        chapter: '2장',
        amount: '12oops',
        unit: 'pages',
        planned_on: '2026-11-01',
        requestId: newRequestId(),
        requestPayload: '',
      },
    });
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await userEvent.click(await canvas.findByRole('button', { name: m.common_draft_restore() }));
    await expect(canvas.getByLabelText(m.study_new_title())).toHaveValue('다음 학습 / Next unit');
    await expect(canvas.getByLabelText(m.study_amount())).toHaveValue('12oops');
  },
};

/** 보관 항목은 개인 진도를 읽기만 한다. 읽기 전용 사용자의 개인 진도는 허용한다(D-113 SR1-11). */
export const Archived: Story = {
  beforeEach: () => {
    // 지난 계획일 단위를 둬, 쓸 수 있었다면 다시 배정 폼이 보일 상태에서 그 부재를 본다(R5 B 최종 FC-14).
    spyOn(api, 'studyPlan').mockResolvedValue({
      ...study,
      access: 'archived',
      units: study.units.map((u) => ({ ...u, planned_on: '2020-01-01' })),
    });
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await canvas.findByRole('button', { name: m.study_track_aria({ title: '연습 문제' }) });
    await expect(canvas.queryByRole('form', { name: m.study_replan_form() })).toBeNull();
    await userEvent.click(canvas.getByRole('button', { name: m.study_mine() }));
    await expect(canvas.getByLabelText(m.study_note())).toHaveAttribute('readonly');
    await expect(canvas.queryByRole('button', { name: m.common_save() })).toBeNull();
  },
};
