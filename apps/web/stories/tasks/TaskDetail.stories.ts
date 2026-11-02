import { timeData } from '$lib/features/time/data';
import { activityApi } from '$lib/features/activity/api';
import type { Meta, StoryObj } from '@storybook/svelte';
import { api } from '$lib/api/client';
import { fn, spyOn } from 'storybook/test';
import TaskDetail from '$lib/features/tasks/TaskDetail.svelte';
import { taskData } from '$lib/features/tasks/detail-data';
import {
  taskScreenApi,
  type DetailRead,
  type DetailOptionsRead,
} from '$lib/features/tasks/screen-api';
import { me } from '$lib/me.svelte';
import { itemPath } from '$lib/router.svelte';
import { setUpScreen } from '../fixtures/screens';
import { taskViews, TASK_ITEM, WORKSPACE } from '../fixtures/synthetic';
const tasks = taskViews();
const first = tasks[0];
if (first === undefined) throw new Error('missing task');
const firstId = first.id;
function setup() {
  const restore = setUpScreen(itemPath(WORKSPACE.id, TASK_ITEM.id));
  const mocks = [
    spyOn(api, 'taskBacklinks').mockResolvedValue([]),
    spyOn(activityApi, 'comments').mockResolvedValue([]),
    spyOn(activityApi, 'mark').mockResolvedValue({ pin: false, favorite: false }),
    spyOn(timeData, 'taskTime').mockResolvedValue({
      estimate_minutes: null,
      mine_seconds: 0,
    }),
  ];
  return () => {
    for (const mock of mocks) mock.mockRestore();
    restore();
  };
}

function aggregate(section: 'ready' | 'statuses' | 'assignable') {
  return async () => {
    const restore = setup();
    const [current, statuses, assignable] = await Promise.all([
      taskData.task(firstId, TASK_ITEM.id),
      taskData.taskStatuses(TASK_ITEM.id),
      taskData.assignable(TASK_ITEM.id),
    ]);
    const read: DetailRead = {
      viewer_id: me.userId,
      task: current,
      item: TASK_ITEM,
      archived: false,
      tasks,
      assignees: tasks.flatMap((task) =>
        task.assignees.map((person) => ({ task_id: task.id, ...person })),
      ),
      dependencies: tasks.flatMap((task) =>
        task.predecessors.map((link) => ({
          predecessor_id: link.task_id,
          successor_id: task.id,
          kind: link.kind,
          lag_days: link.lag_days,
        })),
      ),
      series: tasks.flatMap((task) => (task.series === null ? [] : [task.series])),
      checklist: current.checklist.map((row) => ({ ...row, task_id: current.id })),
    };
    const options: DetailOptionsRead = {
      viewer_id: me.userId,
      item_id: TASK_ITEM.id,
      statuses:
        section === 'statuses'
          ? { state: 'failed', error: 'unavailable' }
          : { state: 'ready', value: statuses },
      assignable:
        section === 'assignable'
          ? { state: 'failed', error: 'unavailable' }
          : { state: 'ready', value: assignable },
    };
    const mock = spyOn(taskScreenApi, 'detail').mockResolvedValue(read);
    const optionMock = spyOn(taskScreenApi, 'options').mockResolvedValue(options);
    return () => {
      mock.mockRestore();
      optionMock.mockRestore();
      restore();
    };
  };
}

const meta = {
  title: 'Tasks/TaskDetail',
  component: TaskDetail,
  args: { taskId: first.id, tasks, workspaces: [WORKSPACE], onopen: fn(), onchanged: fn() },
  beforeEach: setup,
} satisfies Meta<typeof TaskDetail>;

export default meta;
type Story = StoryObj<typeof meta>;

export const LightDesktop: Story = {
  globals: { theme: 'light', fontScale: '100', viewport: { value: 'desktop', isRotated: false } },
};
export const Dark375Font200: Story = {
  args: { aggregate: true },
  beforeEach: aggregate('ready'),
  globals: { theme: 'dark', fontScale: '200', viewport: { value: 'narrow375', isRotated: false } },
};

export const AggregateRead: Story = {
  args: { aggregate: true },
  beforeEach: aggregate('ready'),
};

export const AggregateStatusesUnavailable: Story = {
  args: { aggregate: true },
  beforeEach: aggregate('statuses'),
};

export const AggregateAssignableUnavailable: Story = {
  args: { aggregate: true },
  beforeEach: aggregate('assignable'),
};
