import { ApiError, expectOk, fail, request } from '$lib/shared/api';
import { isRecord, parseList, rec } from '$lib/shared/api/parse';
import type { RecurrenceRule } from '$lib/features/tasks/model';
import { parseEvent, type EventInput, type EventView } from './model';

export class RuleConflict extends ApiError {
  constructor(readonly current: EventView) {
    super(409, 'rule_conflict', 'rule_conflict');
    this.name = 'RuleConflict';
  }
}

const none = (): undefined => undefined;

export const eventsApi = {
  itemEvents: (itemId: string) =>
    expectOk('GET', `/api/items/${encodeURIComponent(itemId)}/events`, (v) =>
      parseList(v, parseEvent),
    ),
  event: (id: string) => expectOk('GET', `/api/events/${encodeURIComponent(id)}`, parseEvent),
  createEvent: (itemId: string, requestId: string, input: EventInput) =>
    expectOk('POST', `/api/items/${encodeURIComponent(itemId)}/events`, parseEvent, {
      request_id: requestId,
      ...input,
    }),
  /** 일정 고치기. 기준 버전이 지났으면 `{ kind: 'conflict', current }`. */
  async updateEvent(
    id: string,
    baseVersion: number,
    input: EventInput,
  ): Promise<{ kind: 'updated'; event: EventView } | { kind: 'conflict'; current: EventView }> {
    const { status, json } = await request('PUT', `/api/events/${encodeURIComponent(id)}`, {
      base_version: baseVersion,
      ...input,
    });
    if (status === 409 && isRecord(json) && json.error === 'conflict' && json.current !== undefined)
      return { kind: 'conflict', current: parseEvent(rec(json, 'conflict').current) };
    if (status !== 200) fail(status, json);
    return { kind: 'updated', event: parseEvent(json) };
  },
  deleteEvent: (id: string) => expectOk('DELETE', `/api/events/${encodeURIComponent(id)}`, none),
  /**
   * 일정 반복 정하기. `base`는 편집을 시작한 규칙(반복이 없었으면 null)이다. 그사이 규칙이 바뀌었으면 서버가 아무것도
   * 바꾸지 않고 409 `rule_conflict`와 지금 일정으로 거절한다(`RuleConflict`).
   */
  async setEventRecurrence(
    id: string,
    rule: RecurrenceRule | null,
    base: RecurrenceRule | null,
  ): Promise<EventView> {
    const { status, json } = await request(
      'PUT',
      `/api/events/${encodeURIComponent(id)}/recurrence`,
      { rule: rule === null ? null : { ...rule }, base: base === null ? null : { ...base } },
    );
    if (
      status === 409 &&
      isRecord(json) &&
      json.error === 'rule_conflict' &&
      json.current !== undefined
    )
      throw new RuleConflict(parseEvent(json.current));
    if (status !== 200) fail(status, json);
    return parseEvent(json);
  },
  excludeOccurrence: (id: string, date: string) =>
    expectOk('POST', `/api/events/${encodeURIComponent(id)}/exclusions`, none, { date }),
};
