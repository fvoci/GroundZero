import './fixtures/event-user';
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { tick } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { ApiError } from '$lib/api/client';
import { RuleConflict, eventsApi } from '$lib/features/events/api';
import type { EventView } from '$lib/features/events/model';
import EventDialog from '$lib/features/events/EventDialog.svelte';
import { me } from '$lib/me.svelte';
import { readSessionGeneration } from '$lib/features/auth/session-signal';
import { m } from '$lib/paraglide/messages.js';
import { switchAccount } from '$lib/features/feedback/feedback.svelte';

const base: EventView = {
  id: 'A',
  item_id: 'i1',
  workspace_id: 'ws-1',
  kind: 'meeting',
  title: '처음 제목',
  description: '처음 설명',
  all_day: true,
  start_date: '2026-11-02',
  end_date: '2026-11-02',
  starts_at: null,
  ends_at: null,
  timezone: 'Asia/Seoul',
  recurrence: null,
  excluded_dates: [],
  version: 5,
  access: 'write',
};
const server: EventView = { ...base, title: '서버 제목', description: '서버 설명', version: 6 };
beforeEach(() => {
  vi.spyOn(eventsApi, 'event').mockResolvedValue({
    ...base,
    viewer_id: 'event-test-user',
    archived: false,
  });
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});
async function mount() {
  const onclose = vi.fn(),
    onchanged = vi.fn();
  const view = render(EventDialog, { eventId: base.id, onclose, onchanged });
  await screen.findByLabelText('제목');
  return { ...view, onclose, onchanged };
}
async function edit(label: string, value: string) {
  await fireEvent.input(screen.getByLabelText(label), { target: { value } });
}
async function conflict() {
  await edit('제목', '내 제목');
  await edit('설명', '내 설명');
  await fireEvent.click(screen.getByRole('button', { name: '저장' }));
  return screen.findByRole('region', { name: '바뀐 내용 확인' });
}
function choose(panel: HTMLElement, label: string, side: '내 입력' | '서버 최신') {
  return fireEvent.click(within(panel).getByRole('radio', { name: `${label}: ${side} 사용` }));
}
const save = (panel: HTMLElement) =>
  within(panel).getByRole('button', { name: '선택한 내용으로 저장' });

it('opens on the first 409, focuses missing choices and saves mixed fields with the reviewed version', async () => {
  const update = vi
    .spyOn(eventsApi, 'updateEvent')
    .mockResolvedValueOnce({ kind: 'conflict', current: server })
    .mockResolvedValueOnce({
      kind: 'updated',
      event: { ...server, description: '내 설명', version: 7 },
    });
  await mount();
  const panel = await conflict();
  expect(within(panel).getByRole('heading', { name: '바뀐 내용 확인' })).toHaveFocus();
  await fireEvent.click(save(panel));
  expect(within(panel).getByRole('alert')).toHaveTextContent('2');
  await waitFor(() => {
    expect(within(panel).getByRole('radio', { name: '제목: 내 입력 사용' })).toHaveFocus();
  });
  expect(update).toHaveBeenCalledOnce();
  await choose(panel, '제목', '서버 최신');
  await choose(panel, '설명', '내 입력');
  await fireEvent.click(save(panel));
  await waitFor(() => {
    expect(update).toHaveBeenCalledTimes(2);
  });
  expect(update.mock.calls[1]?.[1]).toBe(6);
  expect(update.mock.calls[1]?.[2]).toMatchObject({ title: '서버 제목', description: '내 설명' });
  expect(vi.mocked(eventsApi.event)).toHaveBeenCalledOnce();
  await waitFor(() => {
    expect(screen.queryByRole('region', { name: '바뀐 내용 확인' })).toBeNull();
  });
});

it('folds server-only changes, defaults mine-only values and never retransmits automatically', async () => {
  const current = { ...server, description: base.description, kind: 'presentation' as const };
  const update = vi
    .spyOn(eventsApi, 'updateEvent')
    .mockResolvedValueOnce({ kind: 'conflict', current })
    .mockRejectedValueOnce(new ApiError(503, 'unavailable', 'failed'));
  await mount();
  await edit('설명', '내 설명');
  await fireEvent.click(screen.getByRole('button', { name: '저장' }));
  const panel = await screen.findByRole('region', { name: '바뀐 내용 확인' });
  const summary = within(panel).getByText('다른 사람이 바꾼 칸 2개(서버 값 유지)');
  expect(summary.closest('details')).not.toHaveAttribute('open');
  expect(within(panel).getByRole('radio', { name: '설명: 내 입력 사용' })).toBeChecked();
  expect(update).toHaveBeenCalledOnce();
  await fireEvent.click(save(panel));
  await waitFor(() => {
    expect(update).toHaveBeenCalledTimes(2);
  });
  expect(update.mock.calls[1]?.[2]).toMatchObject({
    kind: 'presentation',
    title: '서버 제목',
    description: '내 설명',
  });
});

it('reopens immediately on another conflict and keeps the original and chosen candidate after failure', async () => {
  const update = vi
    .spyOn(eventsApi, 'updateEvent')
    .mockResolvedValueOnce({ kind: 'conflict', current: server })
    .mockResolvedValueOnce({
      kind: 'conflict',
      current: { ...server, version: 7, description: '다시 바뀐 설명' },
    })
    .mockRejectedValueOnce(new ApiError(503, 'unavailable', 'failed'));
  await mount();
  let panel = await conflict();
  await choose(panel, '제목', '서버 최신');
  await choose(panel, '설명', '내 입력');
  await fireEvent.click(save(panel));
  await screen.findByDisplayValue('다시 바뀐 설명');
  panel = screen.getByRole('region', { name: '바뀐 내용 확인' });
  expect(within(panel).getByRole('radio', { name: '설명: 내 입력 사용' })).not.toBeChecked();
  expect(update).toHaveBeenCalledTimes(2);
  await choose(panel, '설명', '내 입력');
  await fireEvent.click(save(panel));
  await waitFor(() => {
    expect(update).toHaveBeenCalledTimes(3);
    expect(save(panel)).toBeEnabled();
  });
  expect(Object.values(localStorage).join()).toContain('내 제목');
  expect(Object.values(localStorage).join()).toContain('내 설명');
  await fireEvent.click(within(panel).getByRole('button', { name: '처음 입력 복원' }));
  expect(screen.getByLabelText('제목')).toHaveValue('내 제목');
  expect(screen.getByLabelText('설명')).toHaveValue('내 설명');
});

it('keeps all date, time, all-day and timezone values in one choice and keeps full long text', async () => {
  const current = {
    ...server,
    all_day: false,
    start_date: null,
    end_date: null,
    starts_at: '2026-11-02T03:00:01Z',
    ends_at: '2026-11-02T04:00:02Z',
    timezone: 'UTC',
  };
  const update = vi
    .spyOn(eventsApi, 'updateEvent')
    .mockResolvedValueOnce({ kind: 'conflict', current })
    .mockRejectedValueOnce(new ApiError(503, 'unavailable', 'failed'));
  await mount();
  const text = '긴 설명\n'.repeat(100);
  await edit('제목', '내 제목');
  await edit('설명', text);
  await edit(m.events_start_date(), '2026-11-03');
  await edit(m.events_end_date(), '2026-11-04');
  await fireEvent.click(screen.getByRole('button', { name: '저장' }));
  const panel = await screen.findByRole('region', { name: '바뀐 내용 확인' });
  expect(within(panel).getByRole('textbox', { name: '설명: 내 입력' })).toHaveValue(text);
  await choose(panel, '제목', '서버 최신');
  await choose(panel, '설명', '내 입력');
  await choose(panel, m.events_clash_when(), '서버 최신');
  await fireEvent.click(save(panel));
  await waitFor(() => {
    expect(update).toHaveBeenCalledTimes(2);
  });
  expect(update.mock.calls[1]?.[2]).toMatchObject({
    all_day: false,
    start_date: null,
    end_date: null,
    starts_at: current.starts_at,
    ends_at: current.ends_at,
    timezone: 'UTC',
  });
});

it('applies choices to the existing form for editing and closes comparison alone with Escape', async () => {
  const update = vi
    .spyOn(eventsApi, 'updateEvent')
    .mockResolvedValueOnce({ kind: 'conflict', current: server })
    .mockRejectedValueOnce(new ApiError(503, 'unavailable', 'failed'));
  const { onclose } = await mount();
  const panel = await conflict();
  await choose(panel, '제목', '서버 최신');
  await choose(panel, '설명', '내 입력');
  await fireEvent.click(within(panel).getByRole('button', { name: '선택 적용 후 계속 편집' }));
  expect(screen.getByLabelText('제목')).toHaveValue('서버 제목');
  await edit('설명', '내 보충 설명');
  await fireEvent.click(screen.getByRole('button', { name: '저장' }));
  await waitFor(() => {
    expect(update).toHaveBeenCalledTimes(2);
  });
  expect(update.mock.calls[1]?.[1]).toBe(6);
  expect(update.mock.calls[1]?.[2]).toMatchObject({
    title: '서버 제목',
    description: '내 보충 설명',
  });
  expect(onclose).not.toHaveBeenCalled();
});

it('does not write when explicitly choosing values that already match the reviewed snapshot', async () => {
  const update = vi
    .spyOn(eventsApi, 'updateEvent')
    .mockResolvedValueOnce({ kind: 'conflict', current: server });
  await mount();
  const panel = await conflict();
  await choose(panel, '제목', '서버 최신');
  await choose(panel, '설명', '서버 최신');
  await fireEvent.click(save(panel));
  await waitFor(() => {
    expect(screen.queryByRole('region', { name: '바뀐 내용 확인' })).toBeNull();
  });
  expect(screen.getByLabelText('제목')).toHaveValue(server.title);
  expect(screen.getByLabelText('설명')).toHaveValue(server.description);
  expect(update).toHaveBeenCalledOnce();
});

it.each([403, 404])(
  'blocks a %s without losing the comparison or its original input',
  async (status) => {
    vi.spyOn(eventsApi, 'updateEvent')
      .mockResolvedValueOnce({ kind: 'conflict', current: server })
      .mockRejectedValueOnce(new ApiError(status, 'forbidden', 'revoked'));
    await mount();
    const panel = await conflict();
    await choose(panel, '제목', '내 입력');
    await choose(panel, '설명', '내 입력');
    await fireEvent.click(save(panel));
    await waitFor(() => {
      expect(Object.values(localStorage).join()).toContain('내 제목');
    });
    if (status === 403)
      await waitFor(() => {
        expect(save(panel)).toBeDisabled();
      });
    else
      await waitFor(() => {
        expect(panel).not.toBeVisible();
      });
    expect(Object.values(localStorage).join()).toContain('내 설명');
  },
);

it('discards a late write from an earlier A→B→A session epoch', async () => {
  let resolve!: (result: Awaited<ReturnType<typeof eventsApi.updateEvent>>) => void;
  const pending = new Promise<Awaited<ReturnType<typeof eventsApi.updateEvent>>>((yes) => {
    resolve = yes;
  });
  vi.spyOn(eventsApi, 'updateEvent').mockReturnValueOnce(pending);
  const { onchanged } = await mount();
  await edit('제목', '내 제목');
  await fireEvent.click(screen.getByRole('button', { name: '저장' }));
  for (const sequence of ['B', 'A'])
    localStorage.setItem(
      'fvoci:auth-session',
      JSON.stringify({
        state: 'settled',
        generation: readSessionGeneration() + 1,
        sequence,
      }),
    );
  resolve({ kind: 'updated', event: { ...server, title: '늦은 응답' } });
  await pending;
  await tick();
  await waitFor(() => {
    expect(screen.getByLabelText('제목')).toHaveValue('내 제목');
  });
  expect(onchanged).not.toHaveBeenCalled();
  expect(me.userId).toBe('event-test-user');
});

it('keeps a temporary invalid_state retryable after one owned access check, locking while it runs', async () => {
  let finish!: (value: Awaited<ReturnType<typeof eventsApi.event>>) => void;
  vi.mocked(eventsApi.event)
    .mockResolvedValueOnce({ ...base, viewer_id: me.userId, archived: false })
    .mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
  const update = vi
    .spyOn(eventsApi, 'updateEvent')
    .mockRejectedValueOnce(new ApiError(409, 'invalid_state', 'temporary'))
    .mockResolvedValueOnce({ kind: 'updated', event: { ...base, title: '내 제목', version: 6 } });
  await mount();
  await edit('제목', '내 제목');
  const button = screen.getByRole('button', { name: '저장' });
  await fireEvent.click(button);
  await waitFor(() => {
    expect(eventsApi.event).toHaveBeenCalledTimes(2);
  });
  expect(button).toBeDisabled();
  finish({ ...base, viewer_id: me.userId, archived: false });
  await waitFor(() => {
    expect(button).toBeEnabled();
  });
  expect(screen.getByLabelText('제목')).toHaveValue('내 제목');
  await fireEvent.click(button);
  await waitFor(() => {
    expect(update).toHaveBeenCalledTimes(2);
  });
  expect(update.mock.calls[1]?.[1]).toBe(5);
});

it('restores the comparison, candidate and original after remount and Escape returns only to the form', async () => {
  vi.spyOn(eventsApi, 'updateEvent').mockResolvedValue({ kind: 'conflict', current: server });
  const { unmount } = await mount();
  const panel = await conflict();
  await choose(panel, '제목', '서버 최신');
  await choose(panel, '설명', '내 입력');
  await waitFor(() => {
    expect(Object.values(localStorage).join()).toContain('choices');
  });
  unmount();
  const { onclose } = await mount();
  await fireEvent.click(await screen.findByRole('button', { name: m.common_draft_restore() }));
  const restored = await screen.findByRole('region', { name: '바뀐 내용 확인' });
  expect(within(restored).getByRole('radio', { name: '제목: 서버 최신 사용' })).toBeChecked();
  await fireEvent.keyDown(within(restored).getByRole('heading'), { key: 'Escape' });
  await waitFor(() => {
    expect(screen.queryByRole('region', { name: '바뀐 내용 확인' })).toBeNull();
  });
  expect(onclose).not.toHaveBeenCalled();
  expect(screen.getByLabelText('제목')).toHaveValue('내 제목');
});

it.each(['read', 'archived', 'unavailable'] as const)(
  'rechecks invalid_state without changing input or base (%s)',
  async (access) => {
    const reads = vi.mocked(eventsApi.event);
    reads.mockResolvedValueOnce({ ...base, viewer_id: me.userId, archived: false });
    if (access === 'unavailable')
      reads.mockRejectedValueOnce(new ApiError(503, 'unavailable', 'retry'));
    else
      reads.mockResolvedValueOnce({
        ...server,
        access: access === 'read' ? 'read' : 'write',
        archived: access === 'archived',
        viewer_id: me.userId,
      });
    const update = vi
      .spyOn(eventsApi, 'updateEvent')
      .mockRejectedValue(new ApiError(409, 'invalid_state', 'temporary'));
    await mount();
    await edit('제목', '내 제목');
    const button = screen.getByRole('button', { name: '저장' });
    await fireEvent.click(button);
    await waitFor(() => {
      expect(reads).toHaveBeenCalledTimes(2);
    });
    if (access === 'unavailable')
      await waitFor(() => {
        expect(button).toBeEnabled();
      });
    else
      await waitFor(() => {
        expect(button).toBeDisabled();
      });
    expect(screen.getByLabelText('제목')).toHaveValue('내 제목');
    expect(update).toHaveBeenCalledOnce();
    expect(Object.values(localStorage).join()).toContain('내 제목');
  },
);

it('hides comparison and its draft offer when the authenticated account changes', async () => {
  vi.spyOn(eventsApi, 'updateEvent').mockResolvedValue({ kind: 'conflict', current: server });
  await mount();
  await conflict();
  me.userId = 'different-user';
  await waitFor(() => {
    expect(screen.queryByRole('region', { name: '바뀐 내용 확인' })).toBeNull();
  });
  expect(screen.queryByDisplayValue('내 제목')).toBeNull();
  expect(screen.queryByDisplayValue('서버 제목')).toBeNull();
  expect(screen.queryByRole('button', { name: m.common_draft_restore() })).toBeNull();
  expect(Object.values(localStorage).join()).toContain('내 제목');
});

it('rejects overlapping recurrence submissions while the content comparison is open', async () => {
  vi.mocked(eventsApi.event).mockResolvedValue({
    ...base,
    recurrence: { freq: 'daily', interval: 1, weekdays: [], count: null, until: null },
    viewer_id: me.userId,
    archived: false,
  });
  vi.spyOn(eventsApi, 'updateEvent').mockResolvedValue({ kind: 'conflict', current: server });
  const rules = vi.spyOn(eventsApi, 'setEventRecurrence');
  await mount();
  await conflict();
  const form = document.querySelector('[aria-labelledby="event-recurrence-heading"] form');
  if (!(form instanceof HTMLFormElement)) throw new Error('missing rule form');
  expect(
    [...document.querySelectorAll('fieldset')].some((field) => field.inert && field.contains(form)),
  ).toBe(true);
  await fireEvent.submit(form);
  expect(rules).not.toHaveBeenCalled();
});

it('does not apply a late access recheck to the next account or lock its form', async () => {
  let finish!: (value: Awaited<ReturnType<typeof eventsApi.event>>) => void;
  vi.mocked(eventsApi.event)
    .mockResolvedValueOnce({ ...base, viewer_id: me.userId, archived: false })
    .mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    )
    .mockImplementationOnce(() =>
      Promise.resolve({
        ...base,
        title: '다른 계정 일정',
        viewer_id: me.userId,
        archived: false,
      }),
    );
  vi.spyOn(eventsApi, 'updateEvent').mockRejectedValue(
    new ApiError(409, 'invalid_state', 'temporary'),
  );
  await mount();
  await edit('제목', '내 제목');
  await fireEvent.click(screen.getByRole('button', { name: '저장' }));
  await waitFor(() => {
    expect(eventsApi.event).toHaveBeenCalledTimes(2);
  });
  me.userId = 'different-user';
  await screen.findByDisplayValue('다른 계정 일정');
  finish({ ...base, access: 'read', viewer_id: 'event-test-user', archived: true });
  await new Promise((resolve) => setTimeout(resolve, 0));
  expect(screen.getByRole('button', { name: '저장' })).toBeEnabled();
  expect(screen.queryByDisplayValue('내 제목')).toBeNull();
});

it('hides the comparison when the owned access check detects a different server session', async () => {
  vi.mocked(eventsApi.event)
    .mockResolvedValueOnce({ ...base, viewer_id: me.userId, archived: false })
    .mockImplementationOnce(() => {
      switchAccount();
      return Promise.reject(new ApiError(409, 'account_changed', 'different session'));
    });
  vi.spyOn(eventsApi, 'updateEvent')
    .mockResolvedValueOnce({ kind: 'conflict', current: server })
    .mockRejectedValueOnce(new ApiError(409, 'invalid_state', 'temporary'));
  await mount();
  const panel = await conflict();
  await choose(panel, '제목', '내 입력');
  await choose(panel, '설명', '내 입력');
  await fireEvent.click(save(panel));
  await waitFor(() => {
    expect(eventsApi.event).toHaveBeenCalledTimes(2);
  });
  await waitFor(() => {
    expect(panel).not.toBeVisible();
  });
  expect(Object.values(localStorage).join()).toContain('내 제목');
});

it('announces an until rejection outside the inert editors while reviewing event fields', async () => {
  const rule = {
    freq: 'daily' as const,
    interval: 1,
    weekdays: [],
    until: '2026-11-10',
    count: null,
  };
  vi.mocked(eventsApi.event).mockResolvedValue({
    ...base,
    recurrence: rule,
    viewer_id: me.userId,
    archived: false,
  });
  const update = vi
    .spyOn(eventsApi, 'updateEvent')
    .mockResolvedValueOnce({
      kind: 'conflict',
      current: { ...server, recurrence: { ...rule, until: '2026-11-04' } },
    })
    .mockRejectedValueOnce(
      new ApiError(
        400,
        'bad_request',
        'until before start',
        undefined,
        'until_before_first_occurrence',
        'until',
      ),
    );
  await mount();
  await edit(m.events_start_date(), '2026-11-05');
  await edit(m.events_end_date(), '2026-11-05');
  const panel = await conflict();
  await choose(panel, '제목', '내 입력');
  await choose(panel, '설명', '내 입력');
  await fireEvent.click(save(panel));
  await waitFor(() => {
    expect(update).toHaveBeenCalledTimes(2);
  });
  await waitFor(() => {
    const inert = [...document.querySelectorAll('fieldset')].filter((field) => field.inert);
    expect(
      screen.getAllByRole('alert').some((alert) => !inert.some((field) => field.contains(alert))),
    ).toBe(true);
  });
});

it('reopens a restored comparison against a newer read instead of replacing it with the stale snapshot', async () => {
  const newer = { ...server, title: '다른 탭 제목', version: 7 };
  const update = vi
    .spyOn(eventsApi, 'updateEvent')
    .mockResolvedValueOnce({ kind: 'conflict', current: server })
    .mockResolvedValueOnce({
      kind: 'updated',
      event: { ...newer, description: '마지막 설명', version: 8 },
    });
  const { unmount } = await mount();
  await conflict();
  await waitFor(() => {
    expect(Object.values(localStorage).join()).toContain('choices');
  });
  unmount();
  vi.mocked(eventsApi.event).mockResolvedValue({
    ...newer,
    viewer_id: 'event-test-user',
    archived: false,
  });
  const { onchanged } = await mount();
  await fireEvent.click(await screen.findByRole('button', { name: m.common_draft_restore() }));
  const restored = await screen.findByRole('region', { name: '바뀐 내용 확인' });
  await choose(restored, '제목', '서버 최신');
  await choose(restored, '설명', '서버 최신');
  await fireEvent.click(save(restored));
  const reopened = await screen.findByRole('region', { name: '바뀐 내용 확인' });
  await waitFor(() => {
    expect(within(reopened).getByText('다른 탭 제목')).toBeInTheDocument();
  });
  expect(screen.getByRole('heading', { name: '다른 탭 제목' })).toBeInTheDocument();
  expect(update).toHaveBeenCalledOnce();
  expect(onchanged).not.toHaveBeenCalled();
  await fireEvent.click(save(reopened));
  await waitFor(() => {
    expect(screen.queryByRole('region', { name: '바뀐 내용 확인' })).toBeNull();
  });
  expect(screen.getByLabelText('제목')).toHaveValue('다른 탭 제목');
  expect(update).toHaveBeenCalledOnce();
  await edit('설명', '마지막 설명');
  await fireEvent.click(screen.getByRole('button', { name: '저장' }));
  await waitFor(() => {
    expect(update).toHaveBeenCalledTimes(2);
  });
  expect(update.mock.calls[1]?.[1]).toBe(7);
});

it('does not restore a comparison from a draft whose input belongs to another event', async () => {
  const update = vi
    .spyOn(eventsApi, 'updateEvent')
    .mockResolvedValueOnce({ kind: 'conflict', current: server });
  const { unmount } = await mount();
  await conflict();
  await waitFor(() => {
    expect(Object.values(localStorage).join()).toContain('choices');
  });
  unmount();
  const key = Object.keys(localStorage).find((k) => localStorage.getItem(k)?.includes('choices'));
  if (key === undefined) throw new Error('missing draft');
  const stored = localStorage.getItem(key) ?? '';
  const other = stored.replaceAll('"id":"A"', '"id":"B"');
  expect(other).not.toBe(stored);
  localStorage.setItem(key, other);
  const { onchanged } = await mount();
  await fireEvent.click(await screen.findByRole('button', { name: m.common_draft_restore() }));
  await tick();
  expect(screen.queryByRole('region', { name: '바뀐 내용 확인' })).toBeNull();
  expect(screen.getByLabelText('제목')).toHaveValue(base.title);
  await fireEvent.click(screen.getByRole('button', { name: '저장' }));
  await waitFor(() => {
    expect(update).toHaveBeenCalledTimes(2);
  });
  expect(update.mock.calls[1]?.[1]).toBe(base.version);
  expect(onchanged).not.toHaveBeenCalled();
});

// CodeRabbit(PR #305 discussion_r4441100234, D-128 (1)): 비교가 열린 채 "변경 버리고 닫기"를 고르면 입력을 기준값으로
// 되돌려 초안을 저장할 때 부모의 비교가 아직 남아 있어, 버린 비교가 초안에 담겨 같은 일정을 다시 열면 복원 제안으로 나왔다.
async function discardFromCloseButton() {
  const dialog = document.querySelector<HTMLElement>('[data-slot="dialog-content"]');
  if (dialog === null) throw new Error('no dialog');
  await fireEvent.click(within(dialog).getByRole('button', { name: m.common_close() }));
  const box = await screen.findByRole('alertdialog', { name: m.common_discard_title() });
  await fireEvent.click(within(box).getByRole('button', { name: m.common_discard_confirm() }));
}

it('leaves no draft offer after discarding an open content comparison and reopening the event', async () => {
  vi.spyOn(eventsApi, 'updateEvent').mockResolvedValueOnce({ kind: 'conflict', current: server });
  const { onclose, rerender } = await mount();
  await conflict();
  await waitFor(() => {
    expect(Object.values(localStorage).join()).toContain('choices');
  });
  await discardFromCloseButton();
  expect(onclose).toHaveBeenCalledOnce();
  await tick();
  expect(Object.values(localStorage).join()).not.toContain('choices');
  await rerender({ eventId: undefined });
  await tick();
  await rerender({ eventId: base.id });
  await screen.findByLabelText('제목');
  expect(screen.queryByRole('button', { name: m.common_draft_restore() })).toBeNull();
  expect(screen.queryByRole('region', { name: '바뀐 내용 확인' })).toBeNull();
  expect(screen.getByLabelText('제목')).toHaveValue(base.title);
  expect(Object.values(localStorage).join()).not.toContain('choices');
});

// 반복 규칙 비교는 편집기 안의 상태라 편집기의 초안 초기화가 함께 비운다. 같은 모양이 다시 생기지 않게 지킨다.
it('leaves no draft offer after discarding an open rule comparison and reopening the event', async () => {
  const rule = { freq: 'daily' as const, interval: 1, weekdays: [], until: null, count: null };
  vi.mocked(eventsApi.event).mockResolvedValue({
    ...base,
    recurrence: rule,
    viewer_id: me.userId,
    archived: false,
  });
  vi.spyOn(eventsApi, 'setEventRecurrence').mockRejectedValueOnce(
    new RuleConflict({ ...base, recurrence: { ...rule, interval: 3 }, version: 6 }),
  );
  const { onclose, rerender } = await mount();
  await edit('간격', '2');
  await fireEvent.click(screen.getByRole('button', { name: '반복 저장' }));
  await screen.findByRole('region', { name: '바뀐 내용 확인' });
  await discardFromCloseButton();
  expect(onclose).toHaveBeenCalledOnce();
  await rerender({ eventId: undefined });
  await tick();
  await rerender({ eventId: base.id });
  await screen.findByLabelText('간격');
  expect(screen.queryByRole('button', { name: m.common_draft_restore() })).toBeNull();
  expect(screen.queryByRole('region', { name: '바뀐 내용 확인' })).toBeNull();
  expect(screen.getByLabelText('간격')).toHaveValue('1');
  expect(Object.values(localStorage).join()).not.toContain('choices');
});
