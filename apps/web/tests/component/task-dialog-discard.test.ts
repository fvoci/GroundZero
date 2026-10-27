import './fixtures/task-statuses';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { ItemView, TaskDetailView } from '$lib/api/client';
import { api } from '$lib/api/client';
import TaskBoard from '$lib/features/tasks/TaskBoard.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';
import { itemTaskPath, router } from '$lib/router.svelte';
import { me } from '$lib/me.svelte';
import { accountSafety } from '$lib/features/feedback/feedback.svelte';
import { storeFormDraft } from '$lib/features/common/form-draft';
import { formOf } from '$lib/features/tasks/form';

// 사용자 결정(2026-10-22): 업무 대화상자는 고친 입력이 있어도 Esc·바깥 클릭·닫기 단추로 말없이 닫혀 입력을 버렸다.
// 이제 고친 입력이 있으면 "계속 편집 / 변경 버리고 닫기"를 묻고, 버리기를 고를 때만 닫는다. 고친 것이 없으면 바로 닫는다.
// jsdom은 클릭에 따른 초점 이동을 하지 않으므로 클릭한 곳의 초점은 `.focus()`로 흉내 낸다(실제 브라우저 확인은 따로).
afterEach(() => {
  vi.restoreAllMocks();
});
beforeEach(async () => {
  await setLocale('ko', { reload: false });
});

function task(id: string, fields: Partial<TaskDetailView> = {}): TaskDetailView {
  return {
    id,
    item_id: 'i1',
    module_id: 'm1',
    workspace_id: 'ws-1',
    parent_id: null,
    reference: id === 't1' ? 'KED-1' : 'KED-2',
    title: id === 't1' ? '옛 제목' : '다른 업무',
    description: '',
    status: '할 일',
    priority: 'none',
    start_date: null,
    due_date: null,
    estimate_minutes: 30,
    tags: ['가', '나'],
    milestone: false,
    predecessors: [],
    series: null,
    occurrence_date: null,
    assignees: [],
    status_category: 'todo',
    checklist_done: 0,
    checklist_total: 0,
    version: 2,
    move_version: 0,
    completed_at: null,
    access: 'write',
    updated_at: '2026-10-21T00:00:00Z',
    checklist: [],
    ...fields,
  };
}

const item: ItemView = {
  id: 'i1',
  module_id: 'm1',
  workspace_id: 'ws-1',
  reference: 'KED',
  title: '항목',
  description: '',
  visibility: 'workspace',
  access: 'write',
  archived: false,
  updated_at: '2026-10-19T00:00:00Z',
  status: 'active',
  is_template: false,
  version: 1,
  starts_on: null,
  ends_on: null,
};

function deferred<T>(): { promise: Promise<T>; resolve: (value: T) => void } {
  let resolve: (value: T) => void = () => undefined;
  const promise = new Promise<T>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}

const flush = (): Promise<void> => new Promise((r) => setTimeout(r, 0));

function field(id: string): HTMLInputElement | HTMLTextAreaElement {
  const el = document.getElementById(id);
  if (!(el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement)) throw new Error(id);
  return el;
}

async function type(id: string, value: string): Promise<void> {
  await fireEvent.input(field(id), { target: { value } });
}

function content(): HTMLElement {
  const el = document.querySelector<HTMLElement>('[data-slot="dialog-content"]');
  if (el === null) throw new Error('no dialog');
  return el;
}

function closeButton(): HTMLElement {
  return within(content()).getByRole('button', { name: m.common_close() });
}

const confirmBox = (): HTMLElement | null =>
  screen.queryByRole('alertdialog', { name: m.common_discard_title() });

async function findConfirm(): Promise<HTMLElement> {
  return screen.findByRole('alertdialog', { name: m.common_discard_title() });
}

async function escape(from: Element = document.activeElement ?? document.body): Promise<void> {
  await fireEvent.keyDown(from, { key: 'Escape' });
}

/** 맨 위 가림막을 마우스로 누른다(Bits UI는 문서의 pointerdown을 10ms 늦춰 바깥 클릭으로 본다). */
async function clickOutside(): Promise<void> {
  const overlays = document.querySelectorAll<HTMLElement>(
    '[data-slot="dialog-overlay"], [data-slot="alert-dialog-overlay"]',
  );
  const top = overlays[overlays.length - 1];
  if (top === undefined) throw new Error('no overlay');
  const at = { button: 0, pointerType: 'mouse', clientX: 500, clientY: 500 };
  await fireEvent.pointerDown(top, at);
  await fireEvent.pointerUp(top, at);
  await fireEvent.click(top);
  await new Promise((r) => setTimeout(r, 30));
}

/**
 * 보드에서 업무 카드를 눌러 대화상자를 연다. 보드에서 연 상세는 닫으면 그 기록으로 돌아간다(`history.back`, R5 최종 A FA-4).
 * `back`은 그 돌아가기를 센다(주소는 그대로이므로 닫힌 것은 `reopen(undefined)`로 흉내 낸다).
 */
async function openBoard(tasks: TaskDetailView[] = [task('t1')], beforeOpen?: () => Promise<void>) {
  window.history.replaceState(null, '', '/w/ws-1/i/i1');
  vi.spyOn(api, 'tasks').mockResolvedValue(tasks);
  vi.spyOn(api, 'savedViews').mockResolvedValue([]);
  vi.spyOn(api, 'syncRecurrences').mockResolvedValue(0);
  vi.spyOn(api, 'task').mockImplementation((id: string) =>
    Promise.resolve(tasks.find((t) => t.id === id) ?? task(id)),
  );
  vi.spyOn(api, 'assignable').mockResolvedValue([]);
  vi.spyOn(api, 'taskBacklinks').mockResolvedValue([]);
  vi.spyOn(api, 'comments').mockResolvedValue([]);
  const back = vi.spyOn(window.history, 'back').mockImplementation(() => undefined);
  const props = (openTaskId: string | undefined) => ({ item, workspaces: [], openTaskId });
  const view = render(TaskBoard, { props: props(undefined) });
  await beforeOpen?.();
  await fireEvent.click(await screen.findByRole('button', { name: /^KED-1\s*옛 제목$/ }));
  await view.rerender(props('t1'));
  await screen.findByRole('dialog');
  await waitFor(() => {
    expect(field('task-title').value).toBe('옛 제목');
  });
  return {
    back,
    reopen: async (id: string | undefined) => {
      await view.rerender(props(id));
    },
  };
}

describe('closing the task dialog with unsaved input', () => {
  it.each(['clean', 'discard'] as const)(
    'keeps board input when closing the task detail (%s)',
    async (mode) => {
      me.userId = 'user-a';
      window.localStorage.clear();
      try {
        const { back, reopen } = await openBoard([task('t1')], async () => {
          await fireEvent.input(await screen.findByLabelText(m.tasks_new_title()), {
            target: { value: 'Keep this new title' },
          });
          await fireEvent.click(screen.getByTestId('task-filter-toggle'));
          await fireEvent.input(await screen.findByLabelText(m.tasks_view_name_aria()), {
            target: { value: 'Keep this view name' },
          });
        });
        if (mode === 'discard') await type('task-description', 'Discard only this detail');
        await fireEvent.click(closeButton());
        if (mode === 'discard') {
          await fireEvent.click(
            within(await findConfirm()).getByRole('button', { name: m.common_discard_confirm() }),
          );
        } else {
          expect(confirmBox()).toBeNull();
        }
        expect(back).toHaveBeenCalledTimes(1);
        await reopen(undefined);
        expect(screen.getByLabelText(m.tasks_new_title())).toHaveValue('Keep this new title');
        expect(screen.getByLabelText(m.tasks_view_name_aria())).toHaveValue('Keep this view name');
        const path = router.path;
        router.navigate('/tasks');
        await findConfirm();
        expect(router.path).toBe(path);
      } finally {
        me.userId = '';
        window.localStorage.clear();
      }
    },
  );

  it('opens details without discarding new task and view input that stays on the same board', async () => {
    me.userId = 'user-a';
    window.localStorage.clear();
    try {
      await openBoard([task('t1')], async () => {
        await fireEvent.input(await screen.findByLabelText(m.tasks_new_title()), {
          target: { value: 'Keep this new title' },
        });
        await fireEvent.click(screen.getByTestId('task-filter-toggle'));
        await fireEvent.input(await screen.findByLabelText(m.tasks_view_name_aria()), {
          target: { value: 'Keep this view name' },
        });
      });
      expect(router.path).toBe(itemTaskPath('ws-1', 'i1', 't1'));
      expect(confirmBox()).toBeNull();
      expect(screen.getByLabelText(m.tasks_new_title())).toHaveValue('Keep this new title');
      expect(screen.getByLabelText(m.tasks_view_name_aria())).toHaveValue('Keep this view name');
      router.navigate('/tasks');
      await findConfirm();
      expect(router.path).toBe(itemTaskPath('ws-1', 'i1', 't1'));
    } finally {
      me.userId = '';
      window.localStorage.clear();
    }
  });

  it('keeps the account-switch warning when recurrence storage fails beside a reserved task draft', async () => {
    me.userId = 'user-a';
    window.localStorage.clear();
    const initial = task('t1', {
      start_date: '2026-10-27',
      series: {
        id: 'series-1',
        root_task_id: 't1',
        timezone: 'Asia/Seoul',
        rule: { freq: 'daily', interval: 1, weekdays: [], until: null, count: null },
      },
    });
    expect(
      storeFormDraft('task:t1', {
        baseVersion: initial.version,
        baseline: formOf(initial),
        form: { ...formOf(initial), title: 'Previously kept title' },
        tagsText: '가, 나',
        estimateText: '30',
        savedAt: new Date().toISOString(),
      }),
    ).toBe(true);
    const write = vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new DOMException('Quota exceeded', 'QuotaExceededError');
    });
    try {
      await openBoard([initial]);
      await screen.findByRole('button', { name: m.tasks_draft_restore() });
      await type('task-recurrence-interval', '2');
      accountSafety.switching = true;
      await flush();
      const unload = new Event('beforeunload', { cancelable: true });
      window.dispatchEvent(unload);
      expect(unload.defaultPrevented).toBe(true);
    } finally {
      write.mockRestore();
      accountSafety.switching = false;
      me.userId = '';
      window.localStorage.clear();
    }
  });

  it('blocks closing and opening another task while recurrence is saving', async () => {
    me.userId = 'user-a';
    const rule = { freq: 'daily' as const, interval: 1, weekdays: [], until: null, count: null };
    const series = {
      id: 'series-1',
      root_task_id: 't1',
      timezone: 'Asia/Seoul',
      rule,
    };
    const initial = task('t1', {
      start_date: '2026-10-27',
      series,
    });
    const { back } = await openBoard([initial, task('t2', { parent_id: 't1' })]);
    const pending = deferred<TaskDetailView>();
    vi.spyOn(api, 'setTaskRecurrence').mockReturnValue(pending.promise);
    const navigate = vi.spyOn(router, 'navigate');
    await type('task-recurrence-interval', '2');
    await fireEvent.click(screen.getByRole('button', { name: m.events_recurrence_save() }));
    await escape(content());
    await clickOutside();
    await fireEvent.click(within(content()).getByRole('button', { name: /KED-2\s*다른 업무/ }));
    expect(confirmBox()).toBeNull();
    expect(back).not.toHaveBeenCalled();
    expect(navigate).not.toHaveBeenCalled();
    expect(within(content()).queryByRole('button', { name: m.common_close() })).toBeNull();
    pending.resolve({
      ...initial,
      series: { ...series, rule: { ...rule, interval: 2 } },
    });
    await waitFor(() => {
      expect(within(content()).queryByRole('button', { name: m.common_close() })).not.toBeNull();
    });
    await escape(content());
    expect(back).toHaveBeenCalledTimes(1);
    me.userId = '';
  });

  it('guards recurrence-only changes and discards them through the parent confirmation', async () => {
    me.userId = 'user-a';
    window.localStorage.clear();
    try {
      const { back, reopen } = await openBoard([
        task('t1', {
          start_date: '2026-10-27',
          series: {
            id: 'series-1',
            root_task_id: 't1',
            timezone: 'Asia/Seoul',
            rule: { freq: 'daily', interval: 1, weekdays: [], until: null, count: null },
          },
        }),
      ]);
      await type('task-recurrence-interval', '2');
      const unload = new Event('beforeunload', { cancelable: true });
      window.dispatchEvent(unload);
      expect(unload.defaultPrevented).toBe(true);
      await fireEvent.click(closeButton());
      const box = await findConfirm();
      await fireEvent.click(within(box).getByRole('button', { name: m.common_discard_confirm() }));
      expect(back).toHaveBeenCalledTimes(1);
      await reopen(undefined);
      await reopen('t1');
      await waitFor(() => {
        expect(field('task-recurrence-interval')).toHaveValue('1');
      });
      expect(screen.queryByRole('button', { name: m.common_draft_restore() })).toBeNull();
    } finally {
      me.userId = '';
      window.localStorage.clear();
    }
  });

  it.each(['새 댓글', '새 항목'])(
    'does not close or offer to discard while %s is being saved',
    async (label) => {
      me.userId = 'user-a';
      window.localStorage.clear();
      const pending = deferred<undefined>();
      try {
        const { back } = await openBoard();
        const save =
          label === '새 댓글'
            ? vi.spyOn(api, 'addComment').mockImplementation(async () => {
                await pending.promise;
                return 'new-comment';
              })
            : vi.spyOn(api, 'addChecklist').mockImplementation(async () => {
                await pending.promise;
                return task('t1');
              });
        await fireEvent.input(await screen.findByLabelText(new RegExp(`^${label}`)), {
          target: { value: '저장하는 입력' },
        });
        await fireEvent.click(
          within(content()).getByRole('button', {
            name: label === '새 댓글' ? '댓글 달기' : '추가',
          }),
        );
        await waitFor(() => {
          expect(save).toHaveBeenCalledTimes(1);
        });
        await escape(content());
        expect(confirmBox()).toBeNull();
        expect(back).not.toHaveBeenCalled();
        expect(within(content()).queryByRole('button', { name: m.common_close() })).toBeNull();
        pending.resolve(undefined);
        await waitFor(() => {
          expect(closeButton()).toBeInTheDocument();
        });
      } finally {
        pending.resolve(undefined);
        me.userId = '';
        window.localStorage.clear();
      }
    },
  );

  it.each(['새 댓글', '새 항목'])(
    'discards the %s draft when the parent discard is confirmed',
    async (label) => {
      me.userId = 'user-a';
      window.localStorage.clear();
      try {
        const { back, reopen } = await openBoard();
        const input = await screen.findByLabelText(new RegExp(`^${label}`));
        await fireEvent.input(input, { target: { value: '명시적으로 버릴 입력' } });
        const draftValues = () =>
          Object.keys(window.localStorage).map((key) => window.localStorage.getItem(key));
        await waitFor(() => {
          expect(draftValues().some((value) => value?.includes('명시적으로 버릴 입력'))).toBe(true);
        });
        await fireEvent.click(closeButton());
        const box = await findConfirm();
        await fireEvent.click(
          within(box).getByRole('button', { name: m.common_discard_confirm() }),
        );
        expect(back).toHaveBeenCalledTimes(1);
        await reopen(undefined);
        await reopen('t1');
        await screen.findByLabelText(new RegExp(`^${label}`));
        expect(draftValues().some((value) => value?.includes('명시적으로 버릴 입력'))).toBe(false);
      } finally {
        me.userId = '';
        window.localStorage.clear();
      }
    },
  );

  it('closes at once when nothing was changed', async () => {
    const { back } = await openBoard();
    field('task-title').focus();
    await escape();
    expect(back).toHaveBeenCalledTimes(1);
    expect(confirmBox()).toBeNull();
  });

  it('does not count what the server would store the same as a change', async () => {
    const { back } = await openBoard();
    await type('task-title', ' 옛 제목　');
    await type('task-tags', '가,나 , 가');
    await type('task-estimate', ' 030 ');
    field('task-title').focus();
    await escape();
    expect(back).toHaveBeenCalledTimes(1);
  });

  it('asks before Escape drops a changed title, and Escape on the question keeps the input and focus', async () => {
    const { back } = await openBoard();
    await type('task-title', '새 제목');
    const title = field('task-title');
    title.focus();
    await escape();
    const box = await findConfirm();
    expect(back).not.toHaveBeenCalled();
    await waitFor(() => {
      expect(document.activeElement).toBe(box);
    });
    await escape(box);
    await waitFor(() => {
      expect(confirmBox()).toBeNull();
    });
    expect(back).not.toHaveBeenCalled();
    expect(field('task-title').value).toBe('새 제목');
    await waitFor(() => {
      expect(document.activeElement).toBe(title);
    });
  });

  it('asks for an estimate that is not a number', async () => {
    const { back } = await openBoard();
    await type('task-estimate', '삼십');
    field('task-estimate').focus();
    await escape();
    await findConfirm();
    expect(back).not.toHaveBeenCalled();
  });

  it('keeps editing from the close button and returns focus inside the dialog', async () => {
    const { back } = await openBoard();
    await type('task-description', '메모');
    const button = closeButton();
    button.focus();
    await fireEvent.click(button);
    const box = await findConfirm();
    await fireEvent.click(within(box).getByRole('button', { name: m.common_discard_keep() }));
    await waitFor(() => {
      expect(confirmBox()).toBeNull();
    });
    expect(back).not.toHaveBeenCalled();
    expect(field('task-description').value).toBe('메모');
    await waitFor(() => {
      expect(content().contains(document.activeElement)).toBe(true);
    });
  });

  it('closes at once on an outside click when nothing was changed', async () => {
    const { back } = await openBoard();
    await clickOutside();
    await waitFor(() => {
      expect(back).toHaveBeenCalledTimes(1);
    });
  });

  it('asks on an outside click with a changed field, and a click outside the question changes nothing', async () => {
    const { back } = await openBoard();
    await type('task-tags', '가, 나, 다');
    await clickOutside();
    await findConfirm();
    await clickOutside();
    await flush();
    expect(confirmBox()).not.toBeNull();
    expect(back).not.toHaveBeenCalled();
    expect(field('task-tags').value).toBe('가, 나, 다');
  });

  it('closes only after I choose to discard', async () => {
    const { back, reopen } = await openBoard();
    const update = vi.spyOn(api, 'updateTask');
    await type('task-title', '새 제목');
    field('task-title').focus();
    await escape();
    const box = await findConfirm();
    await fireEvent.click(within(box).getByRole('button', { name: m.common_discard_confirm() }));
    expect(back).toHaveBeenCalledTimes(1);
    expect(update).not.toHaveBeenCalled();
    await reopen(undefined);
    await waitFor(() => {
      expect(document.activeElement).toBe(document.querySelector('[data-task-card="t1"]'));
    });
  });

  // 단추가 아닌 곳(설명 칸, 칸 이름, 대화상자 바탕)을 누른 뒤의 Esc도 같다.
  it('asks after clicking the description, a label or the dialog body and pressing Escape', async () => {
    const { back } = await openBoard();
    await type('task-description', '메모');

    const description = field('task-description');
    description.focus();
    await escape();
    let box = await findConfirm();
    await escape(box);
    await waitFor(() => {
      expect(document.activeElement).toBe(description);
    });

    const label = within(content()).getByText(m.tasks_title(), { selector: 'label' });
    await fireEvent.click(label);
    await escape(label);
    box = await findConfirm();
    await escape(box);
    await waitFor(() => {
      expect(confirmBox()).toBeNull();
    });

    content().focus();
    await escape(content());
    box = await findConfirm();
    await escape(box);
    await waitFor(() => {
      expect(confirmBox()).toBeNull();
    });
    await waitFor(() => {
      expect(content().contains(document.activeElement)).toBe(true);
    });
    expect(back).not.toHaveBeenCalled();
    expect(field('task-description').value).toBe('메모');
  });

  it('closes at once again after the change was saved', async () => {
    const { back } = await openBoard();
    vi.spyOn(api, 'updateTask').mockResolvedValue({
      kind: 'updated',
      task: task('t1', { title: '새 제목', version: 3 }),
    });
    await type('task-title', '새 제목');
    const save = document.getElementById('task-save');
    if (save === null) throw new Error('task-save');
    await fireEvent.click(save);
    await screen.findByText('저장했습니다.');
    // 저장이 끝나 다시 닫을 수 있게 된 뒤(닫기 단추가 돌아온다).
    await waitFor(() => {
      expect(within(content()).queryByRole('button', { name: m.common_close() })).not.toBeNull();
    });
    field('task-title').focus();
    await escape();
    expect(back).toHaveBeenCalledTimes(1);
  });

  // 저장하는 동안 칸을 고칠 수 있었는데, 저장되면 폼을 저장된 판으로 다시 채워(`fill`) 그사이 친 글이 말없이 사라졌다
  // (일정 대화상자의 같은 지적, CodeRabbit #59). 보내는 동안은 폼 칸을 잠근다.
  it('locks the fields while saving so a late result cannot drop what I type meanwhile', async () => {
    await openBoard();
    const late = deferred<{ kind: 'updated'; task: TaskDetailView }>();
    vi.spyOn(api, 'updateTask').mockReturnValue(late.promise);
    await type('task-title', '새 제목');
    const save = document.getElementById('task-save');
    if (save === null) throw new Error('task-save');
    await fireEvent.click(save);
    // 시작일도(CodeRabbit 8cc2a52: 마일스톤 조건과 함께 쓰는 칸이라 빠졌다).
    for (const id of [
      'task-title',
      'task-start',
      'task-due',
      'task-description',
      'task-estimate',
      'task-tags',
    ])
      expect(field(id).disabled, id).toBe(true);
    late.resolve({ kind: 'updated', task: task('t1', { title: '새 제목', version: 3 }) });
    await screen.findByText('저장했습니다.');
    await waitFor(() => {
      expect(field('task-title').disabled).toBe(false);
    });
    expect(field('task-title').value).toBe('새 제목');
  });

  // CodeRabbit(cf44454): 저장 중에 "버리고 닫기"를 골라도 이미 보낸 저장은 서버에 남는다. 일정 대화상자처럼 폼을 저장하는
  // 동안은 닫지도 묻지도 않는다(Esc·바깥 클릭은 무시, 닫기 단추는 숨김). 저장이 끝나면 다시 닫을 수 있다.
  it('neither asks nor closes while the form saves', async () => {
    const { back } = await openBoard();
    const late = deferred<{ kind: 'updated'; task: TaskDetailView }>();
    vi.spyOn(api, 'updateTask').mockReturnValue(late.promise);
    await type('task-title', '새 제목');
    const save = document.getElementById('task-save');
    if (save === null) throw new Error('task-save');
    await fireEvent.click(save);
    await escape(content());
    await clickOutside();
    await flush();
    expect(confirmBox()).toBeNull();
    expect(back).not.toHaveBeenCalled();
    expect(within(content()).queryByRole('button', { name: m.common_close() })).toBeNull();
    late.resolve({ kind: 'updated', task: task('t1', { title: '새 제목', version: 3 }) });
    await screen.findByText('저장했습니다.');
    await waitFor(() => {
      expect(within(content()).queryByRole('button', { name: m.common_close() })).not.toBeNull();
    });
    await escape(content());
    expect(back).toHaveBeenCalledTimes(1);
  });

  it('keeps the overwrite question and my input when I keep editing from the close button', async () => {
    const { back } = await openBoard();
    vi.spyOn(api, 'updateTask').mockResolvedValueOnce({
      kind: 'conflict',
      current: task('t1', { title: '그쪽 제목', version: 3 }),
    });
    await type('task-title', '내 제목');
    const save = document.getElementById('task-save');
    if (save === null) throw new Error('task-save');
    await fireEvent.click(save);
    await fireEvent.click(await screen.findByRole('button', { name: m.tasks_save_mine() }));
    await screen.findByRole('alertdialog', { name: m.tasks_clash_heading() });
    await fireEvent.click(closeButton());
    const box = await findConfirm();
    await escape(box);
    await waitFor(() => {
      expect(confirmBox()).toBeNull();
    });
    expect(back).not.toHaveBeenCalled();
    expect(screen.getByRole('alertdialog', { name: m.tasks_clash_heading() })).toBeInstanceOf(
      HTMLElement,
    );
    expect(field('task-title').value).toBe('내 제목');
  });

  it('drops the question when another task opens, and does not close or carry over to it', async () => {
    const { back, reopen } = await openBoard([task('t1'), task('t2')]);
    await type('task-title', '새 제목');
    field('task-title').focus();
    await escape();
    await findConfirm();
    await reopen('t2');
    await waitFor(() => {
      expect(field('task-title').value).toBe('다른 업무');
    });
    await waitFor(() => {
      expect(confirmBox()).toBeNull();
    });
    expect(back).not.toHaveBeenCalled();
    field('task-title').focus();
    await escape();
    expect(back).toHaveBeenCalledTimes(1);
  });
});

// CodeRabbit(#82): 상세 안의 상위·하위·선행·후속·첫 회차 단추는 다른 업무를 연다(같은 대화상자, `{#key openTaskId}`로 폼을
// 새로 그림). 닫기와 달리 묻지 않아 고친 입력이 말없이 사라졌다. 이제 고친 입력이 있으면 같은 확인으로 묻고, 버리기를
// 고를 때만 그 업무를 연다. 고친 것이 없으면 바로 열고, 저장하는 동안은 옮기지 않는다.
describe('opening another task from the dialog with unsaved input', () => {
  const linked = (): TaskDetailView[] => [task('t1'), task('t2', { parent_id: 't1' })];
  const subtaskLink = (): HTMLElement =>
    within(content()).getByRole('button', { name: /KED-2\s*다른 업무/ });

  it('opens at once when nothing was changed', async () => {
    await openBoard(linked());
    const navigate = vi.spyOn(router, 'navigate');
    await fireEvent.click(subtaskLink());
    expect(confirmBox()).toBeNull();
    expect(navigate).toHaveBeenCalledWith(itemTaskPath('ws-1', 'i1', 't2'));
  });

  it('asks first, and keeping editing stays on this task with my input and focus', async () => {
    await openBoard(linked());
    const navigate = vi.spyOn(router, 'navigate');
    await type('task-description', '메모');
    const link = subtaskLink();
    link.focus();
    await fireEvent.click(link);
    const box = await findConfirm();
    expect(navigate).not.toHaveBeenCalled();
    await fireEvent.click(within(box).getByRole('button', { name: m.common_discard_keep() }));
    await waitFor(() => {
      expect(confirmBox()).toBeNull();
    });
    expect(navigate).not.toHaveBeenCalled();
    expect(field('task-description').value).toBe('메모');
    await waitFor(() => {
      expect(document.activeElement).toBe(link);
    });
  });

  it('opens the other task only after I choose to discard', async () => {
    const { reopen } = await openBoard(linked());
    const navigate = vi.spyOn(router, 'navigate');
    const update = vi.spyOn(api, 'updateTask');
    await type('task-title', '새 제목');
    await fireEvent.click(subtaskLink());
    const box = await findConfirm();
    expect(box.textContent).toContain(m.tasks_discard_open_description());
    await fireEvent.click(
      within(box).getByRole('button', { name: m.tasks_discard_open_confirm() }),
    );
    expect(navigate).toHaveBeenCalledWith(itemTaskPath('ws-1', 'i1', 't2'));
    expect(update).not.toHaveBeenCalled();
    await reopen('t2');
    await waitFor(() => {
      expect(field('task-title').value).toBe('다른 업무');
    });
  });

  it('does not move while the form saves', async () => {
    await openBoard(linked());
    const late = deferred<{ kind: 'updated'; task: TaskDetailView }>();
    vi.spyOn(api, 'updateTask').mockReturnValue(late.promise);
    await type('task-title', '새 제목');
    const save = document.getElementById('task-save');
    if (save === null) throw new Error('task-save');
    await fireEvent.click(save);
    const navigate = vi.spyOn(router, 'navigate');
    await fireEvent.click(subtaskLink());
    await flush();
    expect(confirmBox()).toBeNull();
    expect(navigate).not.toHaveBeenCalled();
    late.resolve({ kind: 'updated', task: task('t1', { title: '새 제목', version: 3 }) });
    await screen.findByText('저장했습니다.');
  });
});

it('warns before unloading only while the open task has unsaved input', async () => {
  const { reopen } = await openBoard();
  const unloading = (): boolean => {
    const event = new Event('beforeunload', { cancelable: true });
    window.dispatchEvent(event);
    return event.defaultPrevented;
  };
  expect(unloading()).toBe(false);
  await type('task-title', '새 제목');
  expect(unloading()).toBe(true);
  await type('task-title', '옛 제목');
  expect(unloading()).toBe(false);
  await type('task-title', '새 제목');
  await reopen(undefined);
  expect(unloading()).toBe(false);
});
