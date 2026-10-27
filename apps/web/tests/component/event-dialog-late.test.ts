import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { EventView, RecurrenceRule } from '$lib/api/client';
import { api } from '$lib/api/client';
import EventDialog from '$lib/features/events/EventDialog.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';

afterEach(() => {
  vi.restoreAllMocks();
});

type UpdateResult =
  { kind: 'updated'; event: EventView } | { kind: 'conflict'; current: EventView };

function ev(
  id: string,
  version: number,
  title: string,
  recurrence: RecurrenceRule | null = null,
): EventView {
  return {
    id,
    item_id: 'i1',
    workspace_id: 'ws-1',
    kind: 'meeting',
    title,
    description: '',
    all_day: true,
    start_date: '2026-10-20',
    end_date: '2026-10-20',
    starts_at: null,
    ends_at: null,
    timezone: 'Asia/Seoul',
    recurrence,
    excluded_dates: [],
    version,
    access: 'write',
  };
}

function deferred<T>(): { promise: Promise<T>; resolve: (value: T) => void } {
  let resolve: (value: T) => void = () => undefined;
  const promise = new Promise<T>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}

function titleInput(): HTMLInputElement {
  const input = screen.getByLabelText(m.events_title());
  if (!(input instanceof HTMLInputElement)) throw new Error('no title input');
  return input;
}

function dialogTitle(): string | null {
  return document.querySelector('[data-slot="dialog-title"]')?.textContent ?? null;
}

const flush = (): Promise<void> => new Promise((r) => setTimeout(r, 0));

// 감사 W24: 일정 대화상자는 보낸 요청의 결과를 기다린 뒤 지금 열린 일정이 같은지 보지 않았고, 보내는 중에도 닫을 수
// 있었다. 느린 연결에서 A를 저장하고 닫은 뒤 B를 열면 A의 늦은 결과가 B의 창에 들어갔다(제목·입력이 A로 바뀌고 다음
// 저장이 A를 고침). A의 늦은 지우기·회차 빼기는 B의 창을 닫았다.
describe('event dialog late results', () => {
  function stubEvents(): void {
    vi.spyOn(api, 'event').mockImplementation((id: string) =>
      Promise.resolve(id === 'A' ? ev('A', 1, 'A 일정') : ev('B', 1, 'B 일정')),
    );
  }

  it('does not put a late save of the previous event into the one open now', async () => {
    await setLocale('ko', { reload: false });
    stubEvents();
    const late = deferred<UpdateResult>();
    const update = vi
      .spyOn(api, 'updateEvent')
      .mockImplementation((id: string, _base: number, input) =>
        id === 'A'
          ? late.promise
          : Promise.resolve({ kind: 'updated', event: ev(id, 2, input.title) }),
      );
    const onclose = vi.fn();
    const props = (eventId: string | undefined) => ({
      eventId,
      onclose,
      onchanged: () => undefined,
    });
    const view = render(EventDialog, { props: props('A') });
    await screen.findByDisplayValue('A 일정');
    await fireEvent.input(titleInput(), { target: { value: 'A 새 제목' } });
    await fireEvent.submit(titleInput().closest('form') ?? titleInput());
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(1);
    });

    // R5 최종 A FA-8(1): 같은 창에서 저장하는 동안 B로 바꾸려 하면 저장 중 가드가 막아 A가 남는다. 그래서 A의 늦은
    // 결과는 A의 창에만 들어간다(새 창을 그려 보던 옛 시험은 보호를 지워도 통과했다).
    await view.rerender(props('B'));
    await flush();
    expect(dialogTitle()).toBe('A 일정');
    // 저장 중이라 묻지도 않는다(고친 입력의 버릴지 확인이 아니라 저장 중 잠금이 막는다).
    expect(screen.queryByRole('alertdialog')).toBeNull();
    expect(titleInput().value).toBe('A 새 제목');

    late.resolve({ kind: 'updated', event: ev('A', 2, 'A 새 제목') });
    await waitFor(() => {
      expect(dialogTitle()).toBe('A 새 제목');
    });
    expect(api.event).not.toHaveBeenCalledWith('B');

    await fireEvent.submit(titleInput().closest('form') ?? titleInput());
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(2);
    });
    expect(update.mock.calls[1]?.[0]).toBe('A');
    expect(update.mock.calls[1]?.[1]).toBe(2);
  });

  it('does not close the event open now after a late delete of the previous one', async () => {
    await setLocale('ko', { reload: false });
    stubEvents();
    const late = deferred<undefined>();
    vi.spyOn(api, 'deleteEvent').mockReturnValue(late.promise);
    const onclose = vi.fn();
    const onchanged = vi.fn();
    const props = (eventId: string | undefined) => ({ eventId, onclose, onchanged });
    const { unmount } = render(EventDialog, { props: props('A') });
    await screen.findByDisplayValue('A 일정');
    await fireEvent.click(screen.getByRole('button', { name: m.events_delete_event() }));
    await fireEvent.click(screen.getByRole('button', { name: m.events_delete() }));

    unmount();
    render(EventDialog, { props: props('B') });
    await screen.findByDisplayValue('B 일정');

    late.resolve(undefined);
    await flush();
    expect(onclose).not.toHaveBeenCalled();
    expect(dialogTitle()).toBe('B 일정');
    // A는 실제로 지워졌으니 부르는 쪽 목록은 다시 읽는다.
    expect(onchanged).toHaveBeenCalled();
  });

  it('cannot be closed with Escape while a save is in flight', async () => {
    await setLocale('ko', { reload: false });
    stubEvents();
    const late = deferred<UpdateResult>();
    const update = vi.spyOn(api, 'updateEvent').mockReturnValue(late.promise);
    const onclose = vi.fn();
    render(EventDialog, { props: { eventId: 'A', onclose, onchanged: () => undefined } });
    await screen.findByDisplayValue('A 일정');
    await fireEvent.submit(titleInput().closest('form') ?? titleInput());
    await waitFor(() => {
      expect(update).toHaveBeenCalled();
    });

    await fireEvent.keyDown(document.activeElement ?? document.body, { key: 'Escape' });
    expect(onclose).not.toHaveBeenCalled();

    late.resolve({ kind: 'updated', event: ev('A', 2, 'A 일정') });
    await flush();
    await fireEvent.keyDown(document.activeElement ?? document.body, { key: 'Escape' });
    expect(onclose).toHaveBeenCalledTimes(1);
  });
});

// 감사 W21(화면 부분): 충돌에서 "서버 내용 불러오기"를 누르면 일정 입력만 다시 그리고 반복 편집기는 처음 읽은 규칙의
// 칸을 그대로 두었다. 요약은 서버의 "매주 월·수"인데 칸은 월요일만 체크된 채라, 그대로 저장하면 다른 사람이 넣은
// 수요일을 지웠다.
describe('event dialog load server', () => {
  it('rebuilds the recurrence editor from the server rule', async () => {
    await setLocale('ko', { reload: false });
    const mine: RecurrenceRule = {
      freq: 'weekly',
      interval: 1,
      weekdays: [1],
      until: null,
      count: null,
    };
    const theirs: RecurrenceRule = { ...mine, weekdays: [1, 3] };
    vi.spyOn(api, 'event').mockResolvedValue(ev('A', 5, '회의', mine));
    vi.spyOn(api, 'updateEvent').mockResolvedValue({
      kind: 'conflict',
      current: ev('A', 6, '회의', theirs),
    });
    const setRule = vi
      .spyOn(api, 'setEventRecurrence')
      .mockImplementation((_id: string, rule: RecurrenceRule | null) =>
        Promise.resolve(ev('A', 7, '회의', rule)),
      );
    render(EventDialog, {
      props: { eventId: 'A', onclose: () => undefined, onchanged: () => undefined },
    });
    await screen.findByDisplayValue('회의');
    await fireEvent.input(titleInput(), { target: { value: '내 제목' } });
    await fireEvent.submit(titleInput().closest('form') ?? titleInput());
    await fireEvent.click(await screen.findByRole('button', { name: m.events_load_server() }));
    await flush();

    const wednesday = document.getElementById('event-recurrence-wd-3');
    expect(wednesday?.getAttribute('aria-checked')).toBe('true');

    const rule = document.querySelector('[aria-labelledby="event-recurrence-heading"] form');
    if (!(rule instanceof HTMLFormElement)) throw new Error('no recurrence form');
    await fireEvent.submit(rule);
    await waitFor(() => {
      expect(setRule).toHaveBeenCalled();
    });
    expect(setRule.mock.calls[0]?.[1]?.weekdays).toEqual([1, 3]);
  });

  // 리뷰: "내 내용으로 저장"도 저장된 일정(서버 규칙 월·수)을 그대로 받아 요약만 바꾸고 반복 칸은 처음 읽은
  // 월요일 그대로 두었다. 그대로 반복을 저장하면 다른 사람이 넣은 수요일을 지웠다.
  it('rebuilds the recurrence editor after saving mine over a changed rule', async () => {
    await setLocale('ko', { reload: false });
    const mine: RecurrenceRule = {
      freq: 'weekly',
      interval: 1,
      weekdays: [1],
      until: null,
      count: null,
    };
    const theirs: RecurrenceRule = { ...mine, weekdays: [1, 3] };
    vi.spyOn(api, 'event').mockResolvedValue(ev('A', 5, '회의', mine));
    const update = vi
      .spyOn(api, 'updateEvent')
      .mockResolvedValueOnce({ kind: 'conflict', current: ev('A', 6, '회의', theirs) })
      .mockResolvedValueOnce({ kind: 'updated', event: ev('A', 7, '내 제목', theirs) });
    const setRule = vi
      .spyOn(api, 'setEventRecurrence')
      .mockImplementation((_id: string, rule: RecurrenceRule | null) =>
        Promise.resolve(ev('A', 8, '내 제목', rule)),
      );
    render(EventDialog, {
      props: { eventId: 'A', onclose: () => undefined, onchanged: () => undefined },
    });
    await screen.findByDisplayValue('회의');
    await fireEvent.input(titleInput(), { target: { value: '내 제목' } });
    await fireEvent.submit(titleInput().closest('form') ?? titleInput());
    await fireEvent.click(await screen.findByRole('button', { name: m.events_save_mine() }));
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(2);
    });
    await flush();

    const wednesday = document.getElementById('event-recurrence-wd-3');
    expect(wednesday?.getAttribute('aria-checked')).toBe('true');

    const rule = document.querySelector('[aria-labelledby="event-recurrence-heading"] form');
    if (!(rule instanceof HTMLFormElement)) throw new Error('no recurrence form');
    await fireEvent.submit(rule);
    await waitFor(() => {
      expect(setRule).toHaveBeenCalled();
    });
    expect(setRule.mock.calls[0]?.[1]?.weekdays).toEqual([1, 3]);
  });

  it('keeps unsaved rule input when an ordinary save leaves the rule unchanged', async () => {
    await setLocale('ko', { reload: false });
    const mine: RecurrenceRule = {
      freq: 'weekly',
      interval: 1,
      weekdays: [1],
      until: null,
      count: null,
    };
    vi.spyOn(api, 'event').mockResolvedValue(ev('A', 5, '회의', mine));
    vi.spyOn(api, 'updateEvent').mockImplementation((_id: string, _base: number, input) =>
      Promise.resolve({ kind: 'updated', event: ev('A', 6, input.title, mine) }),
    );
    render(EventDialog, {
      props: { eventId: 'A', onclose: () => undefined, onchanged: () => undefined },
    });
    await screen.findByDisplayValue('회의');
    const friday = document.getElementById('event-recurrence-wd-5');
    if (friday === null) throw new Error('no friday checkbox');
    await fireEvent.click(friday);
    expect(document.getElementById('event-recurrence-wd-5')?.getAttribute('aria-checked')).toBe(
      'true',
    );

    await fireEvent.input(titleInput(), { target: { value: '내 제목' } });
    await fireEvent.submit(titleInput().closest('form') ?? titleInput());
    await screen.findByDisplayValue('내 제목');
    await flush();

    expect(document.getElementById('event-recurrence-wd-5')?.getAttribute('aria-checked')).toBe(
      'true',
    );
  });
});
import './fixtures/event-user';
