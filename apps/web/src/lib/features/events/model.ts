import type { components } from '@fvoci/api-client';
import { parseRule, type RecurrenceRule } from '$lib/features/tasks/model';
import { access, bool, int, optStr, rec, str, strings, type Json } from '$lib/shared/api/parse';
type Schemas = components['schemas'];

export const EVENT_KINDS = ['meeting', 'presentation', 'event'] as const;
export type EventKind = Schemas['EventKind'];

export type EventView = Omit<
  Schemas['Event'],
  'start_date' | 'end_date' | 'starts_at' | 'ends_at' | 'recurrence'
> & {
  start_date: string | null;
  end_date: string | null;
  starts_at: string | null;
  ends_at: string | null;
  recurrence: RecurrenceRule | null;
};

export type EventInput = Required<Schemas['EventInput']>;

export function parseEvent(value: Json): EventView {
  const o = rec(value, 'event');
  const kind = EVENT_KINDS.find((k) => k === str(o, 'kind'));
  if (kind === undefined) throw new TypeError(`unknown event kind ${str(o, 'kind')}`);
  return {
    id: str(o, 'id'),
    item_id: str(o, 'item_id'),
    workspace_id: str(o, 'workspace_id'),
    kind,
    title: str(o, 'title'),
    description: str(o, 'description'),
    all_day: bool(o, 'all_day'),
    start_date: optStr(o, 'start_date'),
    end_date: optStr(o, 'end_date'),
    starts_at: optStr(o, 'starts_at'),
    ends_at: optStr(o, 'ends_at'),
    timezone: str(o, 'timezone'),
    recurrence: parseRule(o.recurrence),
    excluded_dates: strings(o.excluded_dates),
    version: int(o, 'version'),
    access: access(str(o, 'access')),
  };
}
