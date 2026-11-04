import {
  EVENT_KINDS,
  parseEvent,
  type EventKind,
  type EventView,
  type EventInput,
} from '$lib/features/events/model';
import { FREQUENCIES, type Frequency, type RecurrenceRule } from '$lib/api/client';
import { parseRule } from '$lib/features/tasks/model';
import type { FieldSide } from '$lib/features/common/field-conflict';
import type { RuleValue } from './conflict';

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
export interface EventRawDraft {
  fields: EventFields;
  baseFields: EventFields;
  base: EventView | null;
  /** The temporal anchor can differ from the semantic edit baseline after applying choices. */
  source?: EventView | null | undefined;
  startChoice: string | null;
  endChoice: string | null;
  request: { id: string; payload: string } | null;
}
export interface EventComparison {
  base: EventView;
  baseVersion: number;
  mine: EventInput;
  server: EventView;
  mineDraft: EventRawDraft;
  original: EventRawDraft;
  originalVersion: number;
  choices: Record<string, FieldSide>;
  open: boolean;
  repeated: boolean;
}
export interface EventDraft extends EventRawDraft {
  comparison?: EventComparison | undefined;
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
function parseEventRaw(value: unknown): EventRawDraft | undefined {
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
      source: o.source === undefined ? undefined : o.source === null ? null : parseEvent(o.source),
      startChoice: o.startChoice,
      endChoice: o.endChoice,
      request: retry,
    };
  } catch {
    return;
  }
}
export function parseEventDraft(value: unknown): EventDraft | undefined {
  const raw = parseEventRaw(value),
    o = object(value);
  if (!raw || !o) return;
  if (o.comparison === undefined) return raw;
  const comparison = parseEventComparison(o.comparison);
  return comparison === undefined ? undefined : { ...raw, comparison };
}
function choices(value: unknown): Record<string, FieldSide> | undefined {
  const o = object(value);
  if (!o) return;
  return Object.fromEntries(
    Object.entries(o).filter(
      (entry): entry is [string, FieldSide] => entry[1] === 'mine' || entry[1] === 'server',
    ),
  );
}
function parseInput(value: unknown): EventInput | undefined {
  const o = object(value);
  const kind = EVENT_KINDS.find((k) => k === o?.kind);
  if (
    !o ||
    kind === undefined ||
    typeof o.title !== 'string' ||
    typeof o.description !== 'string' ||
    typeof o.all_day !== 'boolean' ||
    typeof o.timezone !== 'string' ||
    !(o.start_date === null || typeof o.start_date === 'string') ||
    !(o.end_date === null || typeof o.end_date === 'string') ||
    !(o.starts_at === null || typeof o.starts_at === 'string') ||
    !(o.ends_at === null || typeof o.ends_at === 'string')
  )
    return;
  return {
    kind,
    title: o.title,
    description: o.description,
    all_day: o.all_day,
    timezone: o.timezone,
    start_date: o.start_date,
    end_date: o.end_date,
    starts_at: o.starts_at,
    ends_at: o.ends_at,
  };
}
function parseEventComparison(value: unknown): EventComparison | undefined {
  const o = object(value);
  if (!o) return;
  const mine = parseInput(o.mine),
    mineDraft = parseEventRaw(o.mineDraft),
    original = parseEventRaw(o.original),
    picked = choices(o.choices);
  if (
    !mine ||
    !mineDraft ||
    !original ||
    !picked ||
    typeof o.baseVersion !== 'number' ||
    typeof o.originalVersion !== 'number' ||
    typeof o.open !== 'boolean' ||
    typeof o.repeated !== 'boolean'
  )
    return;
  try {
    return {
      base: parseEvent(o.base),
      baseVersion: o.baseVersion,
      mine,
      server: parseEvent(o.server),
      mineDraft,
      original,
      originalVersion: o.originalVersion,
      choices: picked,
      open: o.open,
      repeated: o.repeated,
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
export interface RuleRawDraft {
  fields: RuleFields;
  baseFields: RuleFields;
  base: RecurrenceRule | null;
}
export interface RuleComparison {
  base: RuleValue;
  mine: RuleValue;
  server: RuleValue;
  mineDraft: RuleRawDraft;
  original: RuleRawDraft;
  choices: Record<string, FieldSide>;
  open: boolean;
  repeated: boolean;
}
export interface RuleDraft extends RuleRawDraft {
  comparison?: RuleComparison | undefined;
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
function parseRuleRaw(value: unknown): RuleRawDraft | undefined {
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
export function parseRuleDraft(value: unknown): RuleDraft | undefined {
  const raw = parseRuleRaw(value),
    o = object(value);
  if (!raw || !o) return;
  if (o.comparison === undefined) return raw;
  const c = object(o.comparison);
  if (!c) return;
  const mineDraft = parseRuleRaw(c.mineDraft),
    original = parseRuleRaw(c.original),
    picked = choices(c.choices);
  const base = object(c.base),
    mine = object(c.mine),
    server = object(c.server);
  if (
    !mineDraft ||
    !original ||
    !picked ||
    !base ||
    !mine ||
    !server ||
    typeof c.open !== 'boolean' ||
    typeof c.repeated !== 'boolean'
  )
    return;
  try {
    return {
      ...raw,
      comparison: {
        base: { rule: parseRule(base.rule) },
        mine: { rule: parseRule(mine.rule) },
        server: { rule: parseRule(server.rule) },
        mineDraft,
        original,
        choices: picked,
        open: c.open,
        repeated: c.repeated,
      },
    };
  } catch {
    return;
  }
}
