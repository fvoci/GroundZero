import { render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { CourseView, StudyPlanView } from '$lib/api/client';
import { api } from '$lib/api/client';
import CoursePanel from '$lib/features/courses/CoursePanel.svelte';
import StudyPanel from '$lib/features/study/StudyPanel.svelte';
import { me } from '$lib/me.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

// ⑧(D-20 개정 2026-10-26): 읽기 전용은 본인 강의·스터디 진도도 남기지 못한다(서버 `app_set_episode_progress`·
// `app_set_unit_progress`·`app_plan_units`가 42501로 거절). 화면은 서버가 준 `can_write`로 진도 단추·내 메모 폼·
// 다시 배정을 감춘다. 시간 측정은 진도가 아니라 그대로 둔다.

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  me.userId = 'user-a';
  localStorage.clear();
});
afterEach(() => {
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
        planned_on: null,
        completed_on: null,
        watch_count: 0,
        last_position_seconds: null,
        note: '',
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
        done_amount: 0,
        planned_on: null,
        note: '',
        completed_on: null,
      },
      version: 1,
    },
  ],
});

describe('course progress for a read-only member', () => {
  it('hides the progress buttons and my notes but keeps time tracking', async () => {
    vi.spyOn(api, 'course').mockResolvedValue(course(false));
    vi.spyOn(api, 'courseParticipants').mockResolvedValue([]);
    render(CoursePanel, { props: { itemId: 'item-1' } });
    await screen.findByRole('button', { name: '1회 강의 시간 측정' });
    expect(screen.queryByRole('button', { name: '1회 완료' })).toBeNull();
    expect(screen.queryByRole('button', { name: '1회 수강 중' })).toBeNull();
    expect(screen.queryByRole('button', { name: '1회 내 메모·날짜' })).toBeNull();
  });

  it('shows them to a member who can write', async () => {
    vi.spyOn(api, 'course').mockResolvedValue(course(true));
    vi.spyOn(api, 'courseParticipants').mockResolvedValue([]);
    render(CoursePanel, { props: { itemId: 'item-1' } });
    await screen.findByRole('button', { name: '1회 완료' });
    expect(screen.getByRole('button', { name: '1회 수강 중' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '1회 내 메모·날짜' })).toBeInTheDocument();
  });
});

describe('study progress for a read-only member', () => {
  it('hides the status buttons, my amount form and rescheduling but keeps time tracking', async () => {
    vi.spyOn(api, 'studyPlan').mockResolvedValue(plan(false));
    vi.spyOn(api, 'studyParticipants').mockResolvedValue([]);
    render(StudyPanel, { props: { itemId: 'item-1' } });
    await screen.findByRole('button', { name: '1장 공부 시간 측정' });
    expect(screen.queryByRole('button', { name: '완료' })).toBeNull();
    expect(screen.queryByRole('button', { name: '진행 중' })).toBeNull();
    expect(screen.queryByRole('button', { name: '내 분량·날짜·메모' })).toBeNull();
    expect(screen.queryByRole('form', { name: '밀린 단위 다시 배정' })).toBeNull();
  });

  it('shows them to a member who can write', async () => {
    vi.spyOn(api, 'studyPlan').mockResolvedValue(plan(true));
    vi.spyOn(api, 'studyParticipants').mockResolvedValue([]);
    render(StudyPanel, { props: { itemId: 'item-1' } });
    await screen.findByRole('button', { name: '완료' });
    expect(screen.getByRole('button', { name: '내 분량·날짜·메모' })).toBeInTheDocument();
    expect(screen.getByRole('form', { name: '밀린 단위 다시 배정' })).toBeInTheDocument();
  });
});
