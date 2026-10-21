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
    // 한쪽 시각을 고치면 두 끝을 함께 다시 변환한다(고치지 않은 끝도).
    expect(onsubmit.mock.calls[0]?.[0]).toMatchObject({
      starts_at: '2026-10-20T09:30:00.000Z',
      ends_at: '2026-10-20T10:00:00.000Z',
    });
  });

  async function saveWithTime(
    initial: EventView,
    label: string,
    wall: string,
  ): Promise<{ starts: number; ends: number; input: EventInput }> {
    const onsubmit = vi.fn<(input: EventInput) => Promise<void>>().mockResolvedValue(undefined);
    render(EventForm, { props: { idPrefix: 'ev', initial, submitLabel: '저장', onsubmit } });
    const field = await screen.findByLabelText(label);
    await fireEvent.input(field, { target: { value: wall } });
    const form = field.closest('form');
    if (form === null) throw new Error('form');
    await fireEvent.submit(form);
    await waitFor(() => {
      expect(onsubmit).toHaveBeenCalledTimes(1);
    });
    const input = onsubmit.mock.calls[0]?.[0];
    if (input?.starts_at == null || input.ends_at == null) throw new Error('input');
    return { starts: Date.parse(input.starts_at), ends: Date.parse(input.ends_at), input };
  }

  // 한쪽 끝만 원래 값으로 두고 다른 쪽만 다시 변환하면, 두 번 있는 시각(앞의 것을 고른다)이나 초 때문에 끝이 시작보다
  // 앞서 서버가 거절하거나 화면과 다른 길이가 된다. 보존 여부는 두 끝을 함께 정한다.
  it('keeps start before end when only the end changes in the fall-back hour', async () => {
    // 저장값은 두 번째 01:30~02:00(EST). 끝만 01:45로 고친다.
    const { starts, ends, input } = await saveWithTime(
      event('2026-11-01T06:30:00Z', '2026-11-01T07:00:00Z', 'America/New_York'),
      m.events_ends(),
      '2026-11-01T01:45',
    );
    expect(ends).toBeGreaterThanOrEqual(starts);
    expect(input).toMatchObject({
      starts_at: '2026-11-01T05:30:00.000Z',
      ends_at: '2026-11-01T05:45:00.000Z',
    });
  });

  it('keeps start before end when the end is moved onto a start stored with seconds', async () => {
    const { starts, ends } = await saveWithTime(
      event('2026-10-20T09:00:37Z', '2026-10-20T10:00:00Z', 'Asia/Seoul'),
      m.events_ends(),
      '2026-10-20T18:00',
    );
    expect(ends).toBeGreaterThanOrEqual(starts);
  });

  it('keeps the shown length when only the start changes in the fall-back hour', async () => {
    // 저장값은 두 번째 01:30~01:45(EST). 시작만 01:00으로 고치면 화면에 보이는 대로 45분이어야 한다.
    const { starts, ends } = await saveWithTime(
      event('2026-11-01T06:30:00Z', '2026-11-01T06:45:00Z', 'America/New_York'),
      m.events_starts(),
      '2026-11-01T01:00',
    );
    expect(ends - starts).toBe(45 * 60_000);
  });
});
