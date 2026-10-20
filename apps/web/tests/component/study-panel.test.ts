import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { StudyPlanView, StudyUnitView } from '$lib/api/client';
import { api, ApiError } from '$lib/api/client';
import StudyPanel from '$lib/features/study/StudyPanel.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

afterEach(() => {
  vi.restoreAllMocks();
});

function unit(over: Partial<StudyUnitView> = {}): StudyUnitView {
  return {
    id: 'u1',
    number: 1,
    title: '1장',
    chapter: '기초',
    unit: 'pages',
    amount: 20,
    planned_on: null,
    mine: { status: 'not_started', done_amount: 0, planned_on: null, note: '', completed_on: null },
    version: 1,
    ...over,
  };
}

const plan = (u: StudyUnitView): StudyPlanView => ({ can_write: true, units: [u] });

function mockPlan(...plans: StudyPlanView[]): void {
  const spy = vi.spyOn(api, 'studyPlan');
  for (const p of plans) spy.mockResolvedValueOnce(p);
  spy.mockResolvedValue(plans.at(-1) ?? plan(unit()));
  vi.spyOn(api, 'studyParticipants').mockResolvedValue([]);
}

const field = (id: string): HTMLInputElement | HTMLTextAreaElement => {
  const el = document.getElementById(id);
  if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement) return el;
  throw new Error(id);
};

// 숫자 칸(type="number")은 숫자나(비우면) null을 준다. 문자열로 다루면 저장 요청이 나가기 전에 멈춘다.
describe('study numbers', () => {
  it('adds a unit with a typed amount, and an emptied amount counts as 0', async () => {
    await setLocale('ko', { reload: false });
    mockPlan(plan(unit()));
    const add = vi.spyOn(api, 'addStudyUnits').mockResolvedValue(1);
    render(StudyPanel, { props: { itemId: 'i1' } });
    await screen.findByText('1장');

    await fireEvent.input(field('study-unit-title'), { target: { value: '2장' } });
    await fireEvent.input(field('study-unit-amount'), { target: { value: '30' } });
    await fireEvent.click(screen.getByRole('button', { name: '단위 추가' }));
    await waitFor(() => {
      expect(add).toHaveBeenCalledTimes(1);
    });
    expect(add.mock.calls[0]?.[1][0]).toMatchObject({ title: '2장', amount: 30 });

    await fireEvent.input(field('study-unit-title'), { target: { value: '3장' } });
    await fireEvent.input(field('study-unit-amount'), { target: { value: '5' } });
    await fireEvent.input(field('study-unit-amount'), { target: { value: '' } });
    await fireEvent.click(screen.getByRole('button', { name: '단위 추가' }));
    await waitFor(() => {
      expect(add).toHaveBeenCalledTimes(2);
    });
    expect(add.mock.calls[1]?.[1][0]).toMatchObject({ title: '3장', amount: 0 });
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('saves my done amount with a note, and 0 or an emptied amount as 0', async () => {
    await setLocale('ko', { reload: false });
    mockPlan(plan(unit()));
    const save = vi.spyOn(api, 'setUnitProgress').mockResolvedValue(undefined);
    render(StudyPanel, { props: { itemId: 'i1' } });
    await fireEvent.click(await screen.findByRole('button', { name: '내 분량·날짜·메모' }));

    await fireEvent.input(field('unit-done-u1'), { target: { value: '12' } });
    await fireEvent.input(field('unit-note-u1'), { target: { value: '어려움' } });
    await fireEvent.click(screen.getByRole('button', { name: '저장' }));
    await waitFor(() => {
      expect(save).toHaveBeenCalledTimes(1);
    });
    expect(save.mock.calls[0]?.[1]).toMatchObject({ done_amount: 12, note: '어려움' });

    await fireEvent.input(field('unit-done-u1'), { target: { value: '0' } });
    await fireEvent.click(screen.getByRole('button', { name: '저장' }));
    await waitFor(() => {
      expect(save).toHaveBeenCalledTimes(2);
    });
    expect(save.mock.calls[1]?.[1]).toMatchObject({ done_amount: 0 });

    await fireEvent.input(field('unit-done-u1'), { target: { value: '' } });
    await fireEvent.click(screen.getByRole('button', { name: '저장' }));
    await waitFor(() => {
      expect(save).toHaveBeenCalledTimes(3);
    });
    expect(save.mock.calls[2]?.[1]).toMatchObject({ done_amount: 0 });
    expect(screen.queryByRole('alert')).toBeNull();
  });
});

// 공통 학습 단위 고치기: PUT /api/study-units/{id}에 읽은 판을 함께 보낸다. 고치기를 연 뒤 다른 사람이 먼저 고쳤으면
// 서버가 409로 막고 화면은 입력을 지킨 채 알리며, 다시 저장하면(명시적으로) 지금 판으로 바꾼다.
describe('editing a shared study unit', () => {
  it('saves the shared fields and stops once when someone else changed the unit first', async () => {
    await setLocale('ko', { reload: false });
    const changed = unit({ title: '1장 (고침)', version: 2 });
    // 처음 그리기(판 1) → 409 뒤 다시 읽기(다른 사람이 고쳐 판 2) → 저장 뒤 다시 읽기.
    mockPlan(plan(unit()), plan(changed), plan(changed));
    const update = vi
      .spyOn(api, 'updateStudyUnit')
      .mockRejectedValueOnce(new ApiError(409, 'invalid_state', 'the unit changed'))
      .mockResolvedValue(undefined);
    render(StudyPanel, { props: { itemId: 'i1' } });
    await fireEvent.click(await screen.findByRole('button', { name: '1장 공통 정보 고치기' }));

    await fireEvent.input(field('unit-amount-u1'), { target: { value: '0' } });
    await fireEvent.input(field('unit-title-u1'), { target: { value: '첫 장' } });
    const form = screen.getByRole('form', { name: '1장 공통 정보' });
    await fireEvent.submit(form);
    expect(await screen.findByRole('alert')).toHaveTextContent('먼저 고쳤습니다');
    expect(update).toHaveBeenCalledTimes(1);
    expect(update.mock.calls[0]?.[1]).toBe(1);
    // 입력은 그대로 남는다.
    expect(field('unit-title-u1').value).toBe('첫 장');

    await fireEvent.submit(form);
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(2);
    });
    expect(update.mock.calls[1]).toEqual([
      'u1',
      2,
      { number: 1, title: '첫 장', chapter: '기초', unit: 'pages', amount: 0, planned_on: null },
    ]);
  });

  // 저장 중에 다시 제출해도 한 번만 보낸다(두 번째가 내 첫 저장 때문에 409를 받아 잘못 알리지 않게).
  it('sends one save when submitted twice while saving', async () => {
    await setLocale('ko', { reload: false });
    mockPlan(plan(unit()));
    let finish: () => void = () => undefined;
    const update = vi.spyOn(api, 'updateStudyUnit').mockImplementation(
      () =>
        new Promise<undefined>((resolve) => {
          finish = () => {
            resolve(undefined);
          };
        }),
    );
    render(StudyPanel, { props: { itemId: 'i1' } });
    await fireEvent.click(await screen.findByRole('button', { name: '1장 공통 정보 고치기' }));
    const form = screen.getByRole('form', { name: '1장 공통 정보' });
    await fireEvent.submit(form);
    await fireEvent.submit(form);
    expect(update).toHaveBeenCalledTimes(1);
    finish();
    await waitFor(() => {
      expect(screen.queryByRole('form', { name: '1장 공통 정보' })).toBeNull();
    });
    expect(screen.queryByRole('alert')).toBeNull();
  });
});
