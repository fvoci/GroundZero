/** 시간 자원과 대상 표시를 화면 곁에서 조합한다. 한 목록에 읽기 둘이며 기록별 요청은 없다. */
import { timeApi } from './api';
import type { TimeEntryResource, TimeEntryView, TimeEntryTarget } from './model';

export function composeEntry(entry: TimeEntryResource, targets: TimeEntryTarget[]): TimeEntryView {
  const link = entry.target;
  if (link === null) return { ...entry, target: null };
  // 따로 읽은 사이 연결이 바뀌었으면 예전 대상의 문구를 붙이지 않는다.
  const target = targets.find(
    (t) => t.entry_id === entry.id && t.kind === link.kind && t.id === link.id,
  );
  return {
    ...entry,
    target: { ...link, label: target?.label ?? null, workspace_id: target?.workspace_id ?? null },
  };
}
async function display(entry: TimeEntryResource): Promise<TimeEntryView> {
  return composeEntry(
    entry,
    entry.target === null ? [] : await timeApi.timeEntryTargets({ entryId: entry.id }),
  );
}

export const timeData = {
  ...timeApi,
  currentTimer: async () => {
    const entry = await timeApi.currentTimer();
    return entry === null ? null : display(entry);
  },
  timeEntries: async (from: string, to: string, tz: string): Promise<TimeEntryView[]> => {
    const [entries, targets] = await Promise.all([
      timeApi.timeEntries(from, to, tz),
      timeApi.timeEntryTargets({ from, to, tz }),
    ]);
    const incomplete = entries.some(
      (entry) =>
        entry.target !== null &&
        !targets.some(
          (target) =>
            target.entry_id === entry.id &&
            target.kind === entry.target?.kind &&
            target.id === entry.target.id,
        ),
    );
    const current = incomplete ? await timeApi.timeEntryTargets({ from, to, tz }) : targets;
    return entries.map((entry) => composeEntry(entry, current));
  },
  startTimer: async (...args: Parameters<typeof timeApi.startTimer>) => {
    const started = await timeApi.startTimer(...args);
    return { ...(await display(started)), stopped_entry_id: started.stopped_entry_id };
  },
  timerAction: async (...args: Parameters<typeof timeApi.timerAction>) => {
    return display(await timeApi.timerAction(...args));
  },
  addTimeEntry: async (...args: Parameters<typeof timeApi.addTimeEntry>) => {
    return display(await timeApi.addTimeEntry(...args));
  },
  updateTimeEntry: async (...args: Parameters<typeof timeApi.updateTimeEntry>) => {
    const result = await timeApi.updateTimeEntry(...args);
    return result.kind === 'conflict'
      ? { ...result, current: await display(result.current) }
      : { ...result, entry: await display(result.entry) };
  },
};
