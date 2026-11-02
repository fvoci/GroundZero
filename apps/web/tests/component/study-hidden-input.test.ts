import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { api, NetworkError, type StudyPlanView } from '$lib/api/client';
import StudyPanel from '$lib/features/study/StudyPanel.svelte';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';
import { router } from '$lib/router.svelte';
import { unloadBlocked } from './fixtures/leave-warning';

// #289: 새 단위는 계획 write일 때, 밀린 단위 재배치는 보관이 아닐 때만 보인다. 다시 읽은 계획에서 그 권한이 빠지면
// 숨은 입력이 떠날 때 경고를 남기면 안 된다. 읽기 전용에도 보이는 재배치·내 메모는 그대로 보호한다(D-27, D-113
// SR1-11).

function plan(access: StudyPlanView['access']): StudyPlanView {
  return {
    access,
    units: [
      {
        id: 'u1',
        number: 1,
        title: '1장',
        chapter: '',
        unit: 'pages',
        amount: 10,
        planned_on: '2020-01-01',
        version: 1,
        mine: {
          status: 'not_started',
          done_amount: 0,
          planned_on: null,
          completed_on: null,
          note: '',
        },
      },
    ],
  };
}

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  me.userId = 'learner';
  localStorage.clear();
  vi.spyOn(api, 'studyPlan').mockResolvedValue(plan('write'));
  vi.spyOn(api, 'studyParticipants').mockResolvedValue([]);
  vi.spyOn(api, 'setUnitProgress').mockResolvedValue(undefined);
});

afterEach(() => {
  cleanup();
  me.userId = '';
  router.guard = undefined;
  vi.restoreAllMocks();
  localStorage.clear();
});

/** 개인 진도 바꾸기로 다시 읽히는 계획이 `access`가 된다. */
async function reloadAs(access: StudyPlanView['access']): Promise<void> {
  vi.mocked(api.studyPlan).mockResolvedValue(plan(access));
  await fireEvent.click(screen.getByRole('button', { name: m.study_status_in_progress() }));
  await waitFor(() => {
    expect(document.getElementById('study-unit-title') === null).toBe(access !== 'write');
  });
}

it('drops the hidden new unit after write access ends while guarding visible replan input', async () => {
  const add = vi.spyOn(api, 'addStudyUnits');
  render(StudyPanel, { props: { itemId: 'item-hidden' } });
  await fireEvent.input(await screen.findByLabelText(m.study_new_title()), {
    target: { value: '숨을 단위' },
  });
  await fireEvent.input(screen.getByLabelText(m.study_per_day()), { target: { value: '3' } });
  await reloadAs('read');
  // 재배치는 읽기 전용에도 보이므로 경고가 남는다.
  expect(screen.getByLabelText(m.study_per_day())).toHaveValue('3');
  expect(unloadBlocked()).toBe(true);
  await fireEvent.input(screen.getByLabelText(m.study_per_day()), { target: { value: '1' } });
  await waitFor(() => {
    expect(unloadBlocked()).toBe(false);
  });
  expect(router.guard?.(vi.fn(), '/')).toBe(true);
  await reloadAs('write');
  expect(screen.getByLabelText(m.study_new_title())).toHaveValue('');
  expect(add).not.toHaveBeenCalled();
});

it('drops hidden replan input after the plan is archived', async () => {
  const replan = vi.spyOn(api, 'planStudyUnits');
  render(StudyPanel, { props: { itemId: 'item-hidden' } });
  await fireEvent.input(await screen.findByLabelText(m.study_per_day()), {
    target: { value: '4' },
  });
  expect(unloadBlocked()).toBe(true);
  await reloadAs('archived');
  expect(screen.queryByLabelText(m.study_per_day())).toBeNull();
  expect(unloadBlocked()).toBe(false);
  expect(router.guard?.(vi.fn(), '/')).toBe(true);
  expect(replan).not.toHaveBeenCalled();
});

it('keeps a sent unit locked until its response, then drops the hidden input', async () => {
  let reject: (e: Error) => void = () => undefined;
  vi.spyOn(api, 'addStudyUnits').mockReturnValue(
    new Promise((_, fail) => {
      reject = fail;
    }),
  );
  render(StudyPanel, { props: { itemId: 'item-hidden' } });
  await fireEvent.input(await screen.findByLabelText(m.study_new_title()), {
    target: { value: '보내는 단위' },
  });
  await fireEvent.input(screen.getByLabelText(m.study_amount()), { target: { value: '5' } });
  await fireEvent.submit(screen.getByLabelText(m.study_new_title()).closest('form') ?? document);
  await reloadAs('read');
  expect(router.guard?.(vi.fn(), '/')).toBe(false);
  reject(new NetworkError('offline'));
  await waitFor(() => {
    expect(unloadBlocked()).toBe(false);
  });
  expect(router.guard?.(vi.fn(), '/')).toBe(true);
  await reloadAs('write');
  expect(screen.getByLabelText(m.study_new_title())).toHaveValue('');
});
