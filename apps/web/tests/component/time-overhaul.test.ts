import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { api, ApiError } from '$lib/api/client';
import * as feedback from '$lib/features/feedback/feedback.svelte';
import TimerWidget from '$lib/features/time/TimerWidget.svelte';
import TaskTimePanel from '$lib/features/time/TaskTimePanel.svelte';
import ItemTimePanel from '$lib/features/time/ItemTimePanel.svelte';
import CoursePanel from '$lib/features/courses/CoursePanel.svelte';
import StudyPanel from '$lib/features/study/StudyPanel.svelte';
import { timer } from '$lib/features/time/timer.svelte';
import { m } from '$lib/paraglide/messages.js';
import { course, running, study } from '../../stories/time/fixtures';

beforeEach(async () => {
  vi.spyOn(api, 'currentTimer').mockResolvedValue(null);
  await timer.load();
});
afterEach(async () => {
  vi.mocked(api.currentTimer).mockResolvedValue(null);
  await timer.load();
  vi.restoreAllMocks();
});

it('the header has no start or target controls without a timer', async () => {
  // beforeEach의 timer.load() 호출을 지워 아래 대기가 위젯 자신의 읽기를 기다리게 한다(F-15).
  vi.mocked(api.currentTimer).mockClear();
  render(TimerWidget);
  await waitFor(() => {
    expect(api.currentTimer).toHaveBeenCalled();
  });
  expect(screen.queryByRole('button')).toBeNull();
});
it('the header only stops, and preserves the running pill on failure', async () => {
  vi.mocked(api.currentTimer).mockResolvedValue(running);
  vi.spyOn(api, 'timerAction').mockRejectedValue(new Error('offline'));
  render(TimerWidget);
  await fireEvent.click(await screen.findByRole('button', { name: m.time_stop() }));
  expect(await screen.findByRole('alert')).toBeInTheDocument();
  expect(screen.getAllByRole('button')).toHaveLength(1);
  expect(timer.entry?.id).toBe(running.id);
  expect(screen.getByRole('status')).toBeInTheDocument();
});
it('a task starts with one server request and reports the previous entry saved', async () => {
  vi.spyOn(api, 'taskTime').mockResolvedValue({ estimate_minutes: 5, mine_seconds: 60 });
  vi.mocked(api.currentTimer).mockResolvedValue(running);
  await timer.load();
  const start = vi
    .spyOn(api, 'startTimer')
    .mockResolvedValue({ ...running, id: 'timer-2', stopped_entry_id: running.id });
  const stop = vi.spyOn(api, 'timerAction');
  const done = vi.spyOn(feedback, 'notifyDone');
  render(TaskTimePanel, { taskId: 'task-2' });
  await fireEvent.click(screen.getByRole('button', { name: m.time_track_task() }));
  await waitFor(() => {
    expect(start).toHaveBeenCalledWith(expect.any(String), 'work', '', {
      kind: 'task',
      id: 'task-2',
    });
  });
  expect(stop).not.toHaveBeenCalled();
  expect(screen.queryByRole('alertdialog')).toBeNull();
  expect(done).toHaveBeenCalledWith(m.time_previous_saved());
});
it('an item only shows my totals and has no start button', async () => {
  vi.spyOn(api, 'itemTime').mockResolvedValue({
    estimate_minutes: 5,
    kinds: [{ kind: 'work', mine_seconds: 60 }],
  });
  render(ItemTimePanel, { itemId: 'item-1' });
  expect(await screen.findByTestId('item-time')).toBeInTheDocument();
  expect(screen.queryByRole('button')).toBeNull();
  expect(screen.getAllByRole('columnheader')).toHaveLength(2);
});
it('an episode starts lecture time from its own row', async () => {
  vi.spyOn(api, 'course').mockResolvedValue(course);
  vi.spyOn(api, 'courseParticipants').mockResolvedValue([]);
  const start = vi.spyOn(api, 'startTimer').mockResolvedValue(running);
  render(CoursePanel, { itemId: 'item-1' });
  await fireEvent.click(
    await screen.findByRole('button', { name: m.courses_track_aria({ number: 1 }) }),
  );
  expect(start).toHaveBeenCalledWith(expect.any(String), 'lecture', '', {
    kind: 'episode',
    id: 'episode-1',
  });
});
it('a study unit starts study time and reports server failures inline', async () => {
  vi.spyOn(api, 'studyPlan').mockResolvedValue(study);
  vi.spyOn(api, 'studyParticipants').mockResolvedValue([]);
  const start = vi
    .spyOn(api, 'startTimer')
    .mockRejectedValue(new ApiError(403, 'forbidden', 'forbidden'));
  render(StudyPanel, { itemId: 'item-1' });
  await fireEvent.click(
    await screen.findByRole('button', { name: m.study_track_aria({ title: '연습 문제' }) }),
  );
  expect(start).toHaveBeenCalledWith(expect.any(String), 'study', '', {
    kind: 'study_unit',
    id: 'unit-1',
  });
  expect(await screen.findByRole('alert')).toHaveTextContent(m.time_start_failed());
});
