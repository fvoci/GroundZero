import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { TaskInput } from '$lib/api/client';
import { api } from '$lib/api/client';
import { accountSafety, onAccountSwitch } from '$lib/features/feedback/feedback.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

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
    await expect(api.updateTask('t-1', 1, input)).rejects.toMatchObject({
      status: 409,
      code: 'account_changed',
    });
    expect(accountSafety.switching).toBe(true);
    expect(reload).toHaveBeenCalledTimes(1);
  });

  it('does the same for creating a task', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(accountChanged());
    await expect(api.createTask('item-1', 'req-1', '새 업무')).rejects.toMatchObject({
      code: 'account_changed',
    });
    expect(accountSafety.switching).toBe(true);
  });
});
