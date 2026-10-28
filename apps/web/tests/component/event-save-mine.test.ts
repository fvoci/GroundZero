import { type EventInput, type EventView } from '$lib/features/events/model';
import { RuleConflict, eventsApi } from '$lib/features/events/api';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { NetworkError } from '$lib/api/client';
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

/** 내가 옮긴 시각(서울 15:00–16:00)을 입력 양식이 보내는 표기로. */
const mineWhen = { starts_at: '2026-10-20T06:00:00.000Z', ends_at: '2026-10-20T07:00:00.000Z' };

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
  vi.spyOn(eventsApi, 'event').mockResolvedValue(ev());
  const update = vi.spyOn(eventsApi, 'updateEvent');
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

async function open(onclose: () => void = () => undefined): Promise<void> {
  render(EventDialog, {
    props: { eventId: 'A', onclose, onchanged: () => undefined },
  });
  await screen.findByDisplayValue('회의');
}

/** 설명만 고쳐 저장하면 다른 사람의 변경과 충돌한다. */
async function describeAndConflict(text: string): Promise<void> {
  await fireEvent.input(field(m.events_description()), { target: { value: text } });
  await fireEvent.submit(eventForm());
  await screen.findByRole('button', { name: m.events_save_mine() });
}

/** 서울 15:00–16:00으로 옮겨 저장하면 다른 사람의 변경(14:00–15:00)과 충돌한다. */
async function moveAndConflict(): Promise<void> {
  await fireEvent.input(field(m.events_starts()), { target: { value: '2026-10-20T15:00' } });
  await fireEvent.input(field(m.events_ends()), { target: { value: '2026-10-20T16:00' } });
  await fireEvent.submit(eventForm());
  await screen.findByRole('button', { name: m.events_save_mine() });
}

function saveMineButton(): HTMLElement {
  return screen.getByRole('button', { name: m.events_save_mine() });
}

async function saveMine(): Promise<void> {
  await fireEvent.click(saveMineButton());
}

async function overwrite(): Promise<void> {
  const box = await screen.findByRole('alertdialog', { name: m.events_clash_question() });
  await fireEvent.click(within(box).getByRole('button', { name: m.events_clash_overwrite() }));
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
    await moveAndConflict();

    await saveMine();
    const box = await screen.findByRole('alertdialog', { name: m.events_clash_question() });
    await waitFor(() => {
      expect(document.activeElement).toBe(box);
    });
    const zone = localTimeZone();
    // 겹친 칸은 일시 하나(제목은 서버만 바꿨다). 서버 값과 내 값을 함께 보인다.
    expect(
      within(box)
        .getAllByRole('term')
        .map((t) => t.textContent),
    ).toEqual([m.events_clash_when()]);
    // 일시 묶음의 이름은 종일 여부까지 가리킨다(종일 전환도 이 묶음으로 덮어쓴다).
    expect(m.events_clash_when()).toContain(m.events_all_day());
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

  // 리뷰: 덮어쓸지 묻는 칸에서 Esc를 누르면 대화상자 전체가 닫혀 저장 안 한 입력을 잃었다.
  it('Escape on the overwrite question cancels only the question and keeps the dialog', async () => {
    const update = stub({ kind: 'conflict', current: theirs });
    const onclose = vi.fn();
    await open(onclose);
    await moveAndConflict();
    await saveMine();
    const box = await screen.findByRole('alertdialog', { name: m.events_clash_question() });

    await fireEvent.keyDown(box, { key: 'Escape' });
    expect(onclose).not.toHaveBeenCalled();
    expect(screen.queryByRole('alertdialog')).toBeNull();
    await waitFor(() => {
      expect(document.activeElement).toBe(saveMineButton());
    });
    expect(update).toHaveBeenCalledTimes(1);
    expect(field(m.events_starts()).value).toBe('2026-10-20T15:00');
  });

  // 리뷰: 덮어쓰기가 다시 409를 받거나 저장되면 누른 단추가 사라져 초점이 대화상자 바탕으로 떨어졌다.
  it('focuses "save mine" after a repeated 409 and the heading after the save', async () => {
    // 그사이 또 제목을 바꿨다(일시는 그대로라 다시 겹친다).
    const newer = { ...theirs, title: '더 새 제목', version: 7 };
    const update = stub(
      { kind: 'conflict', current: theirs },
      { kind: 'conflict', current: newer },
    );
    await open();
    await moveAndConflict();
    await saveMine();
    await overwrite();
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(2);
    });
    await waitFor(() => {
      expect(document.activeElement).toBe(saveMineButton());
    });

    await saveMine();
    await overwrite();
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(3);
    });
    expect(update.mock.calls[2]?.[1]).toBe(7);
    await waitFor(() => {
      expect(document.activeElement).toBe(screen.getByRole('heading', { name: '더 새 제목' }));
    });
  });

  function ruleForm(): HTMLFormElement {
    const rule = document.querySelector('[aria-labelledby="event-recurrence-heading"] form');
    if (!(rule instanceof HTMLFormElement)) throw new Error('no recurrence form');
    return rule;
  }

  /**
   * 알림이 떠 있는 동안 반복 규칙을 저장하면 서버는 더 새 판(7판, 그사이 제목도 바뀜)을 돌려준다. `fields`는 그 판에서
   * 더 바뀐 칸.
   */
  async function saveRuleDuringNotice(fields: Partial<EventInput> = {}): Promise<EventView> {
    const afterRule = { ...theirs, title: '규칙 뒤 제목', ...fields, version: 7 };
    const setRule = vi.spyOn(eventsApi, 'setEventRecurrence').mockResolvedValue(afterRule);
    await fireEvent.submit(ruleForm());
    await waitFor(() => {
      expect(setRule).toHaveBeenCalledTimes(1);
    });
    return afterRule;
  }

  // 리뷰: 알림 뒤 반복 규칙을 저장하면 `event`가 알림의 일정보다 새 판인데, 다시 저장은 옛 판에 합쳐 반드시 다시 409였다.
  it('merges onto the newer version a rule save returned while the notice was shown', async () => {
    const update = stub({ kind: 'conflict', current: theirs });
    await open();
    await describeAndConflict('내 설명');
    await saveRuleDuringNotice();

    await saveMine();
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(2);
    });
    expect(update.mock.calls[1]).toEqual([
      'A',
      7,
      sent({
        title: '규칙 뒤 제목',
        starts_at: '2026-10-20T05:00:00Z',
        ends_at: '2026-10-20T06:00:00Z',
        description: '내 설명',
      }),
    ]);
  });

  it('shows the newer version a rule save returned in the notice', async () => {
    stub({ kind: 'conflict', current: theirs });
    await open();
    await describeAndConflict('내 설명');
    const afterRule = await saveRuleDuringNotice();

    const zone = localTimeZone();
    expect(
      await screen.findByText(
        m.events_conflict({ title: '규칙 뒤 제목', when: describeWhen(afterRule, zone) }),
      ),
    ).toBeTruthy();
  });

  // 다시 저장은 requestSubmit() 동안만 두는 표시로 보통 저장과 가린다. 확인에 걸려 제출되지 않았을 때 그 표시가 남아
  // 다음 보통 저장을 합치기로 바꾸지 않는지 지킨다.
  it('a save-mine blocked by validation leaves the next plain save a plain save', async () => {
    const update = stub({ kind: 'conflict', current: theirs });
    await open();
    await describeAndConflict('내 설명');

    // 필수 칸(제목)을 비우면 양식이 제출되지 않는다.
    await fireEvent.input(field(m.events_title()), { target: { value: '' } });
    await saveMine();
    // 양식 자신의 확인(모르는 시간대)에 걸려도 보내지 않는다.
    await fireEvent.input(field(m.events_title()), { target: { value: '회의' } });
    await fireEvent.input(field(m.events_time_zone()), { target: { value: 'Nowhere/Zone' } });
    await saveMine();
    expect(await screen.findByText(m.events_zone_invalid())).toBeTruthy();
    expect(update).toHaveBeenCalledTimes(1);

    // 고친 뒤 양식의 저장 단추는 보통 저장이다: 입력을 연 판(5)으로 입력 그대로 보낸다(합치기였다면 6판).
    await fireEvent.input(field(m.events_time_zone()), { target: { value: 'Asia/Seoul' } });
    await fireEvent.submit(eventForm());
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(2);
    });
    expect(update.mock.calls[1]).toEqual(['A', 5, sent({ description: '내 설명' })]);
  });

  // 반박 검증: 물은 뒤 반복 규칙 저장으로 더 새 판이 생겨도 겹친 일시는 그대로다. 덮어쓰기는 다시 묻지 않고 그 판에 합친다.
  it('overwrites onto the newer version a rule save returned after the question, without asking again', async () => {
    const update = stub({ kind: 'conflict', current: theirs });
    await open();
    await moveAndConflict();
    await saveMine();
    await screen.findByRole('alertdialog', { name: m.events_clash_question() });
    await saveRuleDuringNotice();

    await overwrite();
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(2);
    });
    expect(update.mock.calls[1]).toEqual(['A', 7, sent({ title: '규칙 뒤 제목', ...mineWhen })]);
    await waitFor(() => {
      expect(screen.queryByRole('alertdialog')).toBeNull();
    });
  });

  it('asks again when the newer version a rule save returned also moved the time', async () => {
    const update = stub({ kind: 'conflict', current: theirs });
    await open();
    await moveAndConflict();
    await saveMine();
    await screen.findByRole('alertdialog', { name: m.events_clash_question() });
    // 규칙 저장이 돌려준 판에서는 다른 사람이 서울 13:00–14:00으로 또 옮겼다.
    const afterRule = await saveRuleDuringNotice({
      starts_at: '2026-10-20T04:00:00Z',
      ends_at: '2026-10-20T05:00:00Z',
    });

    await overwrite();
    const zone = localTimeZone();
    await waitFor(() => {
      expect(screen.getByRole('alertdialog').textContent).toContain(
        m.events_clash_server({ value: describeWhen(afterRule, zone) }),
      );
    });
    expect(update).toHaveBeenCalledTimes(1);

    await overwrite();
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(2);
    });
    expect(update.mock.calls[1]).toEqual(['A', 7, sent({ title: '규칙 뒤 제목', ...mineWhen })]);
  });

  it('asks again when I change the clashing time after the question', async () => {
    const update = stub({ kind: 'conflict', current: theirs });
    await open();
    await moveAndConflict();
    await saveMine();
    await screen.findByRole('alertdialog', { name: m.events_clash_question() });
    // 물은 뒤 서울 16:30–17:30으로 다시 옮겼다. 확인받은 내 값과 다르니 덮어쓰기는 새 값으로 다시 묻는다.
    await fireEvent.input(field(m.events_starts()), { target: { value: '2026-10-20T16:30' } });
    await fireEvent.input(field(m.events_ends()), { target: { value: '2026-10-20T17:30' } });
    const again = { starts_at: '2026-10-20T07:30:00.000Z', ends_at: '2026-10-20T08:30:00.000Z' };

    await overwrite();
    const zone = localTimeZone();
    await waitFor(() => {
      expect(screen.getByRole('alertdialog').textContent).toContain(
        m.events_clash_mine({ value: describeWhen({ ...theirs, ...again }, zone) }),
      );
    });
    expect(update).toHaveBeenCalledTimes(1);

    await overwrite();
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(2);
    });
    expect(update.mock.calls[1]).toEqual(['A', 6, sent({ title: '새 제목', ...again })]);
  });

  // 반박 검증: "서버 내용 불러오기"도 알림 뒤 반복 규칙 저장이 돌려준 더 새 판을 불러온다(옛 판이면 방금 저장한 규칙을
  // 되돌리고, 다음 저장이 반드시 다시 409).
  it('loads the newer version a rule save returned while the notice was shown', async () => {
    const update = stub({ kind: 'conflict', current: theirs });
    await open();
    await describeAndConflict('내 설명');
    await saveRuleDuringNotice();

    await fireEvent.click(screen.getByRole('button', { name: m.events_load_server() }));
    expect(await screen.findByDisplayValue('규칙 뒤 제목')).toBeTruthy();
    expect(field(m.events_description()).value).toBe('');
    expect(screen.queryByRole('button', { name: m.events_save_mine() })).toBeNull();

    await fireEvent.submit(eventForm());
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(2);
    });
    expect(update.mock.calls[1]).toEqual([
      'A',
      7,
      sent({
        title: '규칙 뒤 제목',
        starts_at: '2026-10-20T05:00:00Z',
        ends_at: '2026-10-20T06:00:00Z',
      }),
    ]);
  });

  it('merges onto the event a rule conflict returned while the notice was shown', async () => {
    const update = stub({ kind: 'conflict', current: theirs });
    await open();
    await describeAndConflict('내 설명');
    // 반복 규칙 저장이 그사이 바뀐 규칙과 부딪쳐 서버가 지금 일정(7판)을 함께 돌려준다.
    const ruled: EventView = {
      ...theirs,
      title: '규칙 뒤 제목',
      recurrence: { freq: 'weekly', interval: 1, weekdays: [2], until: null, count: null },
      version: 7,
    };
    vi.spyOn(eventsApi, 'setEventRecurrence').mockRejectedValue(new RuleConflict(ruled));
    await fireEvent.submit(ruleForm());

    const zone = localTimeZone();
    expect(
      await screen.findByText(
        m.events_conflict({ title: '규칙 뒤 제목', when: describeWhen(ruled, zone) }),
      ),
    ).toBeTruthy();
    expect(field(m.events_description()).value).toBe('내 설명');

    await saveMine();
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(2);
    });
    expect(update.mock.calls[1]).toEqual([
      'A',
      7,
      sent({
        title: '규칙 뒤 제목',
        starts_at: '2026-10-20T05:00:00Z',
        ends_at: '2026-10-20T06:00:00Z',
        description: '내 설명',
      }),
    ]);
  });

  // 반박 검증 nit: 공백만 남긴 내 설명은 서버가 빈 설명으로 저장한다. 물을 때도 비어 있다고 보인다.
  it('shows a description of only spaces as empty in the question', async () => {
    stub({ kind: 'conflict', current: ev({ description: '남의 설명' }, 6) });
    vi.spyOn(eventsApi, 'event').mockResolvedValue(ev({ description: '원래 설명' }));
    await open();
    await describeAndConflict('  \n ');

    await saveMine();
    const box = await screen.findByRole('alertdialog', { name: m.events_clash_question() });
    expect(box.textContent).toContain(m.events_clash_server({ value: '남의 설명' }));
    expect(box.textContent).toContain(m.events_clash_mine({ value: m.events_clash_empty() }));
  });

  it('reopening drops the old notice and question', async () => {
    const update = stub({ kind: 'conflict', current: theirs });
    const props = (eventId: string | undefined) => ({
      eventId,
      onclose: () => undefined,
      onchanged: () => undefined,
    });
    const { rerender } = render(EventDialog, { props: props('A') });
    await screen.findByDisplayValue('회의');
    await moveAndConflict();
    await saveMine();
    await screen.findByRole('alertdialog', { name: m.events_clash_question() });

    await rerender(props(undefined));
    await fireEvent.click(await screen.findByRole('button', { name: m.common_discard_confirm() }));
    await rerender(props('A'));
    await screen.findByDisplayValue('회의');
    expect(screen.queryByRole('alertdialog')).toBeNull();
    expect(screen.queryByRole('button', { name: m.events_save_mine() })).toBeNull();
    expect(field(m.events_starts()).value).toBe('2026-10-20T10:00');

    await fireEvent.submit(eventForm());
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(2);
    });
    expect(update.mock.calls[1]).toEqual(['A', 5, sent()]);
  });
});
import './fixtures/event-user';
