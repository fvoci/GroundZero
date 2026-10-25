import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { timer } from '$lib/features/time/timer.svelte';
import TaskTimeStart from '$lib/features/time/TaskTimeStart.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

// 업무 행·카드의 시간 기록 시작 단추(D-117): 서버 합계를 읽지 않고 타이머 저장소로 이 업무를 시작하거나 멈춘다.

describe('task time start button', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
    timer.entry = null;
  });
  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
    timer.entry = null;
  });

  it('starts work time on this task without loading the total', async () => {
    const start = vi.spyOn(timer, 'start').mockResolvedValue(undefined);
    render(TaskTimeStart, { props: { taskId: 't-1', title: '설계 검토' } });
    const button = screen.getByRole('button', { name: /설계 검토/ });
    expect(button).toHaveAttribute('aria-pressed', 'false');
    await fireEvent.click(button);
    expect(start).toHaveBeenCalledWith('work', { kind: 'task', id: 't-1' });
  });

  it('offers stop while this task is being recorded and reports a failed stop inline', async () => {
    timer.entry = {
      id: 'e-1',
      kind: 'work',
      memo: '',
      started_at: '2026-10-26T00:00:00Z',
      ended_at: null,
      paused_at: null,
      paused_seconds: 0,
      seconds: 60,
      version: 1,
      target: { kind: 'task', id: 't-1', label: '설계 검토', workspace_id: 'ws-1' },
    };
    const act = vi.spyOn(timer, 'act').mockRejectedValue(new Error('offline'));
    render(TaskTimeStart, { props: { taskId: 't-1', title: '설계 검토' } });
    const button = screen.getByRole('button', { name: /설계 검토/ });
    expect(button).toHaveAttribute('aria-pressed', 'true');
    await fireEvent.click(button);
    expect(act).toHaveBeenCalledWith('stop');
    expect(await screen.findByRole('alert')).toBeInTheDocument();
  });
});
