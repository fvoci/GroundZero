import { ApiError, EXPECTED_USER, expectOk, fail, request } from '$lib/shared/api';
import { bool, isRecord, parseList, rec } from '$lib/shared/api/parse';
import type { RecurrenceRule } from '$lib/features/tasks/model';
import { parseEvent, type EventInput, type EventView, type EventRead } from './model';
import { me } from '$lib/me.svelte';
import { untrack } from 'svelte';
import { accountSafety, switchAccount } from '$lib/features/feedback/feedback.svelte';
import { DiscardedTaskRead, readTaskSessionGeneration } from '$lib/features/tasks/context-read';

export class RuleConflict extends ApiError {
  constructor(readonly current: EventView) {
    super(409, 'rule_conflict', 'rule_conflict');
    this.name = 'RuleConflict';
  }
}

export const eventsApi = {
  itemEvents: (itemId: string) =>
    expectOk('GET', `/api/items/${encodeURIComponent(itemId)}/events`, (v) =>
      parseList(v, parseEvent),
    ),
  event: async (id: string, user = me.userId): Promise<EventRead> => {
    const generation = readTaskSessionGeneration();
    const safety = untrack(() => accountSafety.generation);
    const current = () =>
      me.userId === user &&
      generation === readTaskSessionGeneration() &&
      safety === accountSafety.generation;
    const response = await request('GET', `/api/events/${encodeURIComponent(id)}`).catch(
      (cause: unknown) => {
        if (!current()) throw new DiscardedTaskRead();
        throw cause;
      },
    );
    if (!current()) throw new DiscardedTaskRead();
    if (response.status !== 200) fail(response.status, response.json);
    if (response.viewerId !== user) {
      switchAccount();
      throw new ApiError(409, 'account_changed', 'account_changed');
    }
    return {
      ...parseEvent(response.json),
      viewer_id: response.viewerId,
      archived: bool(rec(response.json, 'event detail'), 'archived'),
    };
  },
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
    user = me.userId,
  ): Promise<{ kind: 'updated'; event: EventView } | { kind: 'conflict'; current: EventView }> {
    const { status, json } = await request(
      'PUT',
      `/api/events/${encodeURIComponent(id)}`,
      {
        base_version: baseVersion,
        ...input,
      },
      { [EXPECTED_USER]: user },
    );
    if (status === 409 && isRecord(json) && json.error === 'conflict' && json.current !== undefined)
      return { kind: 'conflict', current: parseEvent(rec(json, 'conflict').current) };
    if (status !== 200) fail(status, json);
    return { kind: 'updated', event: parseEvent(json) };
  },
  async deleteEvent(id: string, user = me.userId): Promise<void> {
    const { status, json } = await request(
      'DELETE',
      `/api/events/${encodeURIComponent(id)}`,
      undefined,
      { [EXPECTED_USER]: user },
    );
    if (status < 200 || status >= 300) fail(status, json);
  },
  /**
   * 일정 반복 정하기. `base`는 편집을 시작한 규칙(반복이 없었으면 null)이다. 그사이 규칙이 바뀌었으면 서버가 아무것도
   * 바꾸지 않고 409 `rule_conflict`와 지금 일정으로 거절한다(`RuleConflict`).
   */
  async setEventRecurrence(
    id: string,
    rule: RecurrenceRule | null,
    base: RecurrenceRule | null,
    user = me.userId,
  ): Promise<EventView> {
    const { status, json } = await request(
      'PUT',
      `/api/events/${encodeURIComponent(id)}/recurrence`,
      { rule: rule === null ? null : { ...rule }, base: base === null ? null : { ...base } },
      { [EXPECTED_USER]: user },
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
  async excludeOccurrence(id: string, date: string, user = me.userId): Promise<void> {
    const { status, json } = await request(
      'POST',
      `/api/events/${encodeURIComponent(id)}/exclusions`,
      { date },
      { [EXPECTED_USER]: user },
    );
    if (status < 200 || status >= 300) fail(status, json);
  },
};
