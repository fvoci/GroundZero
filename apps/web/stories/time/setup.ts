import { timeData } from '$lib/features/time/data';
import { spyOn } from 'storybook/test';
import { api } from '$lib/api/client';
import { timer } from '$lib/features/time/timer.svelte';
import { setUpScreen } from '../fixtures/screens';
import { course, running, study } from './fixtures';

export async function setUpTime(active = false): Promise<() => Promise<void>> {
  const restore = setUpScreen('/time', {
    currentTimer: () => Promise.resolve(active ? running : null),
  });
  await timer.load();
  spyOn(timeData, 'taskTime').mockResolvedValue({ estimate_minutes: 30, mine_seconds: 600 });
  spyOn(timeData, 'itemTime').mockResolvedValue({
    estimate_minutes: 30,
    kinds: [{ kind: 'work', mine_seconds: 600 }],
  });
  spyOn(timeData, 'timeEntries').mockResolvedValue([]);
  spyOn(timeData, 'timeSummary').mockResolvedValue([]);
  spyOn(api, 'course').mockResolvedValue(course);
  spyOn(api, 'courseParticipants').mockResolvedValue([]);
  spyOn(api, 'studyPlan').mockResolvedValue(study);
  spyOn(api, 'studyParticipants').mockResolvedValue([]);
  spyOn(timeData, 'startTimer').mockImplementation((_request, kind, memo, target) =>
    Promise.resolve({
      ...running,
      kind,
      memo,
      target: target === null ? null : { ...target, workspace_id: 'ws-1', label: '측정 대상' },
      stopped_entry_id: active ? running.id : null,
    }),
  );
  spyOn(timeData, 'timerAction').mockResolvedValue({
    ...running,
    ended_at: '2026-10-26T01:00:00Z',
  });
  return async () => {
    spyOn(timeData, 'currentTimer').mockResolvedValue(null);
    await timer.load();
    restore();
  };
}
