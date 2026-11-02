import { api } from '$lib/api/client';
import { activityApi } from '$lib/features/activity/api';
import type { Meta, StoryObj } from '@storybook/svelte';
import { expect, spyOn, userEvent, within } from 'storybook/test';

import ItemScreen from '$lib/features/modules/ItemScreen.svelte';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import { itemGroupPath, router } from '$lib/router.svelte';

import MainFrame from '../fixtures/MainFrame.svelte';
import { ACTIVITY } from '../fixtures/personal';
import { DOCS, TASK_ITEM, WORKSPACE } from '../fixtures/synthetic';
import { setUpTime } from '../time/setup';

// 항목 정보(제목·설명·공개 범위) 저장의 판 충돌(D-94 개정, D-104): 첫 409에서 공통 비교 부품으로 칸마다 고른다. 긴 설명은
// 전문을 읽고 스크롤한다. 기본 하나와 375px·어두움·200% 글꼴을 합친 하나(D-108).

const item = { ...TASK_ITEM, access: 'owner' as const, description: '처음 설명' };

const meta = {
  title: 'Screens/ItemInfoConflict',
  component: ItemScreen,
  args: { workspace: WORKSPACE, itemId: item.id, workspaces: [WORKSPACE] },
  decorators: [() => ({ Component: MainFrame })],
  beforeEach: async () => {
    const previousUser = me.userId;
    me.userId = 'story-item-conflict-user';
    for (const key of Object.keys(localStorage))
      if (key.includes('story-item-conflict-user')) localStorage.removeItem(key);
    const restore = await setUpTime();
    spyOn(api, 'item').mockResolvedValue(item);
    spyOn(api, 'itemDocuments').mockResolvedValue(DOCS.slice(0, 2));
    spyOn(api, 'documentTemplates').mockResolvedValue([]);
    spyOn(api, 'itemProfile').mockResolvedValue({
      leader: null,
      goal: '',
      scope: '',
      starts_on: null,
      ends_on: null,
      status: 'active',
      details: {},
      members: [],
      groups: [],
      can_edit: true,
      can_manage_members: false,
      version: 1,
    });
    spyOn(api, 'itemBacklinks').mockResolvedValue([]);
    spyOn(activityApi, 'mark').mockResolvedValue({ pin: false, favorite: false });
    spyOn(activityApi, 'targetActivity').mockResolvedValue({
      activity: ACTIVITY,
      next_before: null,
      next_before_id: null,
    });
    spyOn(api, 'updateItem').mockResolvedValue({
      kind: 'conflict',
      current: {
        ...item,
        title: '다른 탭에서 바꾼 항목 제목',
        description: '다른 탭에서 쓴 설명\n'.repeat(30),
        visibility: item.visibility === 'workspace' ? 'private' : 'workspace',
        version: item.version + 1,
      },
    });
    const before = router.path;
    router.path = itemGroupPath(WORKSPACE.id, item.id, 'docs');
    return async () => {
      router.path = before;
      me.userId = previousUser;
      await restore();
    };
  },
  play: async ({ canvasElement }) => {
    const page = within(canvasElement.ownerDocument.body);
    await userEvent.click(await page.findByRole('button', { name: /^항목 정보/ }));
    const title = page.getByLabelText(m.modules_title());
    await userEvent.clear(title);
    await userEvent.type(title, '내가 바꾼 항목 제목');
    await userEvent.type(page.getByLabelText(m.modules_description()), '\n내가 덧붙인 줄');
    const form = page.getByRole('form', { name: m.modules_item_info() });
    await userEvent.click(within(form).getByRole('button', { name: m.common_save() }));
    const panel = await page.findByRole('region', { name: m.common_conflict_heading() });
    await expect(
      within(panel).getByRole('radio', {
        name: `${m.modules_title()}: ${m.common_conflict_use_mine()}`,
      }),
    ).not.toBeChecked();
    await expect(
      within(panel).getByText(m.common_conflict_server_changes({ count: 1 })),
    ).toBeVisible();
    await expect(
      within(panel).getByRole('textbox', {
        name: `${m.modules_description()}: ${m.common_conflict_server()}`,
      }),
    ).toHaveValue('다른 탭에서 쓴 설명\n'.repeat(30));
  },
} satisfies Meta<typeof ItemScreen>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Default: Story = {
  globals: { theme: 'light', fontScale: '100', viewport: { value: 'desktop', isRotated: false } },
};
export const Dark375Font200: Story = {
  globals: { theme: 'dark', fontScale: '200', viewport: { value: 'narrow375', isRotated: false } },
};
