import { cleanup, render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { timeApi } from '$lib/features/time/api';
import { composeEntry, timeData } from '$lib/features/time/data';
import type { TimeEntryResource, TimeEntryTarget, TimeTargetKind } from '$lib/features/time/model';
import TimeScreen from '$lib/features/time/TimeScreen.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';
import { running } from '../../stories/time/fixtures';

const kinds: TimeTargetKind[] = ['task', 'document', 'item', 'episode', 'study_unit'];
const labels = ['TIM-2 분석', 'DOC-3 문서', 'PRJ-1 항목', 'LEC-1 2회 벡터', 'STU-1 3. 연습 문제'];
const entries: TimeEntryResource[] = kinds.map((kind, i) => ({
  ...running,
  id: `entry-${String(i)}`,
  ended_at: '2026-10-26T00:01:00Z',
  target: { kind, id: `target-${String(i)}` },
}));
const targets: TimeEntryTarget[] = kinds.map((kind, i) => ({
  entry_id: `entry-${String(i)}`,
  kind,
  id: `target-${String(i)}`,
  label: labels[i] ?? null,
  workspace_id: 'ws-1',
}));
beforeEach(async () => {
  await setLocale('ko', { reload: false });
  vi.spyOn(timeData, 'currentTimer').mockResolvedValue(null);
  vi.spyOn(timeData, 'timeSummary').mockResolvedValue([]);
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

it('keeps all five target labels on screen with a single target collection read', async () => {
  const resources = vi.spyOn(timeApi, 'timeEntries').mockResolvedValue(entries);
  const display = vi.spyOn(timeApi, 'timeEntryTargets').mockResolvedValue(targets);
  render(TimeScreen);
  for (const label of labels) expect(await screen.findByText(label)).toBeInTheDocument();
  expect(resources).toHaveBeenCalledTimes(1);
  expect(display).toHaveBeenCalledTimes(1);
});

it('discards mismatched target metadata and retries the collection once', async () => {
  const first = entries[0];
  const target = targets[0];
  if (first === undefined || target === undefined) throw new Error('missing fixture');
  expect(composeEntry(first, [{ ...target, id: 'previous-target' }]).target?.label).toBeNull();
  vi.spyOn(timeApi, 'timeEntries').mockResolvedValue([first]);
  const display = vi
    .spyOn(timeApi, 'timeEntryTargets')
    .mockResolvedValueOnce([{ ...target, id: 'previous-target' }])
    .mockResolvedValueOnce([target]);
  expect((await timeData.timeEntries('2026-10-26', '2026-10-26', 'UTC'))[0]?.target?.label).toBe(
    labels[0],
  );
  expect(display).toHaveBeenCalledTimes(2);
});
