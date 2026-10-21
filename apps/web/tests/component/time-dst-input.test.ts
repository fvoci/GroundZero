import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { TimeEntryView } from '$lib/api/client';
import { api } from '$lib/api/client';
import TimeEntryDialog from '$lib/features/time/TimeEntryDialog.svelte';
import TimeScreen from '$lib/features/time/TimeScreen.svelte';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';

const ZONE = 'America/New_York';

// 여름시간이 시작되는 날(2026-03-08, 뉴욕)의 02:00~03:00은 없는 시각이다(D-81). 수동 기록과 고치기 모두 그런 입력을
// 틈만큼 뒤로 밀어 저장하지 않고 거절한다. 고치지 않은 칸은 원래 초 단위 값을 그대로 보낸다.
describe('time entries at a wall time that does not exist', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
    me.timeZone = ZONE;
    vi.spyOn(api, 'timeEntries').mockResolvedValue([]);
    vi.spyOn(api, 'timeSummary').mockResolvedValue([]);
    vi.spyOn(api, 'currentTimer').mockResolvedValue(null);
    vi.spyOn(api, 'timeEntryRevisions').mockResolvedValue([]);
  });
  afterEach(() => {
    me.timeZone = '';
    vi.restoreAllMocks();
  });

  const submit = async (field: HTMLElement): Promise<void> => {
    const form = field.closest('form');
    if (form === null) throw new Error('form');
    await fireEvent.submit(form);
  };

  it('manual entry: 02:30–03:30 on the gap day is refused instead of becoming a 24-hour entry', async () => {
    const add = vi.spyOn(api, 'addTimeEntry').mockResolvedValue({
      id: 'n1',
      started_at: '2026-03-08T07:30:00Z',
      ended_at: '2026-03-09T07:30:00Z',
      paused_at: null,
      paused_seconds: 0,
      seconds: 86_400,
      kind: 'work',
      memo: '',
      target: null,
      version: 1,
    });
    render(TimeScreen, { props: { workspaces: [] } });
    const day = await screen.findByLabelText(m.time_date());
    await fireEvent.input(day, { target: { value: '2026-03-08' } });
    await fireEvent.input(screen.getByLabelText(m.time_start()), { target: { value: '02:30' } });
    await fireEvent.input(screen.getByLabelText(m.time_end()), { target: { value: '03:30' } });
    await submit(day);
    await waitFor(() => {
      expect(screen.getByRole('alert').textContent).toContain(ZONE);
    });
    expect(add).not.toHaveBeenCalled();

    // 다음 날로 넘어가는 끝이 없는 시각이어도 거절한다(03-07 23:30 → 03-08 02:30).
    await fireEvent.input(day, { target: { value: '2026-03-07' } });
    await fireEvent.input(screen.getByLabelText(m.time_start()), { target: { value: '23:30' } });
    await fireEvent.input(screen.getByLabelText(m.time_end()), { target: { value: '02:30' } });
    await submit(day);
    await waitFor(() => {
      expect(screen.getByRole('alert').textContent).toContain(ZONE);
    });
    expect(add).not.toHaveBeenCalled();

    // 없는 시각을 넣은 칸 옆에 알린다(끝 칸).
    expect(screen.getByLabelText(m.time_end()).getAttribute('aria-invalid')).toBe('true');
    // 있는 시각이면 그대로 더한다(03-07 23:30 → 다음 날 03:30 EDT = 07:30Z).
    await fireEvent.input(screen.getByLabelText(m.time_end()), { target: { value: '03:30' } });
    await submit(day);
    await waitFor(() => {
      expect(add).toHaveBeenCalledTimes(1);
    });
    expect(add.mock.calls[0]?.[1]).toMatchObject({
      started_at: '2026-03-08T04:30:00.000Z',
      ended_at: '2026-03-08T07:30:00.000Z',
    });
    // 틈이 있는 날(03-08) 23:30 → 02:30은 다음 날(03-09) 02:30으로, 그날에는 있는 시각이라 더한다(같은 날의 02:30만 없다).
    await fireEvent.input(day, { target: { value: '2026-03-08' } });
    await fireEvent.input(screen.getByLabelText(m.time_end()), { target: { value: '02:30' } });
    await submit(day);
    await waitFor(() => {
      expect(add).toHaveBeenCalledTimes(2);
    });
    expect(add.mock.calls[1]?.[1]).toMatchObject({
      started_at: '2026-03-09T03:30:00.000Z',
      ended_at: '2026-03-09T06:30:00.000Z',
    });
  });

  it('manual entry: an end earlier on the clock than the start goes to the next day even if that hour is missing today', async () => {
    // 다음 날로 넘길지는 입력한 벽시계 시각으로 정한다. 틈만큼 밀린 값으로 비교하면 03:00 → 02:30은 같은 날의 없는
    // 02:30으로 거절되고 03:30 → 02:30은 더해져, 같은 꼴의 입력이 시작 시각에 따라 갈린다.
    const add = vi.spyOn(api, 'addTimeEntry').mockResolvedValue({
      id: 'n1',
      started_at: '2026-03-08T07:00:00Z',
      ended_at: '2026-03-09T06:30:00Z',
      paused_at: null,
      paused_seconds: 0,
      seconds: 84_600,
      kind: 'work',
      memo: '',
      target: null,
      version: 1,
    });
    render(TimeScreen, { props: { workspaces: [] } });
    const day = await screen.findByLabelText(m.time_date());
    await fireEvent.input(day, { target: { value: '2026-03-08' } });
    await fireEvent.input(screen.getByLabelText(m.time_start()), { target: { value: '03:00' } });
    await fireEvent.input(screen.getByLabelText(m.time_end()), { target: { value: '02:30' } });
    await submit(day);
    await waitFor(() => {
      expect(add).toHaveBeenCalledTimes(1);
    });
    expect(add.mock.calls[0]?.[1]).toMatchObject({
      started_at: '2026-03-08T07:00:00.000Z',
      ended_at: '2026-03-09T06:30:00.000Z',
    });

    // 대조: 03:30 → 02:30도 다음 날 02:30까지.
    await fireEvent.input(screen.getByLabelText(m.time_start()), { target: { value: '03:30' } });
    await submit(day);
    await waitFor(() => {
      expect(add).toHaveBeenCalledTimes(2);
    });
    expect(add.mock.calls[1]?.[1]).toMatchObject({
      started_at: '2026-03-08T07:30:00.000Z',
      ended_at: '2026-03-09T06:30:00.000Z',
    });

    // 끝이 시작보다 늦으면 같은 날이라 없는 02:30은 계속 거절한다.
    await fireEvent.input(screen.getByLabelText(m.time_start()), { target: { value: '01:30' } });
    await submit(day);
    await waitFor(() => {
      expect(screen.getByLabelText(m.time_end()).getAttribute('aria-invalid')).toBe('true');
    });
    expect(add).toHaveBeenCalledTimes(2);
  });

  it('manual entry: changing only the date clears the missing-time error beside the field', async () => {
    // 없는 시각인지는 날짜와 시각을 함께 보고 정하므로, 날짜만 바꿔도 칸 옆 오류를 지운다(다시 저장할 때 다시 본다).
    const add = vi.spyOn(api, 'addTimeEntry').mockRejectedValue(new Error('not expected'));
    render(TimeScreen, { props: { workspaces: [] } });
    const day = await screen.findByLabelText(m.time_date());
    await fireEvent.input(day, { target: { value: '2026-03-08' } });
    await fireEvent.input(screen.getByLabelText(m.time_start()), { target: { value: '02:30' } });
    await fireEvent.input(screen.getByLabelText(m.time_end()), { target: { value: '03:30' } });
    await submit(day);
    await waitFor(() => {
      expect(screen.getByLabelText(m.time_start()).getAttribute('aria-invalid')).toBe('true');
    });
    await fireEvent.input(day, { target: { value: '2026-03-09' } });
    expect(screen.getByLabelText(m.time_start()).getAttribute('aria-invalid')).not.toBe('true');
    expect(screen.queryByText(m.events_missing_time({ zone: ZONE }))).toBeNull();

    // 끝 칸도 같다(03-07 23:30 → 다음 날 02:30은 없고, 날짜를 03-06으로 바꾸면 03-07 02:30이라 있다).
    await fireEvent.input(day, { target: { value: '2026-03-07' } });
    await fireEvent.input(screen.getByLabelText(m.time_start()), { target: { value: '23:30' } });
    await fireEvent.input(screen.getByLabelText(m.time_end()), { target: { value: '02:30' } });
    await submit(day);
    await waitFor(() => {
      expect(screen.getByLabelText(m.time_end()).getAttribute('aria-invalid')).toBe('true');
    });
    await fireEvent.input(day, { target: { value: '2026-03-06' } });
    expect(screen.getByLabelText(m.time_end()).getAttribute('aria-invalid')).not.toBe('true');
    expect(screen.queryByText(m.events_missing_time({ zone: ZONE }))).toBeNull();
    expect(add).not.toHaveBeenCalled();
  });

  it('editing: a changed start at a missing time is refused, and untouched fields keep their stored values', async () => {
    const entry: TimeEntryView = {
      id: 'e1',
      started_at: '2026-03-07T14:00:37Z',
      ended_at: '2026-03-07T15:00:00Z',
      paused_at: null,
      paused_seconds: 0,
      seconds: 3563,
      kind: 'work',
      memo: '',
      target: null,
      version: 1,
    };
    const update = vi
      .spyOn(api, 'updateTimeEntry')
      .mockResolvedValue({ kind: 'updated', entry: { ...entry, version: 2 } });
    render(TimeEntryDialog, {
      props: { entry, zone: ZONE, onclose: () => undefined, onchanged: () => undefined },
    });
    const start = await screen.findByLabelText(m.time_start());
    expect(
      start.getAttribute('value') ?? (start instanceof HTMLInputElement ? start.value : ''),
    ).toBe('2026-03-07T09:00');
    await fireEvent.input(start, { target: { value: '2026-03-08T02:30' } });
    await submit(start);
    await waitFor(() => {
      expect(screen.getByRole('alert').textContent).toContain(ZONE);
    });
    expect(update).not.toHaveBeenCalled();

    // 시작을 연 때의 값으로 되돌리고 메모만 고치면 원래 RFC3339 값(초 포함)을 그대로 보낸다.
    await fireEvent.input(start, { target: { value: '2026-03-07T09:00' } });
    await fireEvent.input(screen.getByLabelText(m.time_memo()), { target: { value: '메모' } });
    await submit(start);
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(1);
    });
    expect(update.mock.calls[0]?.[2]).toMatchObject({
      started_at: '2026-03-07T14:00:37Z',
      ended_at: '2026-03-07T15:00:00Z',
      memo: '메모',
    });
  });

  const finished = (id: string): TimeEntryView => ({
    id,
    started_at: '2026-03-07T14:00:00Z',
    ended_at: '2026-03-07T15:00:00Z',
    paused_at: null,
    paused_seconds: 0,
    seconds: 3600,
    kind: 'work',
    memo: '',
    target: null,
    version: 1,
  });

  it('editing: the missing-time error does not follow the dialog to the next entry it opens', async () => {
    // 다이얼로그는 화면에 늘 마운트돼 있어(닫으면 entry만 비운다) 칸 옆 오류가 다음에 연 기록에 남지 않아야 한다.
    const props = { zone: ZONE, onclose: () => undefined, onchanged: () => undefined };
    const view = render(TimeEntryDialog, { props: { entry: finished('a'), ...props } });
    const start = await screen.findByLabelText(m.time_start());
    await fireEvent.input(start, { target: { value: '2026-03-08T02:30' } });
    await submit(start);
    await waitFor(() => {
      expect(screen.getByLabelText(m.time_start()).getAttribute('aria-invalid')).toBe('true');
    });

    await view.rerender({ entry: undefined, ...props });
    await view.rerender({ entry: finished('b'), ...props });
    const reopened = await screen.findByLabelText(m.time_start());
    expect(reopened.getAttribute('aria-invalid')).not.toBe('true');
    expect(screen.queryByText(m.events_missing_time({ zone: ZONE }))).toBeNull();
  });

  it('editing: a general error already fixed does not stay beside a new missing-time error', async () => {
    const update = vi
      .spyOn(api, 'updateTimeEntry')
      .mockResolvedValue({ kind: 'updated', entry: finished('a') });
    render(TimeEntryDialog, {
      props: {
        entry: finished('a'),
        zone: ZONE,
        onclose: () => undefined,
        onchanged: () => undefined,
      },
    });
    const paused = await screen.findByLabelText(m.time_paused_minutes());
    await fireEvent.input(paused, { target: { value: 'abc' } });
    await submit(paused);
    await waitFor(() => {
      expect(screen.getByText(m.time_paused_invalid())).toBeTruthy();
    });

    // 멈춘 시간을 고치고 시작을 없는 시각으로 두면 시작 칸 오류만 남는다.
    await fireEvent.input(paused, { target: { value: '0' } });
    await fireEvent.input(screen.getByLabelText(m.time_start()), {
      target: { value: '2026-03-08T02:30' },
    });
    await submit(paused);
    await waitFor(() => {
      expect(screen.getByLabelText(m.time_start()).getAttribute('aria-invalid')).toBe('true');
    });
    expect(screen.queryByText(m.time_paused_invalid())).toBeNull();
    expect(update).not.toHaveBeenCalled();
  });
});
