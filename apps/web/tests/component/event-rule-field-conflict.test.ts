import './fixtures/event-user';
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, expect, it, vi } from 'vitest';
import RecurrenceEditor from '$lib/features/events/RecurrenceEditor.svelte';
import type { RecurrenceRule } from '$lib/api/client';
import { ApiError } from '$lib/api/client';

const base: RecurrenceRule = { freq: 'daily', interval: 1, weekdays: [], count: null, until: null };
const changed = { rule: { ...base, interval: 3 } };
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

it('compares a recurrence as one unit immediately, requires a choice and retries with the reviewed rule', async () => {
  const onsave = vi.fn().mockRejectedValueOnce(changed).mockResolvedValueOnce(undefined);
  render(RecurrenceEditor, {
    idPrefix: 'rule',
    rule: base,
    canEdit: true,
    zoneNote: 'UTC',
    draftName: 'event-rule:test',
    onsave,
    conflictRule: (cause: unknown) => (cause === changed ? changed : undefined),
  });
  await fireEvent.input(screen.getByLabelText('간격'), { target: { value: '2' } });
  await fireEvent.click(screen.getByRole('button', { name: '반복 저장' }));
  const panel = await screen.findByRole('region', { name: '바뀐 내용 확인' });
  expect(within(panel).getAllByRole('radio')).toHaveLength(2);
  await fireEvent.click(within(panel).getByRole('button', { name: '선택한 내용으로 저장' }));
  expect(onsave).toHaveBeenCalledOnce();
  await waitFor(() => {
    expect(within(panel).getByRole('radio', { name: '반복: 내 입력 사용' })).toHaveFocus();
  });
  await fireEvent.click(within(panel).getByRole('radio', { name: '반복: 내 입력 사용' }));
  await fireEvent.click(within(panel).getByRole('button', { name: '선택한 내용으로 저장' }));
  await waitFor(() => {
    expect(onsave).toHaveBeenCalledTimes(2);
  });
  expect(onsave.mock.calls[1]).toEqual([{ ...base, interval: 2 }, changed.rule]);
  await waitFor(() => {
    expect(screen.queryByRole('region', { name: '바뀐 내용 확인' })).toBeNull();
  });
});

it('reopens on a second rule conflict and preserves the original rule through a failed candidate', async () => {
  const newer = { rule: { ...base, interval: 5 } };
  const onsave = vi
    .fn()
    .mockRejectedValueOnce(changed)
    .mockRejectedValueOnce(newer)
    .mockRejectedValueOnce(new Error('unavailable'));
  render(RecurrenceEditor, {
    idPrefix: 'rule',
    rule: base,
    canEdit: true,
    zoneNote: 'UTC',
    draftName: 'event-rule:test',
    onsave,
    conflictRule: (cause: unknown) =>
      cause === changed ? changed : cause === newer ? newer : undefined,
  });
  await fireEvent.input(screen.getByLabelText('간격'), { target: { value: '2' } });
  await fireEvent.click(screen.getByRole('button', { name: '반복 저장' }));
  let panel = await screen.findByRole('region', { name: '바뀐 내용 확인' });
  await fireEvent.click(within(panel).getByRole('radio', { name: '반복: 서버 최신 사용' }));
  await fireEvent.click(within(panel).getByRole('button', { name: '선택 적용 후 계속 편집' }));
  expect(screen.getByLabelText('간격')).toHaveValue('3');
  await fireEvent.input(screen.getByLabelText('간격'), { target: { value: '4' } });
  await fireEvent.click(screen.getByRole('button', { name: '반복 저장' }));
  panel = await screen.findByRole('region', { name: '바뀐 내용 확인' });
  expect(within(panel).getByRole('radio', { name: '반복: 내 입력 사용' })).not.toBeChecked();
  await fireEvent.click(within(panel).getByRole('radio', { name: '반복: 내 입력 사용' }));
  await fireEvent.click(within(panel).getByRole('button', { name: '선택한 내용으로 저장' }));
  await waitFor(() => {
    expect(onsave).toHaveBeenCalledTimes(3);
  });
  expect(onsave.mock.calls[2]).toEqual([{ ...base, interval: 4 }, newer.rule]);
  await waitFor(() => {
    expect(within(panel).getByRole('button', { name: '처음 입력 복원' })).toBeEnabled();
  });
  await fireEvent.click(within(panel).getByRole('button', { name: '처음 입력 복원' }));
  expect(screen.getByLabelText('간격')).toHaveValue('2');
});

it('announces an until rejection outside the inert form while reviewing a rule', async () => {
  const original = { ...base, until: '2026-11-03' };
  const current = { rule: { ...base, interval: 3, until: '2026-11-10' } };
  const onsave = vi
    .fn()
    .mockRejectedValueOnce(current)
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
  render(RecurrenceEditor, {
    idPrefix: 'rule',
    rule: original,
    canEdit: true,
    zoneNote: 'UTC',
    draftName: 'event-rule:test',
    onsave,
    conflictRule: (cause: unknown) => (cause === current ? current : undefined),
  });
  await fireEvent.input(screen.getByLabelText('간격'), { target: { value: '2' } });
  await fireEvent.click(screen.getByRole('button', { name: '반복 저장' }));
  const panel = await screen.findByRole('region', { name: '바뀐 내용 확인' });
  await fireEvent.click(within(panel).getByRole('radio', { name: '반복: 내 입력 사용' }));
  await fireEvent.click(within(panel).getByRole('button', { name: '선택한 내용으로 저장' }));
  await waitFor(() => {
    expect(onsave).toHaveBeenCalledTimes(2);
  });
  await waitFor(() => {
    const inert = [...document.querySelectorAll('fieldset')].filter((field) => field.inert);
    expect(
      screen.getAllByRole('alert').some((alert) => !inert.some((field) => field.contains(alert))),
    ).toBe(true);
  });
});
