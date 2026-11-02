import { taskData as work } from '$lib/features/tasks/detail-data';
import type { Meta, StoryObj } from '@storybook/svelte';
import { expect, spyOn, userEvent, within } from 'storybook/test';
import { api, type ItemProfileView } from '$lib/api/client';
import ItemProfilePanel from '$lib/features/modules/ItemProfilePanel.svelte';
import { m } from '$lib/paraglide/messages.js';
import { me } from '$lib/me.svelte';
import MainFrame from '../fixtures/MainFrame.svelte';
import { TASK_ITEM } from '../fixtures/synthetic';

// 항목 프로필 저장의 판 충돌(D-94 개정, D-104): 첫 409에서 공통 비교 부품으로 칸마다 고른다. 연구 항목은 긴 글 칸이 많아
// 전문 비교·접힌 서버 변경·기간 묶음을 함께 본다. 기본 하나와 375px·어두움·200% 글꼴을 합친 하나(D-108).

const base: ItemProfileView = {
  leader: null,
  goal: '정확한 권한 표시',
  scope: '공유와 공개 링크',
  starts_on: '2026-11-01',
  ends_on: '2026-11-30',
  status: 'active',
  details: { question: '처음 연구 질문', keywords: ['권한', '공유'] },
  members: [],
  groups: [],
  can_edit: true,
  can_manage_members: false,
  version: 3,
};

const meta = {
  title: 'Screens/ItemProfileConflict',
  component: ItemProfilePanel,
  args: { item: TASK_ITEM, kind: 'research', onchanged: () => undefined },
  decorators: [() => ({ Component: MainFrame })],
  beforeEach: () => {
    const previousUser = me.userId;
    me.userId = 'story-profile-conflict-user';
    for (const key of Object.keys(localStorage))
      if (key.includes('story-profile-conflict-user')) localStorage.removeItem(key);
    spyOn(api, 'itemProfile').mockResolvedValue(base);
    spyOn(work, 'assignable').mockResolvedValue([]);
    spyOn(api, 'setItemProfile').mockResolvedValue({
      kind: 'conflict',
      current: {
        ...base,
        leader: { user_id: 'story-leader', display_name: '다른 탭의 책임자' },
        goal: '다른 탭에서 바꾼 목표',
        starts_on: '2026-11-05',
        details: { ...base.details, question: '다른 탭에서 고친 연구 질문\n'.repeat(12) },
        version: base.version + 1,
      },
    });
    return () => {
      me.userId = previousUser;
    };
  },
  play: async ({ canvasElement }) => {
    const page = within(canvasElement.ownerDocument.body);
    await userEvent.click(await page.findByRole('button', { name: m.modules_edit() }));
    const goal = page.getByLabelText(m.modules_goal());
    await userEvent.clear(goal);
    await userEvent.type(goal, '내가 바꾼 목표');
    await userEvent.type(page.getByLabelText(m.modules_field_question()), '\n내가 덧붙인 문단');
    await userEvent.click(page.getByRole('button', { name: m.common_save() }));
    const panel = await page.findByRole('region', { name: m.common_conflict_heading() });
    // 양쪽이 바꾼 목표·연구 질문은 미선택, 서버만 바꾼 리더·기간은 접힌 목록이다.
    await expect(
      within(panel).getByRole('radio', {
        name: `${m.modules_goal()}: ${m.common_conflict_use_mine()}`,
      }),
    ).not.toBeChecked();
    await expect(
      within(panel).getByText(m.common_conflict_server_changes({ count: 2 })),
    ).toBeVisible();
    await expect(
      within(panel).getByRole('textbox', {
        name: `${m.modules_field_question()}: ${m.common_conflict_server()}`,
      }),
    ).toHaveValue('다른 탭에서 고친 연구 질문\n'.repeat(12));
  },
} satisfies Meta<typeof ItemProfilePanel>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Default: Story = {
  globals: { theme: 'light', fontScale: '100', viewport: { value: 'desktop', isRotated: false } },
};
export const Dark375Font200: Story = {
  globals: { theme: 'dark', fontScale: '200', viewport: { value: 'narrow375', isRotated: false } },
};
