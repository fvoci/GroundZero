import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { EventInput, EventView } from '$lib/api/client';
import { api } from '$lib/api/client';
import EventDialog from '$lib/features/events/EventDialog.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';

// 사용자 결정(2026-10-22): 일정 대화상자는 고친 입력이 있어도 Esc·바깥 클릭·닫기 단추로 말없이 닫혀 입력을 버렸다.
// 이제 고친 입력이 있으면 "계속 편집 / 변경 버리고 닫기"를 묻고, 버리기를 고를 때만 닫는다. 고친 것이 없으면 바로 닫는다.
// jsdom은 클릭에 따른 초점 이동을 하지 않으므로 클릭한 곳의 초점은 `.focus()`로 흉내 낸다(실제 브라우저 확인은 따로).
afterEach(() => {
  vi.restoreAllMocks();
});
beforeEach(async () => {
  await setLocale('ko', { reload: false });
});

type UpdateResult =
  { kind: 'updated'; event: EventView } | { kind: 'conflict'; current: EventView };

function ev(id: string, fields: Partial<EventInput> = {}, version = 5): EventView {
  return {
    id,
    item_id: 'i1',
    workspace_id: 'ws-1',
    kind: 'meeting',
    title: id === 'B' ? '다른 회의' : '회의',
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

function deferred<T>(): { promise: Promise<T>; resolve: (value: T) => void } {
  let resolve: (value: T) => void = () => undefined;
  const promise = new Promise<T>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}

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

function content(): HTMLElement {
  const el = document.querySelector<HTMLElement>('[data-slot="dialog-content"]');
  if (el === null) throw new Error('no dialog');
  return el;
}

function closeButton(): HTMLElement {
  return within(content()).getByRole('button', { name: m.common_close() });
}

const confirmBox = (): HTMLElement | null =>
  screen.queryByRole('alertdialog', { name: m.common_discard_title() });

async function findConfirm(): Promise<HTMLElement> {
  return screen.findByRole('alertdialog', { name: m.common_discard_title() });
}

const flush = (): Promise<void> => new Promise((r) => setTimeout(r, 0));

function stub(...results: UpdateResult[]) {
  vi.spyOn(api, 'event').mockImplementation((id: string) => Promise.resolve(ev(id)));
  const update = vi.spyOn(api, 'updateEvent');
  for (const r of results) update.mockResolvedValueOnce(r);
  update.mockImplementation((id: string, base: number, input: EventInput) =>
    Promise.resolve({ kind: 'updated', event: ev(id, input, base + 1) }),
  );
  return update;
}

async function open(onclose: () => void = () => undefined, eventId = 'A') {
  const props = (id: string | undefined) => ({
    eventId: id,
    onclose,
    onchanged: () => undefined,
  });
  const view = render(EventDialog, { props: props(eventId) });
  await screen.findByDisplayValue(eventId === 'B' ? '다른 회의' : '회의');
  return {
    reopen: async (id: string | undefined) => {
      await view.rerender(props(id));
    },
  };
}

async function typeTitle(value: string): Promise<void> {
  await fireEvent.input(field(m.events_title()), { target: { value } });
}

/** 맨 위 가림막을 마우스로 누른다(Bits UI는 문서의 pointerdown을 10ms 늦춰 바깥 클릭으로 본다). */
async function clickOutside(): Promise<void> {
  const overlays = document.querySelectorAll<HTMLElement>(
    '[data-slot="dialog-overlay"], [data-slot="alert-dialog-overlay"]',
  );
  const top = overlays[overlays.length - 1];
  if (top === undefined) throw new Error('no overlay');
  await fireEvent.pointerDown(top, { button: 0, pointerType: 'mouse', clientX: 500, clientY: 500 });
  await fireEvent.pointerUp(top, { button: 0, pointerType: 'mouse', clientX: 500, clientY: 500 });
  await fireEvent.click(top);
  await new Promise((r) => setTimeout(r, 30));
}

async function escape(from: Element = document.activeElement ?? document.body): Promise<void> {
  await fireEvent.keyDown(from, { key: 'Escape' });
}

describe('closing the event dialog with unsaved input', () => {
  it('closes at once when nothing was changed', async () => {
    stub();
    const onclose = vi.fn();
    await open(onclose);
    field(m.events_title()).focus();
    await escape();
    expect(onclose).toHaveBeenCalledTimes(1);
    expect(confirmBox()).toBeNull();
  });

  it('closes at once from the close button when nothing was changed', async () => {
    stub();
    const onclose = vi.fn();
    await open(onclose);
    await fireEvent.click(closeButton());
    expect(onclose).toHaveBeenCalledTimes(1);
    expect(confirmBox()).toBeNull();
  });

  it('does not count spaces the server trims as a change', async () => {
    stub();
    const onclose = vi.fn();
    await open(onclose);
    await typeTitle('  회의　');
    await fireEvent.input(field(m.events_description()), { target: { value: ' \n ' } });
    field(m.events_title()).focus();
    await escape();
    expect(onclose).toHaveBeenCalledTimes(1);
  });

  it('asks before Escape drops a changed title, and Escape on the question keeps the input and focus', async () => {
    stub();
    const onclose = vi.fn();
    await open(onclose);
    await typeTitle('새 회의');
    const title = field(m.events_title());
    title.focus();
    await escape();
    const box = await findConfirm();
    expect(onclose).not.toHaveBeenCalled();
    // 초점은 확인 상자로 옮겨 읽히게 하고(Bits UI 기본), 버리기 단추에서 시작하지 않는다.
    await waitFor(() => {
      expect(document.activeElement).toBe(box);
    });

    await escape(box);
    await waitFor(() => {
      expect(confirmBox()).toBeNull();
    });
    expect(onclose).not.toHaveBeenCalled();
    expect(field(m.events_title()).value).toBe('새 회의');
    await waitFor(() => {
      expect(document.activeElement).toBe(title);
    });
  });

  it('keeps editing from the close button and returns focus inside the dialog', async () => {
    stub();
    const onclose = vi.fn();
    await open(onclose);
    await fireEvent.input(field(m.events_description()), { target: { value: '메모' } });
    const button = closeButton();
    button.focus();
    await fireEvent.click(button);
    const box = await findConfirm();
    await fireEvent.click(within(box).getByRole('button', { name: m.common_discard_keep() }));
    await waitFor(() => {
      expect(confirmBox()).toBeNull();
    });
    expect(onclose).not.toHaveBeenCalled();
    expect(field(m.events_description()).value).toBe('메모');
    await waitFor(() => {
      expect(content().contains(document.activeElement)).toBe(true);
    });
  });

  it('closes at once on an outside click when nothing was changed', async () => {
    stub();
    const onclose = vi.fn();
    await open(onclose);
    await clickOutside();
    await waitFor(() => {
      expect(onclose).toHaveBeenCalledTimes(1);
    });
    expect(confirmBox()).toBeNull();
  });

  it('asks on an outside click with a changed field, and a click outside the question changes nothing', async () => {
    stub();
    const onclose = vi.fn();
    await open(onclose);
    await typeTitle('새 회의');
    await clickOutside();
    await findConfirm();
    expect(onclose).not.toHaveBeenCalled();
    // 확인 바깥(그 뒤의 가림막)을 눌러도 확인도 대화상자도 닫히지 않는다.
    await clickOutside();
    await flush();
    expect(confirmBox()).not.toBeNull();
    expect(onclose).not.toHaveBeenCalled();
    expect(field(m.events_title()).value).toBe('새 회의');
  });

  it('closes only after I choose to discard', async () => {
    const update = stub();
    const onclose = vi.fn();
    await open(onclose);
    await typeTitle('새 회의');
    field(m.events_title()).focus();
    await escape();
    const box = await findConfirm();
    await fireEvent.click(within(box).getByRole('button', { name: m.common_discard_confirm() }));
    expect(onclose).toHaveBeenCalledTimes(1);
    expect(update).not.toHaveBeenCalled();
  });

  // 단추가 아닌 곳(설명 칸, 칸 이름, 대화상자 바탕)을 누른 뒤의 Esc도 같다.
  it('asks after clicking the description, a label or the dialog body and pressing Escape', async () => {
    stub();
    const onclose = vi.fn();
    await open(onclose);
    await fireEvent.input(field(m.events_description()), { target: { value: '메모' } });

    const description = field(m.events_description());
    description.focus();
    await escape();
    let box = await findConfirm();
    await escape(box);
    await waitFor(() => {
      expect(document.activeElement).toBe(description);
    });

    const label = screen.getByText(m.events_starts(), { selector: 'label' });
    await fireEvent.click(label);
    await escape(label);
    box = await findConfirm();
    await escape(box);
    await waitFor(() => {
      expect(confirmBox()).toBeNull();
    });

    content().focus();
    await escape(content());
    box = await findConfirm();
    await escape(box);
    await waitFor(() => {
      expect(confirmBox()).toBeNull();
    });
    await waitFor(() => {
      expect(content().contains(document.activeElement)).toBe(true);
    });
    expect(onclose).not.toHaveBeenCalled();
    expect(field(m.events_description()).value).toBe('메모');
  });

  it('closes at once again after the change was saved', async () => {
    stub();
    const onclose = vi.fn();
    await open(onclose);
    await typeTitle('새 회의');
    await fireEvent.submit(eventForm());
    await screen.findByText('새 회의', { selector: '[data-slot="dialog-title"]' });
    field(m.events_title()).focus();
    await escape();
    expect(onclose).toHaveBeenCalledTimes(1);
    expect(confirmBox()).toBeNull();
  });

  it('neither asks nor closes while saving', async () => {
    vi.spyOn(api, 'event').mockResolvedValue(ev('A'));
    const late = deferred<UpdateResult>();
    vi.spyOn(api, 'updateEvent').mockReturnValue(late.promise);
    const onclose = vi.fn();
    await open(onclose);
    await typeTitle('새 회의');
    await fireEvent.submit(eventForm());
    field(m.events_title()).focus();
    await escape();
    await flush();
    expect(onclose).not.toHaveBeenCalled();
    expect(confirmBox()).toBeNull();
    late.resolve({ kind: 'updated', event: ev('A', { title: '새 회의' }, 6) });
  });

  it('keeps the conflict notice and my input when I keep editing after a conflict', async () => {
    stub({ kind: 'conflict', current: ev('A', { description: '다른 사람' }, 6) });
    const onclose = vi.fn();
    await open(onclose);
    await typeTitle('새 회의');
    await fireEvent.submit(eventForm());
    await screen.findByRole('button', { name: m.events_save_mine() });
    await fireEvent.click(closeButton());
    const box = await findConfirm();
    await fireEvent.click(within(box).getByRole('button', { name: m.common_discard_keep() }));
    await waitFor(() => {
      expect(confirmBox()).toBeNull();
    });
    expect(onclose).not.toHaveBeenCalled();
    expect(screen.getByRole('button', { name: m.events_save_mine() })).toBeInstanceOf(
      HTMLButtonElement,
    );
    expect(field(m.events_title()).value).toBe('새 회의');
  });

  it('keeps the overwrite question when I keep editing from the close button', async () => {
    stub({ kind: 'conflict', current: ev('A', { title: '그쪽 제목' }, 6) });
    const onclose = vi.fn();
    await open(onclose);
    await typeTitle('내 제목');
    await fireEvent.submit(eventForm());
    await fireEvent.click(await screen.findByRole('button', { name: m.events_save_mine() }));
    await screen.findByRole('alertdialog', { name: m.events_clash_question() });
    await fireEvent.click(closeButton());
    const box = await findConfirm();
    await escape(box);
    await waitFor(() => {
      expect(confirmBox()).toBeNull();
    });
    expect(onclose).not.toHaveBeenCalled();
    expect(screen.getByRole('alertdialog', { name: m.events_clash_question() })).toBeInstanceOf(
      HTMLElement,
    );
    expect(field(m.events_title()).value).toBe('내 제목');
  });

  it('drops the question when another event opens, and does not close or carry over to it', async () => {
    stub();
    const onclose = vi.fn();
    const { reopen } = await open(onclose);
    await typeTitle('새 회의');
    field(m.events_title()).focus();
    await escape();
    await findConfirm();
    await reopen('B');
    await screen.findByDisplayValue('다른 회의');
    await waitFor(() => {
      expect(confirmBox()).toBeNull();
    });
    expect(onclose).not.toHaveBeenCalled();
    // 새로 연 일정은 고친 것이 없으므로 바로 닫힌다.
    field(m.events_title()).focus();
    await escape();
    expect(onclose).toHaveBeenCalledTimes(1);
  });
});
