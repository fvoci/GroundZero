import { request, expectOk, fail } from '$lib/shared/api';
import {
  int,
  isRecord,
  none,
  parseList,
  rec,
  str,
  optStr,
  optInt,
  strings,
  type Json,
} from '$lib/shared/api/parse';
import {
  parseEntryResource,
  parseEntryTarget,
  timeKind,
  type TimeKind,
  type TimeTargetKind,
  type TimeEntryResource,
  type TimeEntryInput,
  type TimeSummaryRowView,
  type ItemTimeView,
  type TaskTimeView,
} from './model';

export const timeApi = {
  // D-117 개인 시간 계약, 시작 응답의 자동 종료 기록.
  currentTimer: () =>
    expectOk('GET', '/api/me/timer', (v) => (v === null ? null : parseEntryResource(v))),
  startTimer: (
    requestId: string,
    kind: TimeKind,
    memo: string,
    target: { kind: TimeTargetKind; id: string } | null,
    mode?: 'continue' | 'new',
  ) =>
    expectOk(
      'POST',
      '/api/me/timer',
      (v) => ({
        ...parseEntryResource(v),
        stopped_entry_id: optStr(rec(v, 'started timer'), 'stopped_entry_id'),
      }),
      {
        request_id: requestId,
        kind,
        memo,
        target: target === null ? null : { ...target },
        ...(mode === undefined ? {} : { mode }),
      },
    ),
  timerAction: (id: string, action: 'pause' | 'resume' | 'stop') =>
    expectOk('POST', `/api/time-entries/${encodeURIComponent(id)}/timer`, parseEntryResource, {
      action,
    }),
  timeEntries: (from: string, to: string, tz: string) =>
    expectOk(
      'GET',
      `/api/me/time-entries?from=${from}&to=${to}&tz=${encodeURIComponent(tz)}`,
      (v) => parseList(v, parseEntryResource),
    ),
  addTimeEntry: (requestId: string, input: TimeEntryInput) =>
    expectOk('POST', '/api/me/time-entries', parseEntryResource, {
      request_id: requestId,
      ...input,
      target: input.target === null ? null : { ...input.target },
    }),
  /** 기록 고치기. 기준 버전이 지났으면 `{ kind: 'conflict', current }`. */
  async updateTimeEntry(
    id: string,
    baseVersion: number,
    input: TimeEntryInput,
  ): Promise<
    | { kind: 'updated'; entry: TimeEntryResource }
    | { kind: 'conflict'; current: TimeEntryResource; changed: string[] }
  > {
    const { status, json } = await request('PUT', `/api/time-entries/${encodeURIComponent(id)}`, {
      base_version: baseVersion,
      ...input,
      target: input.target === null ? null : { ...input.target },
    });
    if (
      status === 409 &&
      isRecord(json) &&
      json.error === 'conflict' &&
      json.current !== undefined
    ) {
      str(json, 'message');
      optStr(json, 'request_id');
      return {
        kind: 'conflict',
        current: parseEntryResource(json.current),
        changed: strings(json.changed),
      };
    }
    if (status !== 200) fail(status, json);
    return { kind: 'updated', entry: parseEntryResource(json) };
  },
  deleteTimeEntry: (id: string) =>
    expectOk('DELETE', `/api/time-entries/${encodeURIComponent(id)}`, none),
  timeEntryRevisions: (id: string) =>
    expectOk('GET', `/api/time-entries/${encodeURIComponent(id)}/revisions`, (v) =>
      parseList(v, (r) => {
        const o = rec(r, 'revision');
        return {
          started_at: str(o, 'started_at'),
          ended_at: optStr(o, 'ended_at'),
          paused_seconds: int(o, 'paused_seconds'),
          kind: timeKind(str(o, 'kind')),
          memo: str(o, 'memo'),
          changed_at: str(o, 'changed_at'),
        };
      }),
    ),
  timeSummary: (from: string, to: string, tz: string, group: 'day' | 'week' | 'month') =>
    expectOk(
      'GET',
      `/api/me/time-summary?from=${from}&to=${to}&tz=${encodeURIComponent(tz)}&group=${group}`,
      (v) =>
        parseList(v, (r): TimeSummaryRowView => {
          const o = rec(r, 'summary row');
          return {
            period: str(o, 'period'),
            kind: timeKind(str(o, 'kind')),
            seconds: int(o, 'seconds'),
          };
        }),
    ),
  itemTime: (itemId: string) =>
    expectOk('GET', `/api/items/${encodeURIComponent(itemId)}/time`, (v): ItemTimeView => {
      const o = rec(v, 'item time');
      if (!Array.isArray(o.kinds)) throw new TypeError('item time kinds');
      return {
        estimate_minutes: int(o, 'estimate_minutes'),
        kinds: o.kinds.map((k: Json) => {
          const r = rec(k, 'item time kind');
          return {
            kind: timeKind(str(r, 'kind')),
            mine_seconds: int(r, 'mine_seconds'),
          };
        }),
      };
    }),
  taskTime: (taskId: string) =>
    expectOk('GET', `/api/tasks/${encodeURIComponent(taskId)}/time`, (v): TaskTimeView => {
      const o = rec(v, 'task time');
      return {
        estimate_minutes: optInt(o, 'estimate_minutes'),
        mine_seconds: int(o, 'mine_seconds'),
      };
    }),

  timeEntryTargets: (scope: { entryId: string } | { from: string; to: string; tz: string }) => {
    const params =
      'entryId' in scope
        ? new URLSearchParams({ entry_id: scope.entryId })
        : new URLSearchParams(scope);
    return expectOk('GET', `/api/time-entry-targets?${params.toString()}`, (v) =>
      parseList(v, parseEntryTarget),
    );
  },
};
