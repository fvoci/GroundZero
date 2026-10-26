import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { api, ApiError, NetworkError, type EventView } from '$lib/api/client';
import CalendarTaskDialog from '$lib/features/events/CalendarTaskDialog.svelte';
import EventDialog from '$lib/features/events/EventDialog.svelte';
import EventForm from '$lib/features/events/EventForm.svelte';
import RecurrenceEditor from '$lib/features/events/RecurrenceEditor.svelte';
import { router } from '$lib/router.svelte';
import { accountSafety } from '$lib/features/feedback/feedback.svelte';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';

const event: EventView = {
  id: 'draft-event',
  item_id: 'i1',
  workspace_id: 'w1',
  kind: 'meeting',
  title: 'Meeting',
  description: '',
  all_day: true,
  start_date: '2026-10-20',
  end_date: '2026-10-20',
  starts_at: null,
  ends_at: null,
  timezone: 'Asia/Seoul',
  recurrence: {
    freq: 'daily',
    interval: 1,
    weekdays: [],
    until: null,
    count: null,
  },
  excluded_dates: [],
  version: 3,
  access: 'write',
};
beforeEach(async () => {
  localStorage.clear();
  sessionStorage.clear();
  me.userId = 'draft-user';
  await setLocale('ko', { reload: false });
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  me.userId = '';
});
const mountEvent = () =>
  render(EventDialog, { eventId: event.id, onclose: vi.fn(), onchanged: vi.fn() });

it('loading server event fields preserves edited recurrence text and its original base', async () => {
  vi.spyOn(api, 'event').mockResolvedValue(event);
  const latest = {
    ...event,
    version: 4,
    description: 'Server description',
    recurrence: { ...event.recurrence!, interval: 3 },
  };
  vi.spyOn(api, 'updateEvent').mockResolvedValue({ kind: 'conflict', current: latest });
  const saveRule = vi.spyOn(api, 'setEventRecurrence').mockResolvedValue({ ...latest, version: 5 });
  mountEvent();
  const description = await screen.findByLabelText(m.events_description());
  await fireEvent.input(description, { target: { value: 'My description' } });
  await fireEvent.input(screen.getByLabelText(m.events_interval()), { target: { value: '2' } });
  await fireEvent.submit(formOf(description));
  await fireEvent.click(await screen.findByRole('button', { name: m.events_load_server() }));
  expect(screen.getByLabelText(m.events_description())).toHaveValue('Server description');
  expect(screen.getByLabelText(m.events_interval())).toHaveValue('2');
  const unload = new Event('beforeunload', { cancelable: true });
  window.dispatchEvent(unload);
  expect(unload.defaultPrevented).toBe(true);
  await fireEvent.click(screen.getByRole('button', { name: m.events_recurrence_save() }));
  expect(saveRule).toHaveBeenCalledWith(
    event.id,
    { ...event.recurrence, interval: 2 },
    event.recurrence,
  );
});

it.each(['event', 'recurrence'] as const)(
  'cannot load server fields while an %s save is pending',
  async (kind) => {
    vi.spyOn(api, 'event').mockResolvedValue(event);
    const latest = { ...event, version: 4, title: 'Server title' };
    const update = vi
      .spyOn(api, 'updateEvent')
      .mockResolvedValueOnce({ kind: 'conflict', current: latest });
    mountEvent();
    const description = await screen.findByLabelText(m.events_description());
    await fireEvent.input(description, { target: { value: 'My description' } });
    await fireEvent.input(screen.getByLabelText(m.events_interval()), { target: { value: '2' } });
    await fireEvent.submit(formOf(description));
    const load = await screen.findByRole('button', { name: m.events_load_server() });
    let resolve!: () => void;
    const pending = new Promise<void>((yes) => {
      resolve = yes;
    });
    if (kind === 'event') {
      update.mockImplementationOnce(async () => {
        await pending;
        return { kind: 'updated', event: latest };
      });
      await fireEvent.click(screen.getByRole('button', { name: m.events_save_mine() }));
    } else {
      vi.spyOn(api, 'setEventRecurrence').mockImplementationOnce(async () => {
        await pending;
        return { ...latest, recurrence: { ...event.recurrence!, interval: 2 } };
      });
      await fireEvent.click(screen.getByRole('button', { name: m.events_recurrence_save() }));
    }
    expect(load).toBeDisabled();
    await fireEvent.click(load);
    expect(screen.getByLabelText(m.events_description())).toHaveValue('My description');
    expect(screen.getByLabelText(m.events_interval())).toHaveValue('2');
    resolve();
    await waitFor(() => expect(screen.getByLabelText(m.events_description())).not.toBeDisabled());
  },
);

function formOf(field: HTMLElement): HTMLFormElement {
  const form = field.closest('form');
  if (form === null) throw new Error('Missing form');
  return form;
}

it('offers failed calendar creation explicitly after remount and reuses its request id', async () => {
  const create = vi.spyOn(api, 'createTask').mockRejectedValue(new NetworkError('offline'));
  const props = {
    workspaceId: 'w1',
    day: '2026-10-20',
    item: { id: 'i1', title: 'Item' },
    onclose: vi.fn(),
    oncreated: vi.fn(),
  };
  const first = render(CalendarTaskDialog, props);
  await fireEvent.input(await screen.findByLabelText(m.events_task_name()), {
    target: { value: 'Unsaved task' },
  });
  await fireEvent.click(screen.getByRole('button', { name: m.events_task_create() }));
  await screen.findByRole('alert');
  first.unmount();
  render(CalendarTaskDialog, props);
  await fireEvent.click(await screen.findByRole('button', { name: m.common_draft_restore() }));
  expect(screen.getByLabelText(m.events_task_name())).toHaveValue('Unsaved task');
  expect(create).toHaveBeenCalledTimes(1);
  await fireEvent.click(screen.getByRole('button', { name: m.events_task_create() }));
  await waitFor(() => {
    expect(create).toHaveBeenCalledTimes(2);
  });
  expect(create.mock.calls[1]).toEqual(create.mock.calls[0]);
});

it('restores event input and its original version after the server advances', async () => {
  const read = vi.spyOn(api, 'event').mockResolvedValue(event);
  const update = vi
    .spyOn(api, 'updateEvent')
    .mockResolvedValue({ kind: 'conflict', current: { ...event, version: 8 } });
  const first = mountEvent();
  await fireEvent.input(await screen.findByLabelText(m.events_description()), {
    target: { value: 'My description' },
  });
  first.unmount();
  read.mockResolvedValue({ ...event, version: 8 });
  mountEvent();
  await fireEvent.click(await screen.findByRole('button', { name: m.common_draft_restore() }));
  expect(update).not.toHaveBeenCalled();
  const description = screen.getByLabelText(m.events_description());
  expect(description).toHaveValue('My description');
  await fireEvent.submit(formOf(description));
  await waitFor(() => {
    expect(update).toHaveBeenCalled();
  });
  expect(update.mock.calls[0]?.[1]).toBe(3);
});

it('preserves invalid recurrence text and warns for recurrence-only navigation', async () => {
  vi.spyOn(api, 'event').mockResolvedValue(event);
  const first = mountEvent();
  await fireEvent.input(await screen.findByLabelText(m.events_interval()), {
    target: { value: 'not a number' },
  });
  const unload = new Event('beforeunload', { cancelable: true });
  window.dispatchEvent(unload);
  expect(unload.defaultPrevented).toBe(true);
  first.unmount();
  mountEvent();
  await fireEvent.click(await screen.findByRole('button', { name: m.common_draft_restore() }));
  expect(screen.getByLabelText(m.events_interval())).toHaveValue('not a number');
});

function mountNew(onsubmit = vi.fn().mockResolvedValue(undefined)) {
  return render(EventForm, {
    idPrefix: 'new-event-test',
    initial: undefined,
    draftName: 'event-new:i1',
    submitLabel: 'Create test event',
    onsubmit,
  });
}

it.each([new NetworkError('offline'), new ApiError(403, 'forbidden', 'forbidden')])(
  'keeps failed event creation and its request id across remounts (%s)',
  async (failure) => {
    const submit = vi.fn().mockRejectedValue(failure);
    const first = mountNew(submit);
    const title = await screen.findByLabelText(m.events_title());
    await fireEvent.input(title, { target: { value: 'My event' } });
    await fireEvent.input(screen.getByLabelText(m.events_description()), {
      target: { value: 'Private description\n第二行' },
    });
    await fireEvent.submit(formOf(title));
    await screen.findByRole('alert');
    first.unmount();
    mountNew(submit);
    expect(submit).toHaveBeenCalledTimes(1);
    await fireEvent.click(await screen.findByRole('button', { name: m.common_draft_restore() }));
    const restored = screen.getByLabelText(m.events_title());
    expect(restored).toHaveValue('My event');
    expect(screen.getByLabelText(m.events_description())).toHaveValue(
      'Private description\n第二行',
    );
    await fireEvent.submit(formOf(restored));
    await waitFor(() => {
      expect(submit).toHaveBeenCalledTimes(2);
    });
    expect(submit.mock.calls[1]).toEqual(submit.mock.calls[0]);
  },
);

it('keeps one account input private and offers it when that account returns', async () => {
  mountNew();
  await fireEvent.input(await screen.findByLabelText(m.events_title()), {
    target: { value: 'Only user A' },
  });
  me.userId = 'other-user';
  await waitFor(() => expect(screen.getByLabelText(m.events_title())).toHaveValue(''));
  expect(screen.queryByRole('button', { name: m.common_draft_restore() })).toBeNull();
  me.userId = 'draft-user';
  await fireEvent.click(await screen.findByRole('button', { name: m.common_draft_restore() }));
  expect(screen.getByLabelText(m.events_title())).toHaveValue('Only user A');
});

it('keeps storage-unavailable input visible and warns before leaving', async () => {
  vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
    throw new Error('quota');
  });
  mountNew();
  await fireEvent.input(await screen.findByLabelText(m.events_title()), {
    target: { value: 'Keep in memory' },
  });
  const unload = new Event('beforeunload', { cancelable: true });
  window.dispatchEvent(unload);
  expect(unload.defaultPrevented).toBe(true);
  expect(screen.getByLabelText(m.events_title())).toHaveValue('Keep in memory');
  const action = vi.fn();
  expect(router.guard?.(action)).toBe(false);
  await fireEvent.click(await screen.findByRole('button', { name: m.common_discard_keep() }));
  expect(action).not.toHaveBeenCalled();
  expect(screen.getByLabelText(m.events_title())).toHaveValue('Keep in memory');
});

it('discards only this form draft after an explicit navigation decision', async () => {
  localStorage.setItem('unrelated-draft', 'keep');
  mountNew();
  await fireEvent.input(await screen.findByLabelText(m.events_title()), {
    target: { value: 'Discard explicitly' },
  });
  const action = vi.fn();
  expect(router.guard?.(action)).toBe(false);
  await fireEvent.click(await screen.findByRole('button', { name: m.common_discard_confirm() }));
  expect(action).toHaveBeenCalledOnce();
  expect(
    Object.keys(localStorage).filter((key) => key.includes('form-draft:event-new:i1:')),
  ).toHaveLength(0);
  expect(localStorage.getItem('unrelated-draft')).toBe('keep');
});

it('ignores a late successful creation after the user changes', async () => {
  let resolve!: () => void;
  const submit = vi.fn(
    () =>
      new Promise<void>((r) => {
        resolve = r;
      }),
  );
  mountNew(submit);
  const title = await screen.findByLabelText(m.events_title());
  await fireEvent.input(title, { target: { value: 'User A pending' } });
  await fireEvent.submit(formOf(title));
  me.userId = 'other-user';
  await waitFor(() => expect(screen.getByLabelText(m.events_title())).toHaveValue(''));
  await fireEvent.input(screen.getByLabelText(m.events_title()), {
    target: { value: 'User B input' },
  });
  resolve();
  await waitFor(() => expect(screen.getByLabelText(m.events_title())).toHaveValue('User B input'));
  expect(
    Object.keys(localStorage).some(
      (key) => key.includes('fvoci:draft-user:') && key.includes('form-draft:event-new:i1:'),
    ),
  ).toBe(true);
});

it('suppresses account-reload warnings only when the event and recurrence inputs are durably kept', async () => {
  vi.spyOn(api, 'event').mockResolvedValue(event);
  mountEvent();
  await fireEvent.input(await screen.findByLabelText(m.events_interval()), {
    target: { value: 'bad count' },
  });
  accountSafety.switching = true;
  await waitFor(() => {
    const unload = new Event('beforeunload', { cancelable: true });
    window.dispatchEvent(unload);
    expect(unload.defaultPrevented).toBe(false);
  });
});

it('keeps the current event until another-target discard is explicitly approved', async () => {
  vi.spyOn(api, 'event').mockImplementation((id) => Promise.resolve({ ...event, id, title: id }));
  const view = mountEvent();
  await fireEvent.input(await screen.findByLabelText(m.events_title()), {
    target: { value: 'Draft before target change' },
  });
  await view.rerender({ eventId: 'second-event', onclose: vi.fn(), onchanged: vi.fn() });
  await screen.findByRole('button', { name: m.common_discard_confirm() });
  expect(screen.getByLabelText(m.events_title())).toHaveValue('Draft before target change');
  await fireEvent.click(screen.getByRole('button', { name: m.common_discard_confirm() }));
  await waitFor(() => expect(screen.getByLabelText(m.events_title())).toHaveValue('second-event'));
});

it('restores the original recurrence base before an explicit save', async () => {
  const read = vi.spyOn(api, 'event').mockResolvedValue(event);
  const save = vi.spyOn(api, 'setEventRecurrence').mockResolvedValue(event);
  const first = mountEvent();
  await fireEvent.input(await screen.findByLabelText(m.events_interval()), {
    target: { value: '2' },
  });
  first.unmount();
  read.mockResolvedValue({
    ...event,
    recurrence: { freq: 'daily', interval: 5, weekdays: [], until: null, count: null },
  });
  mountEvent();
  await fireEvent.click(await screen.findByRole('button', { name: m.common_draft_restore() }));
  expect(save).not.toHaveBeenCalled();
  await fireEvent.click(screen.getByRole('button', { name: m.events_recurrence_save() }));
  await waitFor(() => {
    expect(save).toHaveBeenCalledOnce();
  });
  expect(save.mock.calls[0]?.[2]).toEqual(event.recurrence);
  expect(save.mock.calls[0]?.[1]?.interval).toBe(2);
});

it('clears a successful creation without clearing the other tab draft', async () => {
  sessionStorage.setItem('fvoci:tab', 'first-tab');
  const first = mountNew();
  await fireEvent.input(await screen.findByLabelText(m.events_title()), {
    target: { value: 'First tab text' },
  });
  first.unmount();
  const firstKey = Object.keys(localStorage).find((key) => key.endsWith(':tab:first-tab'));
  if (firstKey === undefined) throw new Error('First tab was not preserved');
  const firstValue = localStorage.getItem(firstKey);
  sessionStorage.setItem('fvoci:tab', 'second-tab');
  mountNew();
  const title = await screen.findByLabelText(m.events_title());
  await fireEvent.input(title, { target: { value: 'Second tab event' } });
  await fireEvent.submit(formOf(title));
  await waitFor(() => {
    expect(screen.getByLabelText(m.events_title())).toHaveValue('');
  });
  expect(localStorage.getItem(firstKey)).toBe(firstValue);
  expect(Object.keys(localStorage).some((key) => key.endsWith(':tab:second-tab'))).toBe(false);
});

it('does not let an old recurrence response unlock a new account save', async () => {
  const answers: (() => void)[] = [];
  const save = vi.fn(
    () =>
      new Promise<void>((resolve) => {
        answers.push(resolve);
      }),
  );
  const saving = vi.fn();
  render(RecurrenceEditor, {
    idPrefix: 'test-rule',
    draftName: 'test-rule:task-1',
    rule: event.recurrence,
    canEdit: true,
    zoneNote: 'UTC',
    onsave: save,
    onsaving: saving,
  });
  await fireEvent.input(await screen.findByLabelText(m.events_interval()), {
    target: { value: '2' },
  });
  await fireEvent.click(screen.getByRole('button', { name: m.events_recurrence_save() }));
  me.userId = 'other-user';
  await waitFor(() => {
    expect(screen.getByLabelText(m.events_interval())).toHaveValue('1');
  });
  await fireEvent.input(screen.getByLabelText(m.events_interval()), { target: { value: '3' } });
  await fireEvent.click(screen.getByRole('button', { name: m.events_recurrence_save() }));
  answers[0]?.();
  await new Promise((resolve) => setTimeout(resolve, 0));
  expect(screen.getByRole('button', { name: m.events_recurrence_save() })).toBeDisabled();
  expect(saving).toHaveBeenLastCalledWith(true);
  answers[1]?.();
  await waitFor(() => {
    expect(screen.getByRole('button', { name: m.events_recurrence_save() })).not.toBeDisabled();
  });
});

it('explicit parent discard removes the only restored copy when storage writes fail', async () => {
  vi.spyOn(api, 'event').mockResolvedValue(event);
  const first = mountEvent();
  await fireEvent.input(await screen.findByLabelText(m.events_interval()), {
    target: { value: 'unfinished' },
  });
  first.unmount();
  vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
    throw new Error('quota');
  });
  mountEvent();
  await fireEvent.click(await screen.findByRole('button', { name: m.common_draft_restore() }));
  expect(screen.getByLabelText(m.events_interval())).toHaveValue('unfinished');
  const action = vi.fn();
  expect(router.guard?.(action)).toBe(false);
  await fireEvent.click(await screen.findByRole('button', { name: m.common_discard_confirm() }));
  expect(action).toHaveBeenCalledOnce();
  expect(
    Object.keys(localStorage).filter((key) => key.includes('form-draft:event-recurrence:')),
  ).toHaveLength(0);
});
