import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { EventInput, EventView } from '$lib/api/client';
import EventForm from '$lib/features/events/EventForm.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';

function event(startsAt: string, endsAt: string, timezone: string): EventView {
  return {
    id: 'e1',
    item_id: 'i1',
    workspace_id: 'ws-1',
    kind: 'meeting',
    title: '처음 제목',
    description: '',
    all_day: false,
    start_date: null,
    end_date: null,
    starts_at: startsAt,
    ends_at: endsAt,
    timezone,
    recurrence: null,
    excluded_dates: [],
    version: 1,
    access: 'write',
  };
}

// 일정 입력은 분 단위라 저장 때마다 다시 변환하면 초가 지워지고, 두 번 있는 시각(여름시간 끝)은 앞의 것으로 바뀐다.
// 시각·시간대를 고치지 않았으면 원래 RFC3339 값을 그대로 보낸다.
describe('event form keeps stored instants when only other fields change', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
  });
  afterEach(() => {
    vi.restoreAllMocks();
  });

  async function saveWithTitle(initial: EventView, title: string): Promise<EventInput> {
    const onsubmit = vi.fn<(input: EventInput) => Promise<void>>().mockResolvedValue(undefined);
    render(EventForm, { props: { idPrefix: 'ev', initial, submitLabel: '저장', onsubmit } });
    const field = await screen.findByLabelText(m.events_title());
    await fireEvent.input(field, { target: { value: title } });
    const form = field.closest('form');
    if (form === null) throw new Error('form');
    await fireEvent.submit(form);
    await waitFor(() => {
      expect(onsubmit).toHaveBeenCalledTimes(1);
    });
    const input = onsubmit.mock.calls[0]?.[0];
    if (input === undefined) throw new Error('input');
    return input;
  }

  it('keeps the seconds of a stored start and end', async () => {
    const input = await saveWithTitle(
      event('2026-10-20T09:00:37Z', '2026-10-20T10:00:00Z', 'Asia/Seoul'),
      '제목만 고침',
    );
    expect(input.title).toBe('제목만 고침');
    expect(input.starts_at).toBe('2026-10-20T09:00:37Z');
    expect(input.ends_at).toBe('2026-10-20T10:00:00Z');
    expect(input.timezone).toBe('Asia/Seoul');
  });

  it('keeps the second 01:30 of the New York fall-back night instead of switching to the first', async () => {
    // 2026-11-01 01:30 뉴욕은 두 번 있다: 05:30Z(EDT)와 06:30Z(EST). 저장된 것은 뒤의 것이다.
    const input = await saveWithTitle(
      event('2026-11-01T06:30:00Z', '2026-11-01T07:00:00Z', 'America/New_York'),
      '제목만 고침',
    );
    expect(input.starts_at).toBe('2026-11-01T06:30:00Z');
    expect(input.ends_at).toBe('2026-11-01T07:00:00Z');
  });

  it('converts again once the time itself is changed', async () => {
    const onsubmit = vi.fn<(input: EventInput) => Promise<void>>().mockResolvedValue(undefined);
    render(EventForm, {
      props: {
        idPrefix: 'ev',
        initial: event('2026-10-20T09:00:37Z', '2026-10-20T10:00:00Z', 'Asia/Seoul'),
        submitLabel: '저장',
        onsubmit,
      },
    });
    const start = await screen.findByLabelText(m.events_starts());
    await fireEvent.input(start, { target: { value: '2026-10-20T18:30' } });
    const form = start.closest('form');
    if (form === null) throw new Error('form');
    await fireEvent.submit(form);
    await waitFor(() => {
      expect(onsubmit).toHaveBeenCalledTimes(1);
    });
    expect(onsubmit.mock.calls[0]?.[0]).toMatchObject({
      starts_at: '2026-10-20T09:30:00.000Z',
      ends_at: '2026-10-20T10:00:00Z',
    });
  });
});
