import { cleanup, render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { timeApi } from '$lib/features/time/api';
import { instantOf } from '$lib/features/events/time';
import { composeEntry, timeData } from '$lib/features/time/data';
import type { TimeEntryResource, TimeEntryTarget, TimeTargetKind } from '$lib/features/time/model';
import TimeScreen from '$lib/features/time/TimeScreen.svelte';
import { me } from '$lib/me.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';
import { running } from '../../stories/time/fixtures';

const kinds: TimeTargetKind[] = ['task', 'document', 'item', 'episode', 'study_unit'];
const labels = ['TIM-2 분석', 'DOC-3 문서', 'PRJ-1 항목', 'LEC-1 2회 벡터', 'STU-1 3. 연습 문제'];
const originalDisplay = { timeZone: me.timeZone, weekStart: me.weekStart };
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
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.restoreAllMocks();
  Object.assign(me, originalDisplay);
});

it.each([
  { now: undefined, zone: 'Asia/Seoul', weekStart: 'sun', from: undefined, to: undefined },
  {
    now: '2026-10-31T14:59:59Z',
    zone: 'Asia/Seoul',
    weekStart: 'sun',
    from: '2026-10-25',
    to: '2026-10-31',
  },
  {
    now: '2026-10-31T15:00:00Z',
    zone: 'Asia/Seoul',
    weekStart: 'sun',
    from: '2026-11-01',
    to: '2026-11-07',
  },
  {
    now: '2026-11-01T15:00:00Z',
    zone: 'Asia/Seoul',
    weekStart: 'mon',
    from: '2026-11-02',
    to: '2026-11-08',
  },
  {
    now: '2026-11-01T04:00:00Z',
    zone: 'America/New_York',
    weekStart: 'sun',
    from: '2026-11-01',
    to: '2026-11-07',
  },
  {
    now: '2026-12-31T14:59:59Z',
    zone: 'Asia/Seoul',
    weekStart: 'mon',
    from: '2026-12-28',
    to: '2027-01-03',
  },
  {
    now: '2026-12-31T15:00:00Z',
    zone: 'Asia/Seoul',
    weekStart: 'sun',
    from: '2026-12-27',
    to: '2027-01-02',
  },
  {
    now: '2027-01-03T00:00:00Z',
    zone: 'UTC',
    weekStart: 'sun',
    from: '2027-01-03',
    to: '2027-01-09',
  },
])(
  'keeps all five target labels on screen with a single target collection read ($now, $zone, $weekStart)',
  async ({ now, zone, weekStart, from, to }) => {
    if (now !== undefined) {
      vi.useFakeTimers({ toFake: ['Date'] });
      vi.setSystemTime(new Date(now));
    }
    Object.assign(me, { timeZone: zone, weekStart });
    vi.spyOn(timeData, 'currentTimer').mockResolvedValue(null);
    vi.spyOn(timeData, 'timeSummary').mockResolvedValue([]);
    // API처럼 요청한 주 안의 기록을 돌려준다. 공유 fixture의 고정 날짜가 실제 오늘의 주 밖으로 밀려나지 않게 한다.
    const resources = vi.spyOn(timeApi, 'timeEntries').mockImplementation((start, _end, tz) =>
      Promise.resolve(
        entries.map((entry) => ({
          ...entry,
          started_at: instantOf(`${start}T12:00`, tz),
          ended_at: instantOf(`${start}T12:01`, tz),
        })),
      ),
    );
    const display = vi.spyOn(timeApi, 'timeEntryTargets').mockResolvedValue(targets);
    render(TimeScreen);
    for (const label of labels) expect(await screen.findByText(label)).toBeInTheDocument();
    expect(resources).toHaveBeenCalledTimes(1);
    expect(display).toHaveBeenCalledExactlyOnceWith({
      from: resources.mock.calls[0]?.[0],
      to: resources.mock.calls[0]?.[1],
      tz: zone,
    });
    if (from !== undefined) {
      expect(resources).toHaveBeenCalledExactlyOnceWith(from, to, zone);
      expect(display).toHaveBeenCalledExactlyOnceWith({ from, to, tz: zone });
    }
  },
);

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

it.each([
  { operation: 'startTimer', entry: entries[0] },
  { operation: 'timerAction', entry: entries[1] },
  { operation: 'addTimeEntry', entry: entries[2] },
  { operation: 'updated', entry: entries[3] },
  { operation: 'conflict', entry: entries[4] },
] as const)(
  'keeps the $operation result when target display rejects',
  async ({ operation, entry }) => {
    if (entry === undefined) throw new Error('missing fixture');
    const input = {
      started_at: entry.started_at,
      ended_at: entry.ended_at,
      paused_seconds: entry.paused_seconds,
      kind: entry.kind,
      memo: entry.memo,
      target: entry.target,
    };
    const expected = {
      ...entry,
      target: { ...entry.target, label: null, workspace_id: null },
    };
    const display = vi
      .spyOn(timeApi, 'timeEntryTargets')
      .mockRejectedValue(new Error('display failed'));
    switch (operation) {
      case 'startTimer': {
        const started = { ...entry, stopped_entry_id: 'previous-entry' };
        vi.spyOn(timeApi, 'startTimer').mockResolvedValue(started);
        await expect(
          timeData.startTimer('request-1', entry.kind, entry.memo, entry.target),
        ).resolves.toEqual({ ...expected, stopped_entry_id: started.stopped_entry_id });
        break;
      }
      case 'timerAction':
        vi.spyOn(timeApi, 'timerAction').mockResolvedValue(entry);
        await expect(timeData.timerAction(entry.id, 'stop')).resolves.toEqual(expected);
        break;
      case 'addTimeEntry':
        vi.spyOn(timeApi, 'addTimeEntry').mockResolvedValue(entry);
        await expect(timeData.addTimeEntry('request-1', input)).resolves.toEqual(expected);
        break;
      case 'updated':
        vi.spyOn(timeApi, 'updateTimeEntry').mockResolvedValue({ kind: 'updated', entry });
        await expect(timeData.updateTimeEntry(entry.id, entry.version, input)).resolves.toEqual({
          kind: 'updated',
          entry: expected,
        });
        break;
      case 'conflict':
        vi.spyOn(timeApi, 'updateTimeEntry').mockResolvedValue({
          kind: 'conflict',
          current: entry,
          changed: ['memo'],
        });
        await expect(timeData.updateTimeEntry(entry.id, entry.version, input)).resolves.toEqual({
          kind: 'conflict',
          current: expected,
          changed: ['memo'],
        });
        break;
    }
    expect(display).toHaveBeenCalledExactlyOnceWith({ entryId: entry.id });
  },
);

it('returns the current timer when target display rejects', async () => {
  const entry = entries[0];
  if (entry === undefined) throw new Error('missing fixture');
  vi.spyOn(timeApi, 'currentTimer').mockResolvedValue(entry);
  vi.spyOn(timeApi, 'timeEntryTargets').mockRejectedValue(new Error('display failed'));
  await expect(timeData.currentTimer()).resolves.toEqual({
    ...entry,
    target: { ...entry.target, label: null, workspace_id: null },
  });
});

it('returns the entry collection when target display rejects', async () => {
  vi.spyOn(timeApi, 'timeEntries').mockResolvedValue(entries);
  vi.spyOn(timeApi, 'timeEntryTargets').mockRejectedValue(new Error('display failed'));
  await expect(timeData.timeEntries('2026-10-26', '2026-10-26', 'UTC')).resolves.toEqual(
    entries.map((entry) => ({
      ...entry,
      target: { ...entry.target, label: null, workspace_id: null },
    })),
  );
});

it('keeps the first target display when the incomplete collection retry rejects', async () => {
  const firstTargets = targets.slice(0, 1);
  vi.spyOn(timeApi, 'timeEntries').mockResolvedValue(entries);
  const display = vi
    .spyOn(timeApi, 'timeEntryTargets')
    .mockResolvedValueOnce(firstTargets)
    .mockRejectedValueOnce(new Error('retry failed'));
  await expect(timeData.timeEntries('2026-10-26', '2026-10-26', 'UTC')).resolves.toEqual(
    entries.map((entry) => composeEntry(entry, firstTargets)),
  );
  expect(display).toHaveBeenCalledTimes(2);
});

it.each([
  'timeEntries',
  'currentTimer',
  'startTimer',
  'timerAction',
  'addTimeEntry',
  'updateTimeEntry',
] as const)('still rejects when the %s resource request fails', async (operation) => {
  const entry = entries[0];
  if (entry === undefined) throw new Error('missing fixture');
  const error = new Error('record request failed');
  vi.spyOn(timeApi, operation).mockRejectedValue(error);
  const display = vi.spyOn(timeApi, 'timeEntryTargets').mockResolvedValue(targets);
  const input = {
    started_at: entry.started_at,
    ended_at: entry.ended_at,
    paused_seconds: entry.paused_seconds,
    kind: entry.kind,
    memo: entry.memo,
    target: entry.target,
  };
  const requests = {
    timeEntries: () => timeData.timeEntries('2026-10-26', '2026-10-26', 'UTC'),
    currentTimer: () => timeData.currentTimer(),
    startTimer: () => timeData.startTimer('request-1', entry.kind, entry.memo, entry.target),
    timerAction: () => timeData.timerAction(entry.id, 'stop'),
    addTimeEntry: () => timeData.addTimeEntry('request-1', input),
    updateTimeEntry: () => timeData.updateTimeEntry(entry.id, entry.version, input),
  };
  await expect(requests[operation]()).rejects.toBe(error);
  if (operation !== 'timeEntries') expect(display).not.toHaveBeenCalled();
});
