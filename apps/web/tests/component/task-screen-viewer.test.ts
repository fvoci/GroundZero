import { beforeEach, afterEach, expect, it, vi } from 'vitest';
import { taskScreenApi } from '$lib/features/tasks/screen-api';
import { accountSafety } from '$lib/features/feedback/feedback.svelte';
import { me } from '$lib/me.svelte';
import { PROJECT_MODULE, TASK_ITEM, taskViews } from '../../stories/fixtures/synthetic';

beforeEach(() => {
  me.userId = 'viewer-a';
  accountSafety.switching = false;
  accountSafety.restoring = false;
});
afterEach(() => {
  vi.restoreAllMocks();
  accountSafety.switching = false;
});

it.each(['context', 'board', 'detail'] as const)(
  'rejects another viewer before adopting a %s read, including SPA fallback',
  async (kind) => {
    const bundle = {
      viewer_id: 'viewer-b',
      archived: false,
      item_id: TASK_ITEM.id,
      item_access: 'owner',
      tasks: [],
      statuses: [],
      assignees: [],
      dependencies: [],
      series: [],
      checklist: [],
    };
    const body =
      kind === 'context'
        ? { viewer_id: 'viewer-b', item: TASK_ITEM, module: PROJECT_MODULE, documents: [] }
        : kind === 'board'
          ? bundle
          : {
              ...bundle,
              task: taskViews()[0],
              item: TASK_ITEM,
              statuses: { state: 'ready', value: [] },
              assignable: { state: 'ready', value: [] },
            };
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify(body), {
        status: 200,
        headers: { 'Content-Type': 'application/json' },
      }),
    );
    let adopted = false;
    await expect(
      taskScreenApi[kind]('id').then(() => {
        adopted = true;
      }),
    ).rejects.toMatchObject({ code: 'account_changed' });
    expect(adopted).toBe(false);
    expect(accountSafety.switching).toBe(true);
  },
);
