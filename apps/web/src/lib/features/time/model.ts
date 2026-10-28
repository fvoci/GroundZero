import type { components } from '@fvoci/api-client';
import { rec, str, optStr, int, type Json } from '$lib/shared/api/parse';
type Schemas = components['schemas'];
export type TimeKind = Schemas['TimeKind'];
export type TimeTargetKind = Schemas['TargetKind'];
export const TIME_KINDS = [
  'work',
  'study',
  'lecture',
  'hobby',
  'rest',
  'other',
] as const satisfies readonly TimeKind[];
export type TimeTargetView = Required<Schemas['Target']>;
export type TimeEntryResource = Required<Schemas['TimeEntry']>;
export type TimeEntryView = Omit<TimeEntryResource, 'target'> & { target: TimeTargetView | null };
export type TimeEntryInput = Required<Schemas['EntryInput']>;
export type TimeSummaryRowView = Schemas['TimeSummaryRow'];
export type TaskTimeView = Required<Schemas['TaskTime']>;
export type ItemTimeView = Schemas['ItemTime'];
export type TimeEntryTarget = Required<Schemas['TimeEntryTarget']>;

export function timeKind(value: string): TimeKind {
  const found = TIME_KINDS.find((k) => k === value);
  if (found === undefined) throw new TypeError(`unknown time kind ${value}`);
  return found;
}

function parseTarget(value: Json): TimeTargetView {
  const t = rec(value, 'time target');
  const kind = str(t, 'kind');
  if (
    kind !== 'task' &&
    kind !== 'document' &&
    kind !== 'item' &&
    kind !== 'episode' &&
    kind !== 'study_unit'
  )
    throw new TypeError(`unknown target kind ${kind}`);
  return {
    kind,
    id: str(t, 'id'),
    label: optStr(t, 'label'),
    workspace_id: optStr(t, 'workspace_id'),
  };
}

export function parseTimeEntry(value: Json): TimeEntryView {
  const o = rec(value, 'time entry');
  const target = o.target === null || o.target === undefined ? null : parseTarget(o.target);
  return {
    id: str(o, 'id'),
    started_at: str(o, 'started_at'),
    ended_at: optStr(o, 'ended_at'),
    paused_at: optStr(o, 'paused_at'),
    paused_seconds: int(o, 'paused_seconds'),
    seconds: int(o, 'seconds'),
    kind: timeKind(str(o, 'kind')),
    memo: str(o, 'memo'),
    target,
    version: int(o, 'version'),
  };
}

/** 서버 자원에는 대상의 표시값을 섞지 않는다. 화면·초안 파서는 parseTimeEntry다. */
export function parseEntryResource(value: Json): TimeEntryResource {
  const entry = parseTimeEntry(value);
  return {
    ...entry,
    target: entry.target === null ? null : { kind: entry.target.kind, id: entry.target.id },
  };
}
export function parseEntryTarget(value: Json): TimeEntryTarget {
  const row = rec(value, 'entry target');
  return { entry_id: str(row, 'entry_id'), ...parseTarget(row) };
}
