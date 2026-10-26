import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { TaskInput } from '$lib/api/client';
import { api } from '$lib/api/client';
import { accountSafety, onAccountSwitch } from '$lib/features/feedback/feedback.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';
import { me } from '$lib/me.svelte';

// D-96·CodeRabbit(#141 1e6de63c): 업무 요청이 409 account_changed를 받으면 다른 요청과 똑같이 앱 전체 계정 전환(안전 상태·
// 다시 읽기)을 시작해야 한다. 업무 폼만 잠그고 앞 계정의 화면을 그대로 두면 다른 화면이 새 계정의 세션으로 요청을 보낸다.

const input: TaskInput = {
  title: '구현',
  description: '',
  status: 'todo',
  priority: 'none',
  start_date: null,
  due_date: null,
  estimate_minutes: null,
  tags: [],
  assignee_ids: [],
  milestone: false,
};

function accountChanged(): Response {
  return new Response(JSON.stringify({ error: 'account_changed', message: 'account changed' }), {
    status: 409,
    headers: { 'Content-Type': 'application/json' },
  });
}

describe('task requests and the account switch', () => {
  const writes = [
    () => api.createTask('original-user', 'i', 'r', 'title'),
    () => api.createQuickTask('original-user', 'i', 'r', 'title', 'original-user'),
    () => api.updateTask('original-user', 't', 1, input),
    () => api.moveTask('original-user', 't', '완료', null),
    () => api.setTaskParent('original-user', 't', null),
    () => api.addChecklist('original-user', 't', []),
    () => api.updateChecklistItem('original-user', 'c', { checked: true }),
    () => api.deleteChecklistItem('original-user', 'c'),
    () => api.setPredecessor('original-user', 't', 'p', 'FS', 0, null),
    () => api.removePredecessor('original-user', 't', 'p'),
    () => api.setTaskRecurrence('original-user', 't', null, 'UTC', null),
    () => api.syncRecurrences('original-user', 'i'),
    () => api.trashTask('original-user', 't'),
    () => api.createTaskStatus('original-user', 'i', '검토', '', 'doing'),
    () => api.updateTaskStatus('original-user', 'i', 's', { name: '검토' }),
    () => api.deleteTaskStatus('original-user', 'i', 's', 'other'),
  ];
  it.each(writes.map((write, index) => [index, write] as const))(
    'pins task write %i to its original user and never retransmits',
    async (_index, write) => {
      me.userId = 'new-user';
      const fetch = vi.spyOn(globalThis, 'fetch').mockResolvedValue(accountChanged());
      await expect(write()).rejects.toMatchObject({ code: 'account_changed' });
      expect(fetch).toHaveBeenCalledOnce();
      expect(new Headers(fetch.mock.calls[0]?.[1]?.headers).get('x-fvoci-expected-user')).toBe(
        'original-user',
      );
      expect(accountSafety.switching).toBe(true);
      expect(reload).toHaveBeenCalledOnce();
    },
  );
  let reload = vi.fn<() => void>();
  let stop: () => void;
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
    accountSafety.switching = false;
    reload = vi.fn<() => void>();
    stop = onAccountSwitch(reload);
  });
  afterEach(() => {
    stop();
    accountSafety.switching = false;
    vi.restoreAllMocks();
  });

  it('starts the global account switch when a task save answers account_changed', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(accountChanged());
    await expect(api.updateTask('user-a', 't-1', 1, input)).rejects.toMatchObject({
      status: 409,
      code: 'account_changed',
    });
    expect(accountSafety.switching).toBe(true);
    expect(reload).toHaveBeenCalledTimes(1);
  });

  it('does the same for creating a task', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(accountChanged());
    await expect(api.createTask('user-a', 'item-1', 'req-1', '새 업무')).rejects.toMatchObject({
      code: 'account_changed',
    });
    expect(accountSafety.switching).toBe(true);
  });
});
