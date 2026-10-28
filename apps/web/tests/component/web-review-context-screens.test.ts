import { homeApi } from '$lib/features/home/api';
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { api, type TimeEntryView, type TimeSummaryRowView } from '$lib/api/client';
import { displaySettings } from '$lib/display';
import { dayIn, instantOf } from '$lib/features/events/time';
import ReceivedInvitations from '$lib/features/members/ReceivedInvitations.svelte';
import TimeScreen from '$lib/features/time/TimeScreen.svelte';
import HomeScreen from '$lib/features/workspaces/HomeScreen.svelte';
import { me } from '$lib/me.svelte';
import { org } from '$lib/org.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';
import { INVITATION } from '../../stories/fixtures/personal';
import { TEST_ORGANIZATION } from './fixtures/organization';

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: Error) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  me.userId = 'user-a';
  localStorage.clear();
  org.load('user-a', [TEST_ORGANIZATION, { ...TEST_ORGANIZATION, id: 'org-2' }], 'org-1');
  vi.spyOn(api, 'setDisplaySettings').mockResolvedValue(undefined);
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  localStorage.clear();
  me.userId = '';
});

describe('invitation context', () => {
  it('shows the next invitation from the full list while its refresh is pending', async () => {
    const list = [
      INVITATION,
      { ...INVITATION, id: 'invite-b', organization_name: '다음 초대 조직' },
    ];
    const refresh = deferred<typeof list>();
    vi.spyOn(api, 'myInvitations').mockResolvedValueOnce(list).mockReturnValue(refresh.promise);
    const view = render(ReceivedInvitations, {
      invitationId: INVITATION.id,
      onaccepted: () => Promise.resolve(),
    });
    await screen.findByText(INVITATION.organization_name);
    await view.rerender({ invitationId: 'invite-b' });
    expect(screen.queryByText(m.members_invitation_unavailable())).not.toBeInTheDocument();
    expect(screen.getByText('다음 초대 조직')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: m.members_accept() })).toBeEnabled();
    await act(() => {
      refresh.resolve(list);
    });
  });

  it('clears the previous invitation load error when changing the invitation', async () => {
    const refresh = deferred<(typeof INVITATION)[]>();
    vi.spyOn(api, 'myInvitations')
      .mockRejectedValueOnce(new Error('offline'))
      .mockReturnValue(refresh.promise);
    const view = render(ReceivedInvitations, {
      invitationId: INVITATION.id,
      onaccepted: () => Promise.resolve(),
    });
    await screen.findByRole('alert');
    await view.rerender({ invitationId: 'invite-b' });
    expect(screen.queryByRole('alert')).toBeNull();
    expect(screen.getByRole('status')).toHaveTextContent(m.app_loading());
    await act(() => {
      refresh.resolve([]);
    });
    expect(screen.getByRole('status')).toHaveTextContent(m.members_invitation_unavailable());
  });
});

describe('time organization context', () => {
  it('clears previous records, totals and the editor before the new organization responds', async () => {
    const zone = displaySettings().timeZone;
    const today = dayIn(new Date().toISOString(), zone);
    const entry: TimeEntryView = {
      id: 'entry-a',
      started_at: instantOf(`${today}T12:00`, zone),
      ended_at: instantOf(`${today}T13:00`, zone),
      paused_at: null,
      paused_seconds: 0,
      seconds: 3600,
      kind: 'work',
      memo: '앞 조직 메모',
      target: null,
      version: 1,
    };
    const summary: TimeSummaryRowView[] = [{ period: today, kind: 'work', seconds: 3600 }];
    const next = deferred<TimeEntryView[]>();
    const nextSummary = deferred<TimeSummaryRowView[]>();
    vi.spyOn(api, 'currentTimer').mockResolvedValue(null);
    vi.spyOn(api, 'timeEntryRevisions').mockResolvedValue([]);
    const save = vi.spyOn(api, 'updateTimeEntry').mockResolvedValue({ kind: 'updated', entry });
    const entries = vi
      .spyOn(api, 'timeEntries')
      .mockResolvedValueOnce([entry])
      .mockReturnValue(next.promise);
    vi.spyOn(api, 'timeSummary')
      .mockResolvedValueOnce(summary)
      .mockResolvedValueOnce(summary)
      .mockReturnValue(nextSummary.promise);
    render(TimeScreen);
    await fireEvent.click(await screen.findByRole('button', { name: /앞 조직 메모/ }));
    const editor = within(await screen.findByRole('dialog'));
    await fireEvent.input(editor.getByLabelText(m.time_memo()), {
      target: { value: '미저장 시간 메모' },
    });
    expect(screen.getByTestId('time-week')).toHaveTextContent('1시간');
    await act(() => {
      org.select('org-2');
    });
    expect(entries).toHaveBeenCalledTimes(2);
    expect(screen.queryByRole('button', { name: /앞 조직 메모/ })).toBeNull();
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(screen.getByTestId('time-week')).not.toHaveTextContent('1시간');
    expect(screen.queryAllByText('1시간')).toHaveLength(0);
    expect(screen.queryByDisplayValue('미저장 시간 메모')).toBeNull();
    expect(screen.getAllByText(m.app_loading()).length).toBeGreaterThan(0);
    await act(() => {
      next.resolve([]);
      nextSummary.resolve([]);
    });
    expect(screen.getByText(m.time_entries_empty())).toBeInTheDocument();
    entries.mockResolvedValue([{ ...entry, version: 2 }]);
    await act(() => {
      org.select('org-1');
    });
    await fireEvent.click(await screen.findByRole('button', { name: /앞 조직 메모/ }));
    const reopened = within(await screen.findByRole('dialog'));
    expect(reopened.getByLabelText(m.time_memo())).toHaveValue('앞 조직 메모');
    await fireEvent.click(reopened.getByRole('button', { name: m.common_draft_restore() }));
    expect(reopened.getByLabelText(m.time_memo())).toHaveValue('미저장 시간 메모');
    expect(save).not.toHaveBeenCalled();
    await fireEvent.click(reopened.getByRole('button', { name: m.common_save() }));
    expect(save).toHaveBeenCalledWith(
      entry.id,
      1,
      expect.objectContaining({ memo: '미저장 시간 메모' }),
    );
  });

  it('clears old load errors across an organization deselection and ignores late failures', async () => {
    const old = deferred<TimeEntryView[]>();
    vi.spyOn(api, 'currentTimer').mockResolvedValue(null);
    vi.spyOn(api, 'timeEntries')
      .mockRejectedValueOnce(new Error('offline'))
      .mockReturnValueOnce(old.promise)
      .mockResolvedValue([]);
    vi.spyOn(api, 'timeSummary').mockResolvedValue([]);
    render(TimeScreen);
    await screen.findByRole('alert');
    await act(() => {
      org.select('org-2');
    });
    expect(screen.queryByRole('alert')).toBeNull();
    await act(() => {
      org.reset();
    });
    await act(() => {
      old.reject(new Error('late error'));
    });
    expect(screen.getByText(m.app_organization_required())).toBeInTheDocument();
    await act(() => {
      org.load('user-a', [TEST_ORGANIZATION], 'org-1');
    });
    await screen.findByText(m.time_entries_empty());
    expect(screen.queryByRole('alert')).toBeNull();
  });
});

it('isolates workspace request IDs by organization and restores the original retry identity', async () => {
  vi.spyOn(homeApi, 'home').mockReturnValue(new Promise(() => undefined));
  vi.spyOn(homeApi, 'myTasks').mockReturnValue(new Promise(() => undefined));
  vi.spyOn(api, 'recentActivity').mockResolvedValue([]);
  vi.spyOn(api, 'myInvitations').mockResolvedValue([]);
  const create = vi.spyOn(api, 'createWorkspace').mockRejectedValue(new Error('response lost'));
  render(HomeScreen, { workspaces: [], oncreated: vi.fn(), onjoined: () => Promise.resolve() });
  function input(): HTMLElement {
    const box = document.getElementById('new-workspace');
    if (box === null) throw new Error('missing workspace input');
    return box;
  }
  async function submit(): Promise<void> {
    const form = input().closest('form');
    if (form === null) throw new Error('missing workspace form');
    await fireEvent.submit(form);
    await waitFor(() => expect(input()).toBeEnabled());
  }
  await fireEvent.input(input(), { target: { value: 'Same name' } });
  await submit();
  const originalId = create.mock.calls[0]?.[0];
  expect(originalId).toEqual(expect.any(String));
  await act(() => {
    org.select('org-2');
  });
  expect(input()).toHaveValue('');
  await fireEvent.input(input(), { target: { value: 'Same name' } });
  await submit();
  expect(create.mock.calls[1]?.[0]).not.toBe(originalId);
  expect(create.mock.calls[1]?.slice(1)).toEqual(['org-2', 'Same name']);
  await act(() => {
    org.select('org-1');
  });
  expect(input()).toHaveValue('');
  await fireEvent.click(await screen.findByRole('button', { name: m.common_draft_restore() }));
  expect(input()).toHaveValue('Same name');
  expect(create).toHaveBeenCalledTimes(2);
  await submit();
  expect(create.mock.calls[2]).toEqual([originalId, 'org-1', 'Same name']);
});
