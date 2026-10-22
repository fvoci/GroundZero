import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { EventView } from '$lib/api/client';
import { api } from '$lib/api/client';
import EventDialog from '$lib/features/events/EventDialog.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

afterEach(() => {
  vi.restoreAllMocks();
});

function event(version: number, title: string): EventView {
  return {
    id: 'e1',
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
    recurrence: null,
    excluded_dates: [],
    version,
    access: 'write',
  };
}

// 감사 4번: 반복 규칙을 저장하면 대화상자의 일정이 새 버전이 되지만 일정 입력은 처음 내용 그대로다. 그 사이 다른
// 사람이 일정을 고쳤다면 입력 저장은 충돌로 드러나야 한다(새 버전을 기준으로 보내 덮어쓰면 안 된다).
describe('event dialog base version after a rule save', () => {
  async function saveRuleThenForm(ruleResult: EventView): Promise<number | undefined> {
    await setLocale('ko', { reload: false });
    vi.spyOn(api, 'event').mockResolvedValue(event(1, '처음 제목'));
    const setRule = vi.spyOn(api, 'setEventRecurrence').mockResolvedValue(ruleResult);
    const update = vi
      .spyOn(api, 'updateEvent')
      .mockImplementation((_id: string, _base: number, input) =>
        Promise.resolve({ kind: 'updated', event: { ...event(9, input.title) } }),
      );
    render(EventDialog, {
      props: { eventId: 'e1', onclose: () => undefined, onchanged: () => undefined },
    });
    const title = await screen.findByLabelText('제목');
    const rule = document.querySelector('[aria-labelledby="event-recurrence-heading"] form');
    if (!(rule instanceof HTMLFormElement)) throw new Error('no recurrence form');
    await fireEvent.submit(rule);
    await waitFor(() => {
      expect(setRule).toHaveBeenCalled();
    });
    await new Promise((r) => setTimeout(r, 0));
    await fireEvent.input(title, { target: { value: '내 제목' } });
    await fireEvent.submit(title.closest('form') ?? title);
    await waitFor(() => {
      expect(update).toHaveBeenCalled();
    });
    return update.mock.calls[0]?.[1];
  }

  it('keeps the old base when someone else changed the event meanwhile', async () => {
    expect(await saveRuleThenForm(event(3, '다른 사람 제목'))).toBe(1);
  });

  it('follows the rule save when it was the only change', async () => {
    expect(await saveRuleThenForm(event(2, '처음 제목'))).toBe(2);
  });
});
