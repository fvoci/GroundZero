import { beforeEach, afterEach, expect, it, vi } from 'vitest';
import { taskScreenApi } from '$lib/features/tasks/screen-api';
import { accountSafety, auth } from '$lib/features/feedback/feedback.svelte';
import { me } from '$lib/me.svelte';
import { PROJECT_MODULE, TASK_ITEM, taskViews } from '../../stories/fixtures/synthetic';
import { DiscardedTaskRead } from '$lib/features/tasks/context-read';
import { readSessionGeneration } from '$lib/features/auth/session-signal';

beforeEach(() => {
  me.userId = 'viewer-a';
  accountSafety.switching = false;
  accountSafety.restoring = false;
  auth.expired = false;
});
afterEach(() => {
  vi.restoreAllMocks();
  accountSafety.switching = false;
});

it.each(['context', 'board', 'detail', 'options'] as const)(
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
          : kind === 'detail'
            ? {
                ...bundle,
                task: taskViews()[0],
                item: TASK_ITEM,
                statuses: { state: 'ready', value: [] },
                assignable: { state: 'ready', value: [] },
              }
            : {
                viewer_id: 'viewer-b',
                item_id: 'id',
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

it.each(
  (['context', 'board', 'detail', 'options'] as const).flatMap((kind) =>
    (['same-viewer', 'other-viewer', 'failure', 'expired', 'account-changed'] as const).map(
      (outcome) => ({ kind, outcome }),
    ),
  ),
)(
  'SR-298-2 discards a $kind $outcome response after session A → B → A',
  async ({ kind, outcome }) => {
    let respond!: (response: Response) => void;
    vi.spyOn(globalThis, 'fetch').mockReturnValue(
      new Promise<Response>((resolve) => {
        respond = resolve;
      }),
    );
    const pending = taskScreenApi[kind]('id');
    const rejected = expect(pending).rejects.toBeInstanceOf(DiscardedTaskRead);
    for (const sequence of ['B', 'A']) {
      localStorage.setItem(
        'fvoci:auth-session',
        JSON.stringify({ state: 'settled', generation: readSessionGeneration() + 1, sequence }),
      );
    }
    const viewer_id = outcome === 'other-viewer' ? 'viewer-b' : 'viewer-a';
    const bundle = {
      viewer_id,
      archived: false,
      item_access: 'owner',
      tasks: [],
      statuses: [],
      assignees: [],
      dependencies: [],
      series: [],
      checklist: [],
    };
    const status =
      outcome === 'failure'
        ? 403
        : outcome === 'expired'
          ? 401
          : outcome === 'account-changed'
            ? 409
            : 200;
    const body =
      status !== 200
        ? {
            error: outcome === 'account-changed' ? 'account_changed' : 'forbidden',
            message: 'B session failure',
          }
        : kind === 'context'
          ? { viewer_id, item: TASK_ITEM, module: PROJECT_MODULE, documents: [] }
          : kind === 'board'
            ? bundle
            : kind === 'detail'
              ? { ...bundle, task: taskViews()[0], item: TASK_ITEM }
              : {
                  viewer_id,
                  item_id: 'id',
                  statuses: { state: 'ready', value: [] },
                  assignable: { state: 'ready', value: [] },
                };
    respond(
      new Response(JSON.stringify(body), {
        status,
        headers: { 'Content-Type': 'application/json' },
      }),
    );
    await rejected;
    expect(accountSafety.switching).toBe(false);
    expect(auth.expired).toBe(false);
  },
);

it.each(['context', 'board', 'detail', 'options'] as const)(
  'SR-298-2 preserves a current %s permission error',
  async (kind) => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify({ error: 'forbidden', message: 'current session failure' }), {
        status: 403,
        headers: { 'Content-Type': 'application/json' },
      }),
    );
    await expect(taskScreenApi[kind]('id')).rejects.toMatchObject({
      status: 403,
      code: 'forbidden',
    });
  },
);
