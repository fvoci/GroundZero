import { type EventView } from '$lib/features/events/model';
import { RuleConflict, eventsApi } from '$lib/features/events/api';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { RecurrenceRule } from '$lib/api/client';

import EventDialog from '$lib/features/events/EventDialog.svelte';
import { describeRule } from '$lib/features/events/labels';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';

// 감사 W21(서버 0086): 반복 규칙 저장은 화면이 보이는 규칙을 기준(`base`)으로 보낸다. 그사이 다른 사람이 규칙을 바꿨으면
// 서버가 409 `rule_conflict`로 거절하고, 화면은 지금 규칙을 요약에 보이며 입력한 칸은 그대로 둔다. 다시 저장하면 새
// 기준으로 보내 입력한 규칙으로 바꾼다.
afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

const monday: RecurrenceRule = {
  freq: 'weekly',
  interval: 1,
  weekdays: [1],
  until: null,
  count: null,
};
const monWed: RecurrenceRule = { ...monday, weekdays: [1, 3] };

function ev(version: number, recurrence: RecurrenceRule | null): EventView {
  return {
    id: 'A',
    item_id: 'i1',
    workspace_id: 'ws-1',
    kind: 'meeting',
    title: '회의',
    description: '',
    all_day: true,
    start_date: '2026-10-19',
    end_date: '2026-10-19',
    starts_at: null,
    ends_at: null,
    timezone: 'Asia/Seoul',
    recurrence,
    excluded_dates: [],
    version,
    access: 'write',
  };
}

function ruleForm(): HTMLFormElement {
  const form = document.querySelector('[aria-labelledby="event-recurrence-heading"] form');
  if (!(form instanceof HTMLFormElement)) throw new Error('no recurrence form');
  return form;
}

function reply(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });
}

describe('event rule saved from a stale rule', () => {
  it('shows the current rule, keeps the typed one, and saves it on the next try', async () => {
    await setLocale('ko', { reload: false });
    vi.spyOn(eventsApi, 'event').mockResolvedValue(ev(5, monday));
    const setRule = vi
      .spyOn(eventsApi, 'setEventRecurrence')
      .mockRejectedValueOnce(new RuleConflict(ev(6, monWed)))
      .mockImplementation((_id: string, rule: RecurrenceRule | null) =>
        Promise.resolve(ev(7, rule)),
      );
    render(EventDialog, {
      props: { eventId: 'A', onclose: () => undefined, onchanged: () => undefined },
    });
    await screen.findByDisplayValue('회의');

    // 금요일을 더해 저장한다(화면은 월요일만 안다).
    await fireEvent.click(document.getElementById('event-recurrence-wd-5') ?? document.body);
    await fireEvent.submit(ruleForm());
    await waitFor(() => {
      expect(setRule).toHaveBeenCalledTimes(1);
    });
    expect(setRule.mock.calls[0]?.[1]?.weekdays).toEqual([1, 5]);
    expect(setRule.mock.calls[0]?.[2]).toEqual(monday);
    expect(await screen.findByText(m.events_rule_changed_meanwhile())).toBeTruthy();
    // 입력한 금요일은 남는다.
    expect(document.getElementById('event-recurrence-wd-5')?.getAttribute('aria-checked')).toBe(
      'true',
    );

    // 다시 저장하면 지금 규칙(월·수)을 기준으로 보낸다.
    await fireEvent.submit(ruleForm());
    await waitFor(() => {
      expect(setRule).toHaveBeenCalledTimes(2);
    });
    expect(setRule.mock.calls[1]?.[2]).toEqual(monWed);
    expect(setRule.mock.calls[1]?.[1]?.weekdays).toEqual([1, 5]);
  });

  // 리뷰: 충돌 뒤 일정을 따로 다시 읽었는데, 그 읽기가 실패하면 옛 규칙이 남아 다시 저장해도 옛 기준을 보내 같은 409만
  // 되풀이했다. 서버는 409에 지금 일정을 함께 주므로 그것을 쓴다(다시 읽기에 기대지 않는다).
  it('takes the current rule from the conflict reply even when the event cannot be read again', async () => {
    await setLocale('ko', { reload: false });
    const puts: unknown[] = [];
    let gets = 0;
    vi.stubGlobal('fetch', (input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === 'string' ? input : input instanceof URL ? input.href : input.url;
      const method = init?.method ?? 'GET';
      if (url === '/api/events/A' && method === 'GET') {
        gets += 1;
        return Promise.resolve(
          gets === 1
            ? reply(200, ev(5, monday))
            : reply(500, { error: 'internal', message: 'read failed' }),
        );
      }
      if (url === '/api/events/A/recurrence' && method === 'PUT') {
        const raw = init?.body;
        puts.push(typeof raw === 'string' ? JSON.parse(raw) : null);
        return Promise.resolve(
          puts.length === 1
            ? reply(409, { error: 'rule_conflict', current: ev(6, monWed) })
            : reply(200, ev(7, { ...monWed, weekdays: [1, 5] })),
        );
      }
      return Promise.resolve(reply(404, { error: 'not_found', message: url }));
    });
    render(EventDialog, {
      props: { eventId: 'A', onclose: () => undefined, onchanged: () => undefined },
    });
    await screen.findByDisplayValue('회의');

    await fireEvent.click(document.getElementById('event-recurrence-wd-5') ?? document.body);
    await fireEvent.submit(ruleForm());
    expect(await screen.findByText(m.events_rule_changed_meanwhile())).toBeTruthy();
    const summary =
      document.querySelector('[aria-labelledby="event-recurrence-heading"] > p')?.textContent ?? '';

    // 다시 저장하면 409가 알려 준 지금 규칙(월·수)을 기준으로 보낸다.
    await fireEvent.submit(ruleForm());
    await waitFor(() => {
      expect(puts).toHaveLength(2);
    });
    expect(puts[1]).toMatchObject({ base: monWed, rule: { weekdays: [1, 5] } });
    expect(summary).toContain(describeRule(monWed));
    // 충돌 뒤 일정을 따로 다시 읽지 않는다.
    expect(gets).toBe(1);
  });
});
import './fixtures/event-user';
