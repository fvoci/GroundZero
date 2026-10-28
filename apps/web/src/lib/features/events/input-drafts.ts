import {
  EVENT_KINDS,
  parseEvent,
  type EventKind,
  type EventView,
} from '$lib/features/events/model';
import { FREQUENCIES, type Frequency, type RecurrenceRule } from '$lib/api/client';

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
export function object(value: unknown): Record<string, unknown> | undefined {
  return isObject(value) ? value : undefined;
}

export interface EventFields {
  kind: EventKind;
  title: string;
  description: string;
  allDay: boolean;
  startDate: string;
  endDate: string;
  startWall: string;
  endWall: string;
  timezone: string;
}
export interface EventDraft {
  fields: EventFields;
  baseFields: EventFields;
  base: EventView | null;
  startChoice: string | null;
  endChoice: string | null;
  request: { id: string; payload: string } | null;
}
function fields(value: unknown): EventFields | undefined {
  const o = object(value);
  if (!o) return;
  const kind = EVENT_KINDS.find((k) => k === o.kind);
  if (
    !kind ||
    typeof o.title !== 'string' ||
    typeof o.description !== 'string' ||
    typeof o.allDay !== 'boolean' ||
    typeof o.startDate !== 'string' ||
    typeof o.endDate !== 'string' ||
    typeof o.startWall !== 'string' ||
    typeof o.endWall !== 'string' ||
    typeof o.timezone !== 'string'
  )
    return;
  return {
    kind,
    title: o.title,
    description: o.description,
    allDay: o.allDay,
    startDate: o.startDate,
    endDate: o.endDate,
    startWall: o.startWall,
    endWall: o.endWall,
    timezone: o.timezone,
  };
}
export function request(value: unknown): { id: string; payload: string } | null | undefined {
  if (value === null) return null;
  const o = object(value);
  return o && typeof o.id === 'string' && typeof o.payload === 'string'
    ? { id: o.id, payload: o.payload }
    : undefined;
}
export function parseEventDraft(value: unknown): EventDraft | undefined {
  const o = object(value);
  if (!o) return;
  const f = fields(o.fields),
    baseFields = fields(o.baseFields),
    retry = request(o.request);
  if (
    !f ||
    !baseFields ||
    retry === undefined ||
    !(o.startChoice === null || typeof o.startChoice === 'string') ||
    !(o.endChoice === null || typeof o.endChoice === 'string')
  )
    return;
  try {
    return {
      fields: f,
      baseFields,
      base: o.base === null ? null : parseEvent(o.base),
      startChoice: o.startChoice,
      endChoice: o.endChoice,
      request: retry,
    };
  } catch {
    return;
  }
}
export interface RuleFields {
  freq: Frequency | 'none';
  interval: string;
  weekdays: number[];
  end: 'never' | 'until' | 'count';
  until: string;
  count: string;
}
export interface RuleDraft {
  fields: RuleFields;
  baseFields: RuleFields;
  base: RecurrenceRule | null;
}
function ruleFields(value: unknown): RuleFields | undefined {
  const o = object(value);
  if (!o) return;
  const freq = o.freq === 'none' ? 'none' : FREQUENCIES.find((f) => f === o.freq);
  if (
    !freq ||
    typeof o.interval !== 'string' ||
    !Array.isArray(o.weekdays) ||
    !o.weekdays.every(
      (d): d is number => typeof d === 'number' && Number.isInteger(d) && d >= 1 && d <= 7,
    ) ||
    (o.end !== 'never' && o.end !== 'until' && o.end !== 'count') ||
    typeof o.until !== 'string' ||
    typeof o.count !== 'string'
  )
    return;
  return {
    freq,
    interval: o.interval,
    weekdays: o.weekdays,
    end: o.end,
    until: o.until,
    count: o.count,
  };
}
export function parseRuleDraft(value: unknown): RuleDraft | undefined {
  const o = object(value);
  if (!o) return;
  const f = ruleFields(o.fields),
    baseFields = ruleFields(o.baseFields),
    b = object(o.base);
  if (!f || !baseFields) return;
  if (o.base === null) return { fields: f, baseFields, base: null };
  const freq = FREQUENCIES.find((f) => f === b?.freq);
  if (
    !b ||
    !freq ||
    typeof b.interval !== 'number' ||
    !Array.isArray(b.weekdays) ||
    !b.weekdays.every((d): d is number => typeof d === 'number') ||
    !(b.until === null || typeof b.until === 'string') ||
    !(b.count === null || typeof b.count === 'number')
  )
    return;
  return {
    fields: f,
    baseFields,
    base: { freq, interval: b.interval, weekdays: b.weekdays, until: b.until, count: b.count },
  };
}
