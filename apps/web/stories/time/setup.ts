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
  spyOn(api, 'taskTime').mockResolvedValue({ estimate_minutes: 30, mine_seconds: 600 });
  spyOn(api, 'itemTime').mockResolvedValue({
    estimate_minutes: 30,
    kinds: [{ kind: 'work', mine_seconds: 600 }],
  });
  spyOn(api, 'timeEntries').mockResolvedValue([]);
  spyOn(api, 'timeSummary').mockResolvedValue([]);
  spyOn(api, 'course').mockResolvedValue(course);
  spyOn(api, 'courseParticipants').mockResolvedValue([]);
  spyOn(api, 'studyPlan').mockResolvedValue(study);
  spyOn(api, 'studyParticipants').mockResolvedValue([]);
  spyOn(api, 'startTimer').mockImplementation((_request, kind, memo, target) =>
    Promise.resolve({
      ...running,
      kind,
      memo,
      target: target === null ? null : { ...target, workspace_id: 'ws-1', label: '측정 대상' },
      stopped_entry_id: active ? running.id : null,
    }),
  );
  spyOn(api, 'timerAction').mockResolvedValue({ ...running, ended_at: '2026-10-26T01:00:00Z' });
  return async () => {
    spyOn(api, 'currentTimer').mockResolvedValue(null);
    await timer.load();
    restore();
  };
}
