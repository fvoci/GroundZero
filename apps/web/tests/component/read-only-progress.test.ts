import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { CourseView, StudyPlanView } from '$lib/api/client';
import { api } from '$lib/api/client';
import { formatDay } from '$lib/display';
import CoursePanel from '$lib/features/courses/CoursePanel.svelte';
import StudyPanel from '$lib/features/study/StudyPanel.svelte';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';
import { router } from '$lib/router.svelte';

// ⑧(D-20 개정 2026-10-26): 읽기 전용은 본인 강의·스터디 진도도 남기지 못한다(서버 `app_set_episode_progress`·
// `app_set_unit_progress`·`app_plan_units`가 42501로 거절). 화면은 서버가 준 `can_write`로 진도 단추와 다시 배정을
// 감춘다. 시간 측정은 진도가 아니라 그대로 둔다. 결정은 남기기만 막고 읽기는 막지 않으므로 내 메모·위치·한 분량은 쓸 수
// 없을 때도(읽기 전용, 보관 항목의 모든 멤버: `can_write` = `!archived && access in (owner, write)`) 저장 단추 없는
// 읽기 전용(readonly) 칸으로 보인다(R5 delta3 B D3B-F-2, D5-F-3). 쓸 수 없게 되면 그 칸은 고친 것으로 치지 않고(이탈 확인
// 없음) 남은 초안은 지우지 않는다(확인 필요 2).

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  me.userId = 'user-a';
  localStorage.clear();
});
afterEach(() => {
  cleanup();
  router.guard = undefined;
  router.navigate('/');
  me.userId = '';
  vi.restoreAllMocks();
});

const course = (canWrite: boolean): CourseView => ({
  details: {
    subject: '',
    instructor: '',
    platform: '',
    url: '',
    textbook: '',
    starts_on: null,
    expires_on: null,
    target_on: null,
  },
  episodes: [
    {
      id: 'ep-1',
      number: 1,
      title: '1강',
      chapter: '',
      duration_seconds: 600,
      url: '',
      materials: '',
      planned_on: null,
      mine: {
        status: 'not_started',
        planned_on: '2026-11-02',
        completed_on: null,
        watch_count: 0,
        last_position_seconds: 90,
        note: '내 메모',
      },
      version: 1,
    },
  ],
  can_write: canWrite,
});

const plan = (canWrite: boolean): StudyPlanView => ({
  can_write: canWrite,
  units: [
    {
      id: 'u1',
      number: 1,
      title: '1장',
      chapter: '',
      unit: 'pages',
      amount: 20,
      // 지난 계획일이라 밀린 단위가 되어 다시 배정 폼이 보일 자리다.
      planned_on: '2020-01-01',
      mine: {
        status: 'not_started',
        done_amount: 7,
        planned_on: '2020-01-02',
        note: '스터디 메모',
        completed_on: null,
      },
      version: 1,
    },
  ],
});

/**
 * 읽으라고 보이는 칸: 비활성(흐림·탭 순서 밖·고르기 불가)이 아니라 네이티브 readonly이고 초점을 받는다(R5 delta5 D5-F-3).
 */
function expectReadable(field: HTMLElement): void {
  expect(field).toHaveAttribute('readonly');
  expect(field).toBeEnabled();
  field.focus();
  expect(field).toHaveFocus();
}

const draftKeys = (): string[] =>
  Object.keys(localStorage).filter((k) => localStorage.getItem(k)?.includes('바뀐 메모') === true);

describe('course progress without write access (read-only member or archived item)', () => {
  it('hides the progress buttons but shows my note and position read-only', async () => {
    vi.spyOn(api, 'course').mockResolvedValue(course(false));
    vi.spyOn(api, 'courseParticipants').mockResolvedValue([]);
    render(CoursePanel, { props: { itemId: 'item-1' } });
    await screen.findByRole('button', { name: '1회 강의 시간 측정' });
    expect(screen.queryByRole('button', { name: '1회 완료' })).toBeNull();
    expect(screen.queryByRole('button', { name: '1회 수강 중' })).toBeNull();

    await fireEvent.click(screen.getByRole('button', { name: '1회 내 메모·날짜' }));
    const note = screen.getByLabelText(m.courses_my_note());
    expect(note).toHaveValue('내 메모');
    expectReadable(note);
    const position = screen.getByLabelText(m.courses_last_position());
    expect(position).toHaveValue('0:01:30');
    expectReadable(position);
    // 날짜는 달력이 열리지 않게 글로 보인다.
    const planned = screen.getByLabelText(m.courses_my_planned());
    expect(planned).toHaveValue(formatDay('2026-11-02'));
    expect(planned).not.toHaveAttribute('type', 'date');
    expectReadable(planned);
    expect(screen.queryByRole('button', { name: m.common_save() })).toBeNull();
  });

  it('shows the progress buttons and an editable form to a member who can write', async () => {
    vi.spyOn(api, 'course').mockResolvedValue(course(true));
    vi.spyOn(api, 'courseParticipants').mockResolvedValue([]);
    render(CoursePanel, { props: { itemId: 'item-1' } });
    await screen.findByRole('button', { name: '1회 완료' });
    expect(screen.getByRole('button', { name: '1회 수강 중' })).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: '1회 내 메모·날짜' }));
    expect(screen.getByLabelText(m.courses_my_note())).toBeEnabled();
    expect(screen.getByRole('button', { name: m.common_save() })).toBeInTheDocument();
  });

  it('does not ask to leave after losing write access and keeps the typed draft', async () => {
    window.history.replaceState(null, '', '/w/ws-1/i/item-1');
    router.path = '/w/ws-1/i/item-1';
    vi.spyOn(api, 'course').mockResolvedValueOnce(course(true)).mockResolvedValue(course(false));
    vi.spyOn(api, 'courseParticipants').mockResolvedValue([]);
    vi.spyOn(api, 'setEpisodeProgress').mockResolvedValue(undefined);
    render(CoursePanel, { props: { itemId: 'item-1' } });
    await fireEvent.click(await screen.findByRole('button', { name: '1회 내 메모·날짜' }));
    await fireEvent.input(screen.getByLabelText(m.courses_my_note()), {
      target: { value: '바뀐 메모' },
    });
    // 다른 동작 뒤의 다시 읽기에서 쓸 수 없게 됐다(역할 강등·보관).
    await fireEvent.click(screen.getByRole('button', { name: '1회 완료' }));
    await waitFor(() => {
      expect(screen.queryByRole('button', { name: '1회 완료' })).toBeNull();
    });
    expect(draftKeys()).not.toEqual([]);

    router.navigate('/notes');
    expect(screen.queryByRole('alertdialog')).toBeNull();
    expect(router.path).toBe('/notes');
    expect(draftKeys()).not.toEqual([]);
  });
});

describe('study progress without write access (read-only member or archived item)', () => {
  it('hides status buttons and rescheduling but shows my amount and note read-only', async () => {
    vi.spyOn(api, 'studyPlan').mockResolvedValue(plan(false));
    vi.spyOn(api, 'studyParticipants').mockResolvedValue([]);
    render(StudyPanel, { props: { itemId: 'item-1' } });
    await screen.findByRole('button', { name: '1장 공부 시간 측정' });
    expect(screen.queryByRole('button', { name: '완료' })).toBeNull();
    expect(screen.queryByRole('button', { name: '진행 중' })).toBeNull();
    expect(screen.queryByRole('form', { name: '밀린 단위 다시 배정' })).toBeNull();

    await fireEvent.click(screen.getByRole('button', { name: '내 분량·날짜·메모' }));
    const note = document.getElementById('unit-note-u1');
    const done = document.getElementById('unit-done-u1');
    const planned = document.getElementById('unit-plan-u1');
    if (note === null || done === null || planned === null) throw new Error('my progress fields');
    expect(note).toHaveValue('스터디 메모');
    expectReadable(note);
    expect(done).toHaveValue('7');
    expectReadable(done);
    expect(planned).toHaveValue(formatDay('2020-01-02'));
    expect(planned).not.toHaveAttribute('type', 'date');
    expectReadable(planned);
    expect(screen.queryByRole('button', { name: m.common_save() })).toBeNull();
  });

  it('shows status buttons, rescheduling and an editable form to a member who can write', async () => {
    vi.spyOn(api, 'studyPlan').mockResolvedValue(plan(true));
    vi.spyOn(api, 'studyParticipants').mockResolvedValue([]);
    render(StudyPanel, { props: { itemId: 'item-1' } });
    await screen.findByRole('button', { name: '완료' });
    expect(screen.getByRole('form', { name: '밀린 단위 다시 배정' })).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: '내 분량·날짜·메모' }));
    expect(document.getElementById('unit-note-u1')).toBeEnabled();
    expect(screen.getByRole('button', { name: m.common_save() })).toBeInTheDocument();
  });

  it('does not ask to leave after losing write access and keeps the typed draft', async () => {
    window.history.replaceState(null, '', '/w/ws-1/i/item-1');
    router.path = '/w/ws-1/i/item-1';
    vi.spyOn(api, 'studyPlan').mockResolvedValueOnce(plan(true)).mockResolvedValue(plan(false));
    vi.spyOn(api, 'studyParticipants').mockResolvedValue([]);
    vi.spyOn(api, 'setUnitProgress').mockResolvedValue(undefined);
    render(StudyPanel, { props: { itemId: 'item-1' } });
    await fireEvent.click(await screen.findByRole('button', { name: '내 분량·날짜·메모' }));
    const note = document.getElementById('unit-note-u1');
    if (note === null) throw new Error('note');
    await fireEvent.input(note, { target: { value: '바뀐 메모' } });
    await fireEvent.click(screen.getByRole('button', { name: '완료' }));
    await waitFor(() => {
      expect(screen.queryByRole('button', { name: '완료' })).toBeNull();
    });
    expect(draftKeys()).not.toEqual([]);

    router.navigate('/notes');
    expect(screen.queryByRole('alertdialog')).toBeNull();
    expect(router.path).toBe('/notes');
    expect(draftKeys()).not.toEqual([]);
  });
});
