import type { Meta, StoryObj } from '@storybook/svelte';
import { api } from '$lib/api/client';
import { fn, spyOn } from 'storybook/test';
import TaskDetail from '$lib/features/tasks/TaskDetail.svelte';
import { itemPath } from '$lib/router.svelte';
import { setUpScreen } from '../fixtures/screens';
import { taskViews, TASK_ITEM, WORKSPACE } from '../fixtures/synthetic';
const tasks = taskViews();
const first = tasks[0];
if (first === undefined) throw new Error('missing task');
const meta = {
  title: 'Tasks/TaskDetail',
  component: TaskDetail,
  args: { taskId: first.id, tasks, workspaces: [WORKSPACE], onopen: fn(), onchanged: fn() },
  beforeEach: () => {
    const restore = setUpScreen(itemPath(WORKSPACE.id, TASK_ITEM.id));
    const mocks = [
      spyOn(api, 'taskBacklinks').mockResolvedValue([]),
      spyOn(api, 'comments').mockResolvedValue([]),
      spyOn(api, 'mark').mockResolvedValue({ pin: false, favorite: false }),
      spyOn(api, 'taskTime').mockResolvedValue({
        estimate_minutes: null,
        mine_seconds: 0,
      }),
    ];
    return () => {
      for (const mock of mocks) mock.mockRestore();
      restore();
    };
  },
} satisfies Meta<typeof TaskDetail>;

export default meta;
type Story = StoryObj<typeof meta>;

export const LightDesktop: Story = {
  globals: { theme: 'light', fontScale: '100', viewport: { value: 'desktop', isRotated: false } },
};
export const Dark375Font200: Story = {
  globals: { theme: 'dark', fontScale: '200', viewport: { value: 'narrow375', isRotated: false } },
};
