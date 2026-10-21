import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { CourseView, EpisodeView, StudyPlanView, StudyUnitView } from '$lib/api/client';
import { api } from '$lib/api/client';
import CoursePanel from '$lib/features/courses/CoursePanel.svelte';
import StudyPanel from '$lib/features/study/StudyPanel.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

afterEach(() => {
  vi.restoreAllMocks();
});

const unit: StudyUnitView = {
  id: 'u1',
  number: 1,
  title: '1장',
  chapter: '기초',
  unit: 'pages',
  amount: 20,
  planned_on: null,
  mine: { status: 'not_started', done_amount: 0, planned_on: null, note: '', completed_on: null },
  version: 1,
};

const plan: StudyPlanView = { can_write: true, units: [unit] };

const episode: EpisodeView = {
  id: 'e1',
  number: 1,
  title: '벡터',
  chapter: '',
  duration_seconds: 750,
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
};

const course: CourseView = {
  can_write: true,
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
  episodes: [episode],
};

// 감사 W34: 스터디 단위·강의 회차 지우기는 한 번 누르면 바로 지웠다. 서버는 모든 참여자의 그 단위·회차 진행과
// 개인 메모(쓴 사람만 읽는다)까지 함께 지우고 되돌릴 방법이 없다. 한 번 더 묻고, 그 영향을 알린다.
describe('deleting a study unit', () => {
  it('asks first, warning that every participant progress and note goes too', async () => {
    await setLocale('ko', { reload: false });
    vi.spyOn(api, 'studyPlan').mockResolvedValue(plan);
    vi.spyOn(api, 'studyParticipants').mockResolvedValue([]);
    const remove = vi.spyOn(api, 'deleteStudyUnit').mockResolvedValue(undefined);
    render(StudyPanel, { props: { itemId: 'i1' } });

    await fireEvent.click(await screen.findByRole('button', { name: '지우기' }));
    expect(remove).not.toHaveBeenCalled();
    expect(screen.getByText(/모든 참여자의 이 단위 진행·개인 메모도 함께 지워지고/)).toBeVisible();

    await fireEvent.click(screen.getByRole('button', { name: '취소' }));
    expect(remove).not.toHaveBeenCalled();

    await fireEvent.click(screen.getByRole('button', { name: '지우기' }));
    const confirm = screen.getByRole('button', { name: '지우기' });
    expect(confirm).toHaveFocus();
    await fireEvent.click(confirm);
    await waitFor(() => {
      expect(remove).toHaveBeenCalledWith('u1');
    });
  });
});

describe('deleting a course episode', () => {
  it('asks first, warning that every participant progress and note goes too', async () => {
    await setLocale('ko', { reload: false });
    vi.spyOn(api, 'course').mockResolvedValue(course);
    vi.spyOn(api, 'courseParticipants').mockResolvedValue([]);
    const remove = vi.spyOn(api, 'deleteEpisode').mockResolvedValue(undefined);
    render(CoursePanel, { props: { itemId: 'i1' } });

    await fireEvent.click(await screen.findByRole('button', { name: '1회 지우기' }));
    expect(remove).not.toHaveBeenCalled();
    expect(screen.getByText(/모든 참여자의 이 회차 진도·개인 메모도 함께 지워지고/)).toBeVisible();

    await fireEvent.click(screen.getByRole('button', { name: '취소' }));
    expect(remove).not.toHaveBeenCalled();

    await fireEvent.click(screen.getByRole('button', { name: '1회 지우기' }));
    const confirm = screen.getByRole('button', { name: '1회 지우기' });
    expect(confirm).toHaveFocus();
    await fireEvent.click(confirm);
    await waitFor(() => {
      expect(remove).toHaveBeenCalledWith('e1');
    });
  });
});
