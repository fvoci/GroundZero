import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { api } from '$lib/api/client';
import CoursePanel from '$lib/features/courses/CoursePanel.svelte';
import StudyPanel from '$lib/features/study/StudyPanel.svelte';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';
import { router } from '$lib/router.svelte';

beforeEach(async () => {
  me.userId = 'reader';
  localStorage.clear();
  await setLocale('ko', { reload: false });
});
afterEach(() => {
  cleanup();
  me.userId = '';
  router.guard = undefined;
  vi.restoreAllMocks();
});

it('lets a read-only learner edit their course note while shared episode creation stays hidden', async () => {
  const course = {
    access: 'read' as const,
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
        id: 'e1',
        number: 1,
        title: '1강',
        chapter: '',
        duration_seconds: 60,
        url: '',
        materials: '',
        planned_on: null,
        version: 1,
        mine: {
          status: 'not_started' as const,
          planned_on: null,
          completed_on: null,
          watch_count: 0,
          last_position_seconds: null,
          note: '',
        },
      },
    ],
  };
  vi.spyOn(api, 'course').mockResolvedValue(course);
  vi.spyOn(api, 'courseParticipants').mockResolvedValue([]);
  const save = vi.spyOn(api, 'setEpisodeProgress').mockResolvedValue(undefined);
  render(CoursePanel, { props: { itemId: 'item' } });
  await fireEvent.click(await screen.findByRole('button', { name: '1회 내 메모·날짜' }));
  const note = screen.getByLabelText(m.courses_my_note());
  expect(note).not.toHaveAttribute('readonly');
  expect(document.getElementById('episode-title')).toBeNull();
  await fireEvent.input(note, { target: { value: 'private note' } });
  await fireEvent.click(screen.getByRole('button', { name: m.common_save() }));
  await waitFor(() =>
    expect(save).toHaveBeenCalledWith('e1', expect.objectContaining({ note: 'private note' })),
  );
});

it('lets a read-only learner edit their study note and replan privately while shared creation stays hidden', async () => {
  const plan = {
    access: 'read' as const,
    units: [
      {
        id: 'u1',
        number: 1,
        title: '1장',
        chapter: '',
        unit: 'pages' as const,
        amount: 10,
        planned_on: '2020-01-01',
        version: 1,
        mine: {
          status: 'not_started' as const,
          done_amount: 0,
          planned_on: null,
          completed_on: null,
          note: '',
        },
      },
    ],
  };
  vi.spyOn(api, 'studyPlan').mockResolvedValue(plan);
  vi.spyOn(api, 'studyParticipants').mockResolvedValue([]);
  const save = vi.spyOn(api, 'setUnitProgress').mockResolvedValue(undefined);
  render(StudyPanel, { props: { itemId: 'item' } });
  await fireEvent.click(await screen.findByRole('button', { name: '내 분량·날짜·메모' }));
  const note = document.getElementById('unit-note-u1');
  if (note === null) throw new Error('missing note');
  expect(note).not.toHaveAttribute('readonly');
  expect(document.getElementById('study-unit-title')).toBeNull();
  expect(screen.getByRole('form', { name: '밀린 단위 다시 배정' })).toBeInTheDocument();
  await fireEvent.input(note, { target: { value: 'private note' } });
  await fireEvent.click(screen.getByRole('button', { name: m.common_save() }));
  await waitFor(() =>
    expect(save).toHaveBeenCalledWith('u1', expect.objectContaining({ note: 'private note' })),
  );
});
