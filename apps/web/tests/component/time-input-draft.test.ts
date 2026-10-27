import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { api, ApiError, NetworkError, type TimeEntryView } from '$lib/api/client';
import TimeEntryDialog from '$lib/features/time/TimeEntryDialog.svelte';
import TimeScreen from '$lib/features/time/TimeScreen.svelte';
import { me } from '$lib/me.svelte';
import { org } from '$lib/org.svelte';
import { router } from '$lib/router.svelte';
import { formDraftTabKey, writeFormDraftAt } from '$lib/features/common/form-draft';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';

import { TEST_ORGANIZATION } from './fixtures/organization';

const entry: TimeEntryView = {
  id: 'entry-a',
  started_at: '2026-11-01T04:00:37Z',
  ended_at: '2026-11-01T06:15:19Z',
  paused_at: null,
  paused_seconds: 37,
  seconds: 8045,
  kind: 'work',
  memo: 'saved',
  target: null,
  version: 3,
};
const props = { entry, zone: 'America/New_York', onclose: vi.fn(), onchanged: vi.fn() };

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  localStorage.clear();
  Object.assign(me, { userId: 'user-a', timeZone: props.zone, hourCycle: 'h23' });
  vi.spyOn(api, 'timeEntries').mockResolvedValue([]);
  vi.spyOn(api, 'timeSummary').mockResolvedValue([]);
  vi.spyOn(api, 'currentTimer').mockResolvedValue(null);
  vi.spyOn(api, 'timeEntryRevisions').mockResolvedValue([]);
  props.onclose.mockClear();
  props.onchanged.mockClear();
});
afterEach(() => {
  cleanup();
  localStorage.clear();
  Object.assign(me, { userId: '', timeZone: '', hourCycle: '' });
  vi.restoreAllMocks();
});
async function submit(): Promise<void> {
  const form = screen.getByLabelText(m.time_memo()).closest('form');
  if (form === null) throw new Error('missing form');
  await fireEvent.submit(form);
}
async function memo(value: string): Promise<void> {
  await fireEvent.input(await screen.findByLabelText(m.time_memo()), { target: { value } });
}

it('restores manual fields and the failed request ID only on explicit selection', async () => {
  const add = vi.spyOn(api, 'addTimeEntry').mockRejectedValue(new NetworkError('offline'));
  render(TimeScreen);
  await memo('manual private memo');
  await fireEvent.input(screen.getByLabelText(m.time_date()), { target: { value: '2026-10-20' } });
  await fireEvent.input(screen.getByLabelText(m.time_start()), { target: { value: '22:37' } });
  await submit();
  await screen.findByRole('alert');
  const first = add.mock.calls[0];
  cleanup();
  render(TimeScreen);
  await screen.findByRole('button', { name: m.common_draft_restore() });
  expect(screen.getByLabelText(m.time_memo())).toHaveValue('');
  expect(add).toHaveBeenCalledTimes(1);
  await fireEvent.click(screen.getByRole('button', { name: m.common_draft_restore() }));
  expect(screen.getByLabelText(m.time_memo())).toHaveValue('manual private memo');
  expect(screen.getByLabelText(m.time_date())).toHaveValue('2026-10-20');
  expect(screen.getByLabelText(m.time_start())).toHaveValue('22:37');
  await submit();
  await waitFor(() => {
    expect(add).toHaveBeenCalledTimes(2);
  });
  expect(add.mock.calls[1]).toEqual(first);
});

it('preserves the edited memo and original base version after a conflict and remount', async () => {
  const update = vi.spyOn(api, 'updateTimeEntry').mockResolvedValue({
    kind: 'conflict',
    current: { ...entry, memo: 'server latest', version: 4 },
  });
  render(TimeEntryDialog, props);
  await memo('private edit');
  await submit();
  await screen.findByRole('alert');
  expect(screen.getByLabelText(m.time_memo())).toHaveValue('private edit');
  cleanup();
  render(TimeEntryDialog, { ...props, entry: { ...entry, version: 4 } });
  await fireEvent.click(await screen.findByRole('button', { name: m.common_draft_restore() }));
  expect(update).toHaveBeenCalledTimes(1);
  await submit();
  await waitFor(() => {
    expect(update).toHaveBeenCalledTimes(2);
  });
  expect(update.mock.calls[1]?.[1]).toBe(3);
  expect(update.mock.calls[1]?.[2]).toMatchObject({
    memo: 'private edit',
    started_at: entry.started_at,
    ended_at: entry.ended_at,
    paused_seconds: 37,
  });
});

it('warns on memo-only edits before closing and keeps editing on cancel', async () => {
  render(TimeEntryDialog, props);
  await memo('private edit');
  const unload = new Event('beforeunload', { cancelable: true });
  window.dispatchEvent(unload);
  expect(unload.defaultPrevented).toBe(true);
  await fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
  await screen.findByRole('alertdialog');
  expect(props.onclose).not.toHaveBeenCalled();
  await fireEvent.click(screen.getByRole('button', { name: m.common_discard_keep() }));
  expect(screen.getByLabelText(m.time_memo())).toHaveValue('private edit');
});

it('restores the DST choice, original seconds, target, and unfinished paused string', async () => {
  const update = vi.spyOn(api, 'updateTimeEntry').mockResolvedValue({ kind: 'updated', entry });
  const linked = {
    ...entry,
    target: { kind: 'task' as const, id: 'task-a', label: 'My task', workspace_id: 'ws-a' },
  };
  render(TimeEntryDialog, { ...props, entry: linked });
  await memo('DST edit');
  await fireEvent.input(screen.getByLabelText(m.time_end()), {
    target: { value: '2026-11-01T01:45' },
  });
  await fireEvent.click(screen.getByRole('radio', { name: '두 번째 01:45 (UTC−05:00)' }));
  await fireEvent.input(screen.getByLabelText(m.time_paused_minutes()), {
    target: { value: '1e-' },
  });
  cleanup();
  render(TimeEntryDialog, { ...props, entry: linked });
  await fireEvent.click(await screen.findByRole('button', { name: m.common_draft_restore() }));
  expect(screen.getByRole('radio', { name: '두 번째 01:45 (UTC−05:00)' })).toBeChecked();
  expect(screen.getByLabelText(m.time_paused_minutes())).toHaveValue('1e-');
  expect(update).not.toHaveBeenCalled();
  await submit();
  expect(update).not.toHaveBeenCalled();
  await fireEvent.input(screen.getByLabelText(m.time_paused_minutes()), { target: { value: '1' } });
  await submit();
  await waitFor(() => {
    expect(update).toHaveBeenCalledOnce();
  });
  expect(update.mock.calls[0]).toEqual([
    'entry-a',
    3,
    expect.objectContaining({
      started_at: entry.started_at,
      ended_at: '2026-11-01T06:45:00.000Z',
      paused_seconds: 37,
      memo: 'DST edit',
      target: { kind: 'task', id: 'task-a' },
    }),
  ]);
});

it('keeps account A private and ignores its late update after account B opens', async () => {
  let finish: ((value: Awaited<ReturnType<typeof api.updateTimeEntry>>) => void) | undefined;
  const update = vi.spyOn(api, 'updateTimeEntry').mockReturnValue(
    new Promise((resolve) => {
      finish = resolve;
    }),
  );
  const view = render(TimeEntryDialog, props);
  await memo('account A secret');
  await submit();
  await waitFor(() => {
    expect(update).toHaveBeenCalledOnce();
  });
  me.userId = 'user-b';
  await waitFor(() => {
    expect(screen.queryByDisplayValue('account A secret')).toBeNull();
  });
  expect(screen.queryByText(m.common_draft_available())).toBeNull();
  await view.rerender({ ...props, entry: { ...entry, id: 'entry-b', memo: 'B saved' } });
  await memo('account B new edit');
  if (finish === undefined) throw new Error('missing resolver');
  finish({ kind: 'updated', entry });
  await waitFor(() =>
    expect(screen.getByLabelText(m.time_memo())).toHaveValue('account B new edit'),
  );
  expect(props.onclose).not.toHaveBeenCalled();
  expect(props.onchanged).not.toHaveBeenCalled();
  cleanup();
  me.userId = 'user-a';
  render(TimeEntryDialog, props);
  await fireEvent.click(await screen.findByRole('button', { name: m.common_draft_restore() }));
  expect(screen.getByLabelText(m.time_memo())).toHaveValue('account A secret');
});

it('keeps new manual input when an earlier account request completes', async () => {
  let finish: ((value: TimeEntryView) => void) | undefined;
  const add = vi.spyOn(api, 'addTimeEntry').mockReturnValue(
    new Promise((resolve) => {
      finish = resolve;
    }),
  );
  render(TimeScreen);
  await memo('A manual secret');
  await submit();
  await waitFor(() => {
    expect(add).toHaveBeenCalledOnce();
  });
  me.userId = 'user-b';
  await waitFor(() => expect(screen.getByLabelText(m.time_memo())).toHaveValue(''));
  expect(screen.queryByText(m.common_draft_available())).toBeNull();
  await memo('B manual');
  if (finish === undefined) throw new Error('missing resolver');
  finish(entry);
  await waitFor(() => expect(screen.getByLabelText(m.time_memo())).toHaveValue('B manual'));
  cleanup();
  me.userId = 'user-a';
  render(TimeScreen);
  await fireEvent.click(await screen.findByRole('button', { name: m.common_draft_restore() }));
  expect(screen.getByLabelText(m.time_memo())).toHaveValue('A manual secret');
});

it('requires confirmation before applying conflict values and uses their new base only afterwards', async () => {
  const current = { ...entry, memo: 'server value', version: 8 };
  const update = vi.spyOn(api, 'updateTimeEntry').mockResolvedValue({ kind: 'conflict', current });
  render(TimeEntryDialog, props);
  await memo('my value');
  await submit();
  await fireEvent.click(await screen.findByRole('button', { name: m.time_use_current() }));
  expect(screen.getByLabelText(m.time_memo())).toHaveValue('my value');
  await fireEvent.click(screen.getByRole('button', { name: m.common_discard_confirm() }));
  expect(screen.getByLabelText(m.time_memo())).toHaveValue('server value');
  await memo('edit after comparison');
  await submit();
  await waitFor(() => {
    expect(update).toHaveBeenCalledTimes(2);
  });
  expect(update.mock.calls[1]?.[1]).toBe(8);
});

// F-7(R5 B): "서버 내용 적용"은 버리기 확인을 거쳐 폼을 비운 뒤 서버 기록으로 다시 채운다. 비울 때 지운 수정 이력을
// 다시 읽지 않아 이력이 사라졌다.
it('reloads the revision history after applying the server values', async () => {
  const current = { ...entry, memo: 'server value', version: 8 };
  vi.spyOn(api, 'updateTimeEntry').mockResolvedValue({ kind: 'conflict', current });
  const revisions = vi.spyOn(api, 'timeEntryRevisions').mockResolvedValue([
    {
      started_at: entry.started_at,
      ended_at: entry.ended_at,
      kind: 'work',
      memo: 'older memo',
      changed_at: '2026-11-01T07:00:00Z',
    },
  ]);
  render(TimeEntryDialog, props);
  await screen.findByRole('heading', { name: m.time_history_heading() });
  await memo('my value');
  await submit();
  await fireEvent.click(await screen.findByRole('button', { name: m.time_use_current() }));
  await fireEvent.click(screen.getByRole('button', { name: m.common_discard_confirm() }));
  expect(screen.getByLabelText(m.time_memo())).toHaveValue('server value');
  expect(
    await screen.findByRole('heading', { name: m.time_history_heading() }),
  ).toBeInTheDocument();
  expect(revisions).toHaveBeenCalledTimes(2);
});

it('guards route changes and clears the current draft only after approved discard', async () => {
  render(TimeEntryDialog, props);
  await memo('unsaved');
  const leave = vi.fn();
  expect(router.guard?.(leave)).toBe(false);
  await fireEvent.click(await screen.findByRole('button', { name: m.common_discard_confirm() }));
  expect(leave).toHaveBeenCalledOnce();
  cleanup();
  render(TimeEntryDialog, props);
  await screen.findByLabelText(m.time_memo());
  expect(screen.queryByRole('button', { name: m.common_draft_restore() })).toBeNull();
});

it('retains input and leave warnings when storage fails', async () => {
  vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
    throw new Error('quota');
  });
  const update = vi.spyOn(api, 'updateTimeEntry').mockRejectedValue(new NetworkError('offline'));
  render(TimeEntryDialog, props);
  await memo('not durable');
  await submit();
  await screen.findByRole('alert');
  expect(update).toHaveBeenCalledOnce();
  expect(screen.getByLabelText(m.time_memo())).toHaveValue('not durable');
  const event = new Event('beforeunload', { cancelable: true });
  window.dispatchEvent(event);
  expect(event.defaultPrevented).toBe(true);
});

// 확인 필요 2(R5 B): 시간 기록은 조직 인자가 없는 본인 기록이고(D-117) 초안 키는 사용자별·탭별이다(D-27, 공통
// `form-draft.ts`). 조직 A에서 쓰다 만 수동 기록이 조직 B에서 제안되지 않았다. 수동 기록에는 조직 범위 값이 없다.
it('offers an unfinished manual entry in another organization of the same user', async () => {
  render(TimeScreen);
  await memo('manual across organizations');
  cleanup();
  org.load(
    'user-a',
    [TEST_ORGANIZATION, { ...TEST_ORGANIZATION, id: 'org-2', name: '둘째' }],
    'org-2',
  );
  render(TimeScreen);
  await fireEvent.click(await screen.findByRole('button', { name: m.common_draft_restore() }));
  expect(screen.getByLabelText(m.time_memo())).toHaveValue('manual across organizations');
});

it('successful manual save removes only its own draft and retains another tab offer', async () => {
  const name = 'time-manual';
  const key = formDraftTabKey(name);
  if (key === undefined) throw new Error('missing draft key');
  const sibling = `${key}:sibling`;
  writeFormDraftAt(sibling, {
    savedAt: new Date().toISOString(),
    input: {
      form: { day: '2026-10-01', start: '09:00', end: '10:00', kind: 'work', memo: 'other tab' },
      zone: props.zone,
      pending: null,
    },
  });
  vi.spyOn(api, 'addTimeEntry').mockResolvedValue(entry);
  render(TimeScreen);
  await memo('save this');
  await submit();
  await waitFor(() => expect(screen.getByLabelText(m.time_memo())).toHaveValue(''));
  expect(localStorage.getItem(key)).toBeNull();
  expect(localStorage.getItem(sibling)).not.toBeNull();
});

it.each([new NetworkError('offline'), new ApiError(403, 'forbidden', 'forbidden')])(
  'keeps an edit durable after a rejected save (%s)',
  async (failure) => {
    const update = vi.spyOn(api, 'updateTimeEntry').mockRejectedValue(failure);
    render(TimeEntryDialog, props);
    await memo('retry later');
    await submit();
    await screen.findByRole('alert');
    cleanup();
    render(TimeEntryDialog, props);
    await fireEvent.click(await screen.findByRole('button', { name: m.common_draft_restore() }));
    expect(screen.getByLabelText(m.time_memo())).toHaveValue('retry later');
    expect(update).toHaveBeenCalledOnce();
  },
);

it('ignores a late same-account save when the input changed during the request', async () => {
  let finish: ((value: Awaited<ReturnType<typeof api.updateTimeEntry>>) => void) | undefined;
  vi.spyOn(api, 'updateTimeEntry').mockReturnValue(
    new Promise((resolve) => {
      finish = resolve;
    }),
  );
  render(TimeEntryDialog, props);
  await memo('first edit');
  await submit();
  await memo('later edit');
  if (finish === undefined) throw new Error('missing resolver');
  finish({ kind: 'updated', entry: { ...entry, memo: 'first edit', version: 4 } });
  await waitFor(() =>
    expect(screen.getByRole('button', { name: m.common_save() })).not.toBeDisabled(),
  );
  expect(props.onclose).not.toHaveBeenCalled();
  cleanup();
  render(TimeEntryDialog, props);
  await fireEvent.click(await screen.findByRole('button', { name: m.common_draft_restore() }));
  expect(screen.getByLabelText(m.time_memo())).toHaveValue('later edit');
});

it('keeps restored timezone labels and DST conversion together after settings change', async () => {
  const update = vi.spyOn(api, 'updateTimeEntry').mockResolvedValue({ kind: 'updated', entry });
  render(TimeEntryDialog, props);
  await memo('timezone draft');
  await fireEvent.input(screen.getByLabelText(m.time_end()), {
    target: { value: '2026-11-01T01:45' },
  });
  await fireEvent.click(screen.getByRole('radio', { name: '두 번째 01:45 (UTC−05:00)' }));
  cleanup();
  render(TimeEntryDialog, { ...props, zone: 'UTC' });
  await fireEvent.click(await screen.findByRole('button', { name: m.common_draft_restore() }));
  expect(
    screen.getByRole('group', {
      name: m.events_repeated_time_legend({ field: m.time_end(), zone: props.zone }),
    }),
  ).toBeInTheDocument();
  expect(
    screen.getByText(m.time_edit_description({ duration: '2시간 14분', zone: props.zone })),
  ).toBeInTheDocument();
  await submit();
  await waitFor(() => {
    expect(update).toHaveBeenCalledOnce();
  });
  expect(update.mock.calls[0]?.[2].ended_at).toBe('2026-11-01T06:45:00.000Z');
});

it('keeps manual timezone visible and sends the restored wall time in that timezone', async () => {
  const add = vi.spyOn(api, 'addTimeEntry').mockRejectedValue(new NetworkError('offline'));
  render(TimeScreen);
  await memo('timezone manual');
  await fireEvent.input(screen.getByLabelText(m.time_date()), { target: { value: '2026-11-01' } });
  await submit();
  await screen.findByRole('alert');
  const first = add.mock.calls[0];
  cleanup();
  me.timeZone = 'UTC';
  render(TimeScreen);
  await fireEvent.click(await screen.findByRole('button', { name: m.common_draft_restore() }));
  expect(screen.getByText(`${m.settings_time_zone()}: ${props.zone}`)).toBeInTheDocument();
  await submit();
  await waitFor(() => {
    expect(add).toHaveBeenCalledTimes(2);
  });
  expect(add.mock.calls[1]).toEqual(first);
});
