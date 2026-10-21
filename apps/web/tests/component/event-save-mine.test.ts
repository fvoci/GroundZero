import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { EventInput, EventView } from '$lib/api/client';
import { api, NetworkError } from '$lib/api/client';
import EventDialog from '$lib/features/events/EventDialog.svelte';
import { describeWhen } from '$lib/features/events/labels';
import { localTimeZone } from '$lib/features/events/time';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';

// D-94: 일정 충돌 알림의 "내 입력으로 다시 저장"은 충돌 때 잡아 둔 입력 전체를 보내 다른 사람이 바꾼 칸(내가 손대지
// 않은 제목·시각)을 되돌렸고, 알림이 뜬 뒤 입력한 글은 빠뜨렸다. 이제 지금 입력에서 내가 고친 칸만 서버 최신 일정에
// 얹어 최신 판 번호로 보내고, 같은 칸을 양쪽이 고쳤으면 양쪽 값을 보여 덮어쓸지 묻는다. 실패해도 입력은 남는다.
afterEach(() => {
  vi.restoreAllMocks();
});
beforeEach(async () => {
  await setLocale('ko', { reload: false });
});

type UpdateResult =
  { kind: 'updated'; event: EventView } | { kind: 'conflict'; current: EventView };

/** 서울 10:00–11:00 회의(5판). */
function ev(fields: Partial<EventInput> = {}, version = 5): EventView {
  return {
    id: 'A',
    item_id: 'i1',
    workspace_id: 'ws-1',
    kind: 'meeting',
    title: '회의',
    description: '',
    all_day: false,
    start_date: null,
    end_date: null,
    starts_at: '2026-10-20T01:00:00Z',
    ends_at: '2026-10-20T02:00:00Z',
    timezone: 'Asia/Seoul',
    recurrence: null,
    excluded_dates: [],
    version,
    access: 'write',
    ...fields,
  };
}

/** 보내는 입력(입력 칸만). */
function sent(fields: Partial<EventInput> = {}): EventInput {
  const e = ev(fields);
  return {
    kind: e.kind,
    title: e.title,
    description: e.description,
    all_day: e.all_day,
    start_date: e.start_date,
    end_date: e.end_date,
    starts_at: e.starts_at,
    ends_at: e.ends_at,
    timezone: e.timezone,
  };
}

/** 다른 사람이 제목과 시각(서울 14:00–15:00)을 바꾼 6판. */
const theirs = ev(
  { title: '새 제목', starts_at: '2026-10-20T05:00:00Z', ends_at: '2026-10-20T06:00:00Z' },
  6,
);

function field(label: string): HTMLInputElement | HTMLTextAreaElement {
  const el = screen.getByLabelText(label);
  if (!(el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement))
    throw new Error(`no field ${label}`);
  return el;
}

function eventForm(): HTMLFormElement {
  const form = field(m.events_title()).closest('form');
  if (form === null) throw new Error('no event form');
  return form;
}

function stub(...results: (UpdateResult | Error)[]) {
  vi.spyOn(api, 'event').mockResolvedValue(ev());
  const update = vi.spyOn(api, 'updateEvent');
  for (const r of results) {
    if (r instanceof Error) update.mockRejectedValueOnce(r);
    else update.mockResolvedValueOnce(r);
  }
  // 그 밖의 저장은 보낸 내용대로 저장된다.
  update.mockImplementation((_id: string, base: number, input: EventInput) =>
    Promise.resolve({ kind: 'updated', event: ev(input, base + 1) }),
  );
  return update;
}

async function open(): Promise<void> {
  render(EventDialog, {
    props: { eventId: 'A', onclose: () => undefined, onchanged: () => undefined },
  });
  await screen.findByDisplayValue('회의');
}

/** 설명만 고쳐 저장하면 다른 사람의 변경과 충돌한다. */
async function describeAndConflict(text: string): Promise<void> {
  await fireEvent.input(field(m.events_description()), { target: { value: text } });
  await fireEvent.submit(eventForm());
  await screen.findByRole('button', { name: m.events_save_mine() });
}

async function saveMine(): Promise<void> {
  await fireEvent.click(screen.getByRole('button', { name: m.events_save_mine() }));
}

describe('saving my input after an event conflict (D-94)', () => {
  it('sends the latest title and time with only my description, at the latest version', async () => {
    const update = stub({ kind: 'conflict', current: theirs });
    await open();
    await describeAndConflict('내 설명');

    await saveMine();
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(2);
    });
    expect(update.mock.calls[1]).toEqual([
      'A',
      6,
      sent({
        title: '새 제목',
        starts_at: '2026-10-20T05:00:00Z',
        ends_at: '2026-10-20T06:00:00Z',
        description: '내 설명',
      }),
    ]);
    expect(screen.queryByRole('alertdialog')).toBeNull();
    // 저장된 일정으로 입력을 다시 만든다.
    expect(await screen.findByDisplayValue('새 제목')).toBeTruthy();
  });

  it('includes what I typed after the conflict notice appeared', async () => {
    const update = stub({ kind: 'conflict', current: theirs });
    await open();
    await describeAndConflict('내 설명');
    await fireEvent.input(field(m.events_description()), {
      target: { value: '내 설명, 알림 뒤에 더 씀' },
    });

    await saveMine();
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(2);
    });
    expect(update.mock.calls[1]?.[2]).toMatchObject({
      title: '새 제목',
      description: '내 설명, 알림 뒤에 더 씀',
    });
  });

  it('asks before overwriting a time both changed: cancel sends nothing, confirm sends my time', async () => {
    const update = stub({ kind: 'conflict', current: theirs });
    await open();
    // 나는 서울 15:00–16:00으로 옮겼고, 그사이 다른 사람은 14:00–15:00으로 옮겼다.
    await fireEvent.input(field(m.events_starts()), { target: { value: '2026-10-20T15:00' } });
    await fireEvent.input(field(m.events_ends()), { target: { value: '2026-10-20T16:00' } });
    await fireEvent.submit(eventForm());
    await screen.findByRole('button', { name: m.events_save_mine() });

    await saveMine();
    const box = await screen.findByRole('alertdialog', { name: m.events_clash_question() });
    await waitFor(() => {
      expect(document.activeElement).toBe(box);
    });
    const zone = localTimeZone();
    const mineWhen = { starts_at: '2026-10-20T06:00:00.000Z', ends_at: '2026-10-20T07:00:00.000Z' };
    // 겹친 칸은 일시 하나(제목은 서버만 바꿨다). 서버 값과 내 값을 함께 보인다.
    expect(
      within(box)
        .getAllByRole('term')
        .map((t) => t.textContent),
    ).toEqual([m.events_clash_when()]);
    expect(box.textContent).toContain(m.events_clash_server({ value: describeWhen(theirs, zone) }));
    expect(box.textContent).toContain(
      m.events_clash_mine({ value: describeWhen({ ...theirs, ...mineWhen }, zone) }),
    );
    expect(update).toHaveBeenCalledTimes(1);

    // 취소: 아무것도 보내지 않고 알림과 입력을 두며 초점은 다시 저장 단추로.
    await fireEvent.click(within(box).getByRole('button', { name: m.common_cancel() }));
    expect(screen.queryByRole('alertdialog')).toBeNull();
    const again = screen.getByRole('button', { name: m.events_save_mine() });
    await waitFor(() => {
      expect(document.activeElement).toBe(again);
    });
    expect(update).toHaveBeenCalledTimes(1);
    expect(field(m.events_starts()).value).toBe('2026-10-20T15:00');

    // 덮어쓰기: 내 일시와 나머지는 서버 최신 값을 최신 판으로.
    await saveMine();
    const confirm = await screen.findByRole('alertdialog', { name: m.events_clash_question() });
    await fireEvent.click(
      within(confirm).getByRole('button', { name: m.events_clash_overwrite() }),
    );
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(2);
    });
    expect(update.mock.calls[1]).toEqual(['A', 6, sent({ title: '새 제목', ...mineWhen })]);
    await waitFor(() => {
      expect(screen.queryByRole('alertdialog')).toBeNull();
    });
  });

  it('shows the newer version when it changed again and keeps my input for the next try', async () => {
    const newer = ev({ title: '더 새 제목' }, 7);
    const update = stub(
      { kind: 'conflict', current: ev({ title: '새 제목' }, 6) },
      { kind: 'conflict', current: newer },
    );
    await open();
    await describeAndConflict('내 설명');

    await saveMine();
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(2);
    });
    expect(update.mock.calls[1]).toEqual([
      'A',
      6,
      sent({ title: '새 제목', description: '내 설명' }),
    ]);
    const zone = localTimeZone();
    expect(
      await screen.findByText(
        m.events_conflict({ title: '더 새 제목', when: describeWhen(newer, zone) }),
      ),
    ).toBeTruthy();
    expect(field(m.events_description()).value).toBe('내 설명');
    expect(field(m.events_title()).value).toBe('회의');

    // 다음 다시 저장은 가장 새 일정(7판)에 합친다. 내가 고친 것은 여전히 설명뿐이다.
    await saveMine();
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(3);
    });
    expect(update.mock.calls[2]).toEqual([
      'A',
      7,
      sent({ title: '더 새 제목', description: '내 설명' }),
    ]);
  });

  it('keeps my input and the conflict notice when the save fails', async () => {
    const update = stub({ kind: 'conflict', current: theirs }, new NetworkError('offline'));
    await open();
    await describeAndConflict('내 설명');

    await saveMine();
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(2);
    });
    expect(
      await screen.findByText(`${m.events_save_failed()} ${m.common_error_network()}`),
    ).toBeTruthy();
    expect(field(m.events_description()).value).toBe('내 설명');
    expect(screen.getByRole('button', { name: m.events_save_mine() })).toBeTruthy();

    // 다시 누르면 같은 내용을 다시 보낸다.
    await saveMine();
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(3);
    });
    expect(update.mock.calls[2]).toEqual(update.mock.calls[1]);
  });
});
