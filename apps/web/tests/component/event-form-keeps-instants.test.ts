import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { EventInput, EventView } from '$lib/api/client';
import { ApiError } from '$lib/api/client';
import EventForm from '$lib/features/events/EventForm.svelte';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';

const NY = 'America/New_York';

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

function checked(el: HTMLElement): boolean {
  return el instanceof HTMLInputElement && el.checked;
}

// 일정 입력은 분 단위라 저장 때마다 다시 변환하면 초가 지워지고, 두 번 있는 시각(여름시간 끝)은 한쪽으로 바뀐다.
// 고칠 때(D-93) 끝마다 따로 정한다: 손대지 않은 끝(그 칸의 시각과 시간대가 그대로)은 저장된 RFC3339 값을 그대로 보내고,
// 고친 끝이 두 번 있는 시각이면 보내기 전에 어느 쪽인지 묻는다. 시간대를 바꾸면 두 끝 모두 고친 것으로 본다.
describe('event form keeps stored instants when only other fields change', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
    // 고르는 항목의 시각은 표시 설정(D-81)을 따르므로 한국어·24시간 표시로 본다.
    Object.assign(me, { locale: 'ko', hourCycle: 'h23' });
  });
  afterEach(() => {
    Object.assign(me, { locale: '', hourCycle: '' });
    vi.restoreAllMocks();
  });

  function renderForm(
    initial: EventView | undefined,
    onsubmit = vi.fn<(input: EventInput) => Promise<void>>().mockResolvedValue(undefined),
  ): typeof onsubmit {
    render(EventForm, { props: { idPrefix: 'ev', initial, submitLabel: '저장', onsubmit } });
    return onsubmit;
  }

  async function submit(field: HTMLElement): Promise<void> {
    const form = field.closest('form');
    if (form === null) throw new Error('form');
    await fireEvent.submit(form);
  }

  async function sent(onsubmit: ReturnType<typeof renderForm>, times = 1): Promise<EventInput> {
    await waitFor(() => {
      expect(onsubmit).toHaveBeenCalledTimes(times);
    });
    const input = onsubmit.mock.calls[times - 1]?.[0];
    if (input === undefined) throw new Error('input');
    return input;
  }

  async function saveWithTitle(initial: EventView, title: string): Promise<EventInput> {
    const onsubmit = renderForm(initial);
    const field = await screen.findByLabelText(m.events_title());
    await fireEvent.input(field, { target: { value: title } });
    await submit(field);
    return sent(onsubmit);
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
      event('2026-11-01T06:30:00Z', '2026-11-01T07:00:00Z', NY),
      '제목만 고침',
    );
    expect(input.starts_at).toBe('2026-11-01T06:30:00Z');
    expect(input.ends_at).toBe('2026-11-01T07:00:00Z');
  });

  it('title-only edit in the fall-back hour asks nothing and sends both stored values byte-identical', async () => {
    // 두 끝 모두 두 번 있는 시각(두 번째 01:30:15, 두 번째 01:59:59)이지만 손대지 않았으므로 묻지 않는다.
    const onsubmit = renderForm(event('2026-11-01T06:30:15Z', '2026-11-01T06:59:59Z', NY));
    const title = await screen.findByLabelText(m.events_title());
    await fireEvent.input(title, { target: { value: '제목만 고침' } });
    expect(screen.queryAllByRole('radio')).toHaveLength(0);
    await submit(title);
    const input = await sent(onsubmit);
    expect(input.starts_at).toBe('2026-11-01T06:30:15Z');
    expect(input.ends_at).toBe('2026-11-01T06:59:59Z');
    expect(input.timezone).toBe(NY);
  });

  it('editing only the start reads that start and keeps the stored end exactly', async () => {
    const onsubmit = renderForm(
      event('2026-10-20T09:00:37Z', '2026-10-20T10:00:45Z', 'Asia/Seoul'),
    );
    const start = await screen.findByLabelText(m.events_starts());
    await fireEvent.input(start, { target: { value: '2026-10-20T18:30' } });
    await submit(start);
    // 고친 시작만 다시 읽고, 손대지 않은 끝은 초까지 저장된 값 그대로.
    expect(await sent(onsubmit)).toMatchObject({
      starts_at: '2026-10-20T09:30:00.000Z',
      ends_at: '2026-10-20T10:00:45Z',
    });
  });

  it('editing only the end in the fall-back hour asks which one and keeps the stored start', async () => {
    // 저장값은 두 번째 01:30~02:00(EST). 끝만 01:55로 고친다. 01:55도 두 번 있으므로 보내기 전에 묻는다.
    const onsubmit = renderForm(event('2026-11-01T06:30:00Z', '2026-11-01T07:00:00Z', NY));
    const end = await screen.findByLabelText(m.events_ends());
    end.focus();
    await fireEvent.input(end, { target: { value: '2026-11-01T01:55' } });
    // 묶음이 나타나도 입력란의 포커스는 그대로다.
    expect(document.activeElement).toBe(end);
    const group = screen.getByRole('group', {
      name: m.events_repeated_time_legend({ field: m.events_ends(), zone: NY }),
    });
    expect(end.getAttribute('aria-describedby')).toContain('ev-ends-repeated-legend');
    const first = screen.getByRole('radio', { name: '첫 번째 01:55 (UTC−04:00)' });
    const second = screen.getByRole('radio', { name: '두 번째 01:55 (UTC−05:00)' });
    expect(group.contains(first) && group.contains(second)).toBe(true);
    // 미리 골라 두지 않는다. 시작 칸은 손대지 않았으므로 묻지 않는다.
    expect(checked(first) || checked(second)).toBe(false);
    expect(screen.getAllByRole('radio')).toHaveLength(2);

    // 고르지 않고 저장하면 보내지 않고 그 칸 옆에 알린다.
    await submit(end);
    await waitFor(() => {
      expect(end.getAttribute('aria-invalid')).toBe('true');
    });
    expect(screen.getByRole('alert').textContent).toContain(
      m.events_repeated_time_required({ field: m.events_ends() }),
    );
    expect(end.getAttribute('aria-describedby')).toContain('ev-ends-repeated-error');
    expect(onsubmit).not.toHaveBeenCalled();

    // 두 번째를 고르면 오류가 사라지고, 같은 항목이 남아 포커스를 잃지 않는다.
    second.focus();
    await fireEvent.click(second);
    expect(checked(second)).toBe(true);
    expect(screen.getByRole('radio', { name: '두 번째 01:55 (UTC−05:00)' })).toBe(second);
    expect(document.activeElement).toBe(second);
    expect(end.getAttribute('aria-invalid')).not.toBe('true');
    expect(screen.queryByRole('alert')).toBeNull();

    await submit(end);
    const input = await sent(onsubmit);
    // 시작은 저장된 두 번째 01:30 그대로(한 시간 당겨지지 않음), 끝은 고른 두 번째 01:55.
    expect(input.starts_at).toBe('2026-11-01T06:30:00Z');
    expect(input.ends_at).toBe('2026-11-01T06:55:00.000Z');
  });

  it('a choice does not carry over once that time is typed again', async () => {
    const onsubmit = renderForm(event('2026-11-01T06:30:00Z', '2026-11-01T07:00:00Z', NY));
    const end = await screen.findByLabelText(m.events_ends());
    await fireEvent.input(end, { target: { value: '2026-11-01T01:55' } });
    await fireEvent.click(screen.getByRole('radio', { name: '두 번째 01:55 (UTC−05:00)' }));
    await fireEvent.input(end, { target: { value: '2026-11-01T01:45' } });
    const second = screen.getByRole('radio', { name: '두 번째 01:45 (UTC−05:00)' });
    expect(checked(second)).toBe(false);
    await submit(end);
    await waitFor(() => {
      expect(end.getAttribute('aria-invalid')).toBe('true');
    });
    expect(onsubmit).not.toHaveBeenCalled();
    // 다시 보통 시각으로 고치면 묶음과 오류가 함께 사라진다.
    await fireEvent.input(end, { target: { value: '2026-11-01T02:15' } });
    expect(screen.queryAllByRole('radio')).toHaveLength(0);
    expect(end.getAttribute('aria-invalid')).not.toBe('true');
  });

  it('editing only the start in the fall-back hour asks which one and keeps the stored end', async () => {
    // 저장값은 두 번째 01:30~01:45(EST). 시작만 01:00으로 고치면(두 번 있다) 묻고, 두 번째를 고르면 화면대로 45분이다.
    const onsubmit = renderForm(event('2026-11-01T06:30:00Z', '2026-11-01T06:45:00Z', NY));
    const start = await screen.findByLabelText(m.events_starts());
    await fireEvent.input(start, { target: { value: '2026-11-01T01:00' } });
    await submit(start);
    await waitFor(() => {
      expect(start.getAttribute('aria-invalid')).toBe('true');
    });
    expect(onsubmit).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByRole('radio', { name: '두 번째 01:00 (UTC−05:00)' }));
    await submit(start);
    const input = await sent(onsubmit);
    expect(input.starts_at).toBe('2026-11-01T06:00:00.000Z');
    expect(input.ends_at).toBe('2026-11-01T06:45:00Z');
  });

  it('changing the time zone reads both ends again and asks when one is repeated', async () => {
    // 서울 2026-11-01 01:30:20~02:00으로 저장된 일정의 시간대를 뉴욕으로 바꾼다. 두 끝 모두 새 시간대로 다시 읽고(초는
    // 입력란에 없으므로 지워진다), 뉴욕에서 두 번 있는 01:30(시작)만 묻는다. 02:00은 한 번뿐이다.
    const onsubmit = renderForm(
      event('2026-10-31T16:30:20Z', '2026-10-31T17:00:00Z', 'Asia/Seoul'),
    );
    const zone = await screen.findByLabelText(m.events_time_zone());
    expect(screen.queryAllByRole('radio')).toHaveLength(0);
    await fireEvent.input(zone, { target: { value: NY } });
    expect(
      screen.getByRole('group', {
        name: m.events_repeated_time_legend({ field: m.events_starts(), zone: NY }),
      }),
    ).toBeTruthy();
    expect(screen.getAllByRole('radio')).toHaveLength(2);
    await submit(zone);
    await waitFor(() => {
      expect(screen.getByLabelText(m.events_starts()).getAttribute('aria-invalid')).toBe('true');
    });
    expect(screen.getByLabelText(m.events_ends()).getAttribute('aria-invalid')).not.toBe('true');
    expect(onsubmit).not.toHaveBeenCalled();

    await fireEvent.click(screen.getByRole('radio', { name: '첫 번째 01:30 (UTC−04:00)' }));
    await submit(zone);
    expect(await sent(onsubmit)).toMatchObject({
      starts_at: '2026-11-01T05:30:00.000Z',
      ends_at: '2026-11-01T07:00:00.000Z',
      timezone: NY,
    });
  });

  it('sends an end before the kept start as typed and shows the server refusal', async () => {
    // 시작은 초까지 그대로(09:00:37Z), 끝을 같은 분(18:00 = 09:00:00Z)으로 고치면 끝이 시작보다 37초 앞선다. 화면이
    // 먼저 막지 않고 서버가 거절하며(400), 화면은 그 거절을 그대로 알린다(D-93 (3)).
    const onsubmit = renderForm(
      event('2026-10-20T09:00:37Z', '2026-10-20T10:00:00Z', 'Asia/Seoul'),
      vi
        .fn<(input: EventInput) => Promise<void>>()
        .mockRejectedValue(new ApiError(400, 'bad_request', 'end must not be before start')),
    );
    const end = await screen.findByLabelText(m.events_ends());
    await fireEvent.input(end, { target: { value: '2026-10-20T18:00' } });
    await submit(end);
    expect(await sent(onsubmit)).toMatchObject({
      starts_at: '2026-10-20T09:00:37Z',
      ends_at: '2026-10-20T09:00:00.000Z',
    });
    await waitFor(() => {
      expect(screen.getByRole('alert').textContent).toContain(m.events_save_failed());
    });
  });

  it('a new event at a repeated time still takes the earlier one without asking (D-81)', async () => {
    const onsubmit = renderForm(undefined);
    await fireEvent.input(await screen.findByLabelText(m.events_title()), {
      target: { value: '새 일정' },
    });
    await fireEvent.input(screen.getByLabelText(m.events_time_zone()), { target: { value: NY } });
    await fireEvent.input(screen.getByLabelText(m.events_starts()), {
      target: { value: '2026-11-01T01:30' },
    });
    const end = screen.getByLabelText(m.events_ends());
    await fireEvent.input(end, { target: { value: '2026-11-01T01:45' } });
    expect(screen.queryAllByRole('radio')).toHaveLength(0);
    await submit(end);
    expect(await sent(onsubmit)).toMatchObject({
      starts_at: '2026-11-01T05:30:00.000Z',
      ends_at: '2026-11-01T05:45:00.000Z',
      timezone: NY,
    });
  });

  // 검증 리뷰: 고친 시각이 없는 시각(봄 여름시간 시작)이면 지금처럼 보내지 않고 알린다(D-81).
  it('editing to a skipped spring time is refused with the missing-time message', async () => {
    const onsubmit = renderForm(event('2026-03-08T07:30:00Z', '2026-03-08T08:00:00Z', NY));
    const start = await screen.findByLabelText(m.events_starts());
    await fireEvent.input(start, { target: { value: '2026-03-08T02:30' } });
    await submit(start);
    await waitFor(() => {
      expect(screen.getByRole('alert').textContent).toContain(m.events_missing_time({ zone: NY }));
    });
    expect(onsubmit).not.toHaveBeenCalled();
  });

  // 검증 리뷰: 다시 막힌 저장도 화면 낭독기가 다시 읽도록 칸 옆 알림을 새로 그린다.
  it('announces a blocked save again each time', async () => {
    const onsubmit = renderForm(event('2026-11-01T06:30:00Z', '2026-11-01T07:00:00Z', NY));
    const end = await screen.findByLabelText(m.events_ends());
    await fireEvent.input(end, { target: { value: '2026-11-01T01:55' } });
    await submit(end);
    const firstAlert = await screen.findByRole('alert');
    await submit(end);
    await waitFor(() => {
      expect(screen.getByRole('alert')).not.toBe(firstAlert);
    });
    expect(screen.getByRole('alert').textContent).toContain(
      m.events_repeated_time_required({ field: m.events_ends() }),
    );
    expect(onsubmit).not.toHaveBeenCalled();
  });

  // 검증 리뷰: 종일로 바꿨다 되돌리면 앞서 막힌 "고르세요" 알림이 저장 시도 없이 다시 나타났다.
  it('turning all day on and off clears an earlier choose-one alert', async () => {
    renderForm(event('2026-11-01T06:30:00Z', '2026-11-01T07:00:00Z', NY));
    const end = await screen.findByLabelText(m.events_ends());
    await fireEvent.input(end, { target: { value: '2026-11-01T01:55' } });
    await submit(end);
    await screen.findByRole('alert');
    const allDay = screen.getByRole('checkbox', { name: m.events_all_day() });
    await fireEvent.click(allDay);
    await fireEvent.click(allDay);
    const again = await screen.findByLabelText(m.events_ends());
    expect(screen.queryByRole('alert')).toBeNull();
    expect(again.getAttribute('aria-invalid')).not.toBe('true');
  });
});
