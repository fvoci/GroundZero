import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { api, ApiError, type CourseView, type StudyPlanView } from '$lib/api/client';
import CoursePanel from '$lib/features/courses/CoursePanel.svelte';
import StudyPanel from '$lib/features/study/StudyPanel.svelte';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';
import { router } from '$lib/router.svelte';

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
  episodes: [1, 2].map((number) => ({
    id: `e${String(number)}`,
    number,
    title: `Episode ${String(number)}`,
    chapter: '',
    duration_seconds: 60,
    url: '',
    materials: '',
    planned_on: null,
    version: 1,
    mine: {
      status: 'not_started',
      planned_on: null,
      completed_on: null,
      watch_count: 0,
      last_position_seconds: null,
      note: '',
    },
  })),
};
const plan: StudyPlanView = {
  can_write: true,
  units: [1, 2].map((number) => ({
    id: `u${String(number)}`,
    number,
    title: `Unit ${String(number)}`,
    chapter: '',
    unit: 'pages',
    amount: 10,
    planned_on: null,
    version: 1,
    mine: { status: 'not_started', done_amount: 0, planned_on: null, completed_on: null, note: '' },
  })),
};

beforeEach(async () => {
  localStorage.clear();
  me.userId = 'learner-a';
  await setLocale('en', { reload: false });
  vi.spyOn(api, 'course').mockResolvedValue(course);
  vi.spyOn(api, 'courseParticipants').mockResolvedValue([]);
  vi.spyOn(api, 'studyPlan').mockResolvedValue(plan);
  vi.spyOn(api, 'studyParticipants').mockResolvedValue([]);
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  me.userId = '';
});

async function input(id: string, value: string): Promise<void> {
  const field = document.getElementById(id);
  if (field === null) throw new Error(id);
  await fireEvent.input(field, { target: { value } });
}
function value(id: string): string {
  const field = document.getElementById(id);
  if (!(field instanceof HTMLInputElement || field instanceof HTMLTextAreaElement))
    throw new Error(id);
  return field.value;
}
async function restore(): Promise<void> {
  const offer = await screen.findByTestId('form-draft-offer');
  await fireEvent.click(within(offer).getByRole('button', { name: m.common_draft_restore() }));
}
async function firstButton(name: string): Promise<void> {
  const button = (await screen.findAllByRole('button', { name }))[0];
  if (button === undefined) throw new Error(`Missing button: ${name}`);
  await fireEvent.click(button);
}

describe('durable learning input', () => {
  it('offers a new episode verbatim after remount without sending it', async () => {
    const add = vi.spyOn(api, 'addEpisodes').mockResolvedValue(1);
    const view = render(CoursePanel, { itemId: 'i1' });
    await screen.findByText('Episode 1');
    await input('episode-title', '새 강의 日本語');
    await input('episode-duration', '12:xx');
    view.unmount();
    render(CoursePanel, { itemId: 'i1' });
    await restore();
    expect(value('episode-title')).toBe('새 강의 日本語');
    expect(value('episode-duration')).toBe('12:xx');
    expect(add).not.toHaveBeenCalled();
  });

  it('offers private episode notes only to their owner and confirms episode switches', async () => {
    const save = vi.spyOn(api, 'setEpisodeProgress').mockResolvedValue(undefined);
    const view = render(CoursePanel, { itemId: 'i1' });
    await fireEvent.click(
      await screen.findByRole('button', { name: m.courses_notes_aria({ number: 1 }) }),
    );
    await input('mine-note-e1', 'private A');
    await input('mine-position-e1', 'bad:position');
    await fireEvent.click(
      screen.getByRole('button', { name: m.courses_notes_aria({ number: 2 }) }),
    );
    expect(await screen.findByRole('alertdialog')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: m.common_discard_keep() }));
    view.unmount();
    me.userId = 'learner-b';
    const other = render(CoursePanel, { itemId: 'i1' });
    await screen.findByText('Episode 1');
    expect(screen.queryByTestId('form-draft-offer')).toBeNull();
    expect(document.body).not.toHaveTextContent('private A');
    other.unmount();
    me.userId = 'learner-a';
    render(CoursePanel, { itemId: 'i1' });
    await fireEvent.click(
      await screen.findByRole('button', { name: m.courses_notes_aria({ number: 1 }) }),
    );
    await restore();
    expect(value('mine-note-e1')).toBe('private A');
    expect(value('mine-position-e1')).toBe('bad:position');
    expect(save).not.toHaveBeenCalled();
  });

  it('restores a study creation with invalid numeric text', async () => {
    const view = render(StudyPanel, { itemId: 'i1' });
    await screen.findByText('Unit 1');
    await input('study-unit-title', '계속 읽기');
    await input('study-unit-amount', '12oops');
    view.unmount();
    render(StudyPanel, { itemId: 'i1' });
    await restore();
    expect(value('study-unit-title')).toBe('계속 읽기');
    expect(value('study-unit-amount')).toBe('12oops');
  });

  it('restores private study progress without sending it', async () => {
    const save = vi.spyOn(api, 'setUnitProgress').mockResolvedValue(undefined);
    const view = render(StudyPanel, { itemId: 'i1' });
    await firstButton(m.study_mine());
    await input('unit-note-u1', 'private study');
    await input('unit-done-u1', 'wrong');
    view.unmount();
    render(StudyPanel, { itemId: 'i1' });
    await firstButton(m.study_mine());
    await restore();
    expect(value('unit-note-u1')).toBe('private study');
    expect(value('unit-done-u1')).toBe('wrong');
    await waitFor(() => {
      expect(save).not.toHaveBeenCalled();
    });
  });

  it('preserves course details across a forbidden save and asks before cancelling', async () => {
    const save = vi
      .spyOn(api, 'setCourseDetails')
      .mockRejectedValue(new ApiError(403, 'forbidden', 'denied'));
    const view = render(CoursePanel, { itemId: 'i1' });
    await fireEvent.click(await screen.findByRole('button', { name: m.courses_edit() }));
    await input('course-instructor', '교수 / 日本語');
    await input('course-target_on', '2027-01-10');
    await fireEvent.click(screen.getByRole('button', { name: m.common_save() }));
    await screen.findByRole('alert');
    await fireEvent.click(screen.getByRole('button', { name: m.common_cancel() }));
    expect(await screen.findByRole('alertdialog')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: m.common_discard_keep() }));
    view.unmount();
    render(CoursePanel, { itemId: 'i1' });
    await restore();
    expect(value('course-instructor')).toBe('교수 / 日本語');
    expect(value('course-target_on')).toBe('2027-01-10');
    expect(save).toHaveBeenCalledTimes(1);
  });

  it('restores shared episode fields and its conflict base version', async () => {
    const update = vi
      .spyOn(api, 'updateEpisode')
      .mockRejectedValueOnce(new ApiError(409, 'invalid_state', 'changed'))
      .mockResolvedValue(undefined);
    const view = render(CoursePanel, { itemId: 'i1' });
    await fireEvent.click(
      await screen.findByRole('button', { name: m.courses_edit_episode_aria({ number: 1 }) }),
    );
    await input('ep-title-e1', 'Revised');
    await input('ep-materials-e1', '資料\nhttps://example.org');
    vi.mocked(api.course).mockResolvedValue({
      ...course,
      episodes: course.episodes.map((episode) => ({ ...episode, version: 2 })),
    });
    await fireEvent.submit(
      screen.getByRole('form', { name: m.courses_episode_form({ number: 1 }) }),
    );
    await screen.findByRole('alert');
    view.unmount();
    render(CoursePanel, { itemId: 'i1' });
    await fireEvent.click(
      await screen.findByRole('button', { name: m.courses_edit_episode_aria({ number: 1 }) }),
    );
    await restore();
    expect(value('ep-materials-e1')).toBe('資料\nhttps://example.org');
    expect(update).toHaveBeenCalledTimes(1);
    await fireEvent.submit(
      screen.getByRole('form', { name: m.courses_episode_form({ number: 1 }) }),
    );
    await waitFor(() => {
      expect(update).toHaveBeenCalledTimes(2);
    });
    expect(update.mock.calls[1]?.[1]).toBe(2);
  });

  it('reuses CSV row request IDs after a lost response and explicit restore', async () => {
    const add = vi
      .spyOn(api, 'addEpisodes')
      .mockRejectedValueOnce(new Error('lost response'))
      .mockResolvedValue(2);
    const view = render(CoursePanel, { itemId: 'i1' });
    await screen.findByText('Episode 1');
    await input('episodes-csv', 'First,One,10\nSecond,Two,20');
    await fireEvent.click(screen.getByRole('button', { name: m.courses_csv_add() }));
    await screen.findByRole('alert');
    const ids = add.mock.calls[0]?.[1].map((row) => row.request_id);
    view.unmount();
    render(CoursePanel, { itemId: 'i1' });
    await restore();
    expect(add).toHaveBeenCalledTimes(1);
    await fireEvent.click(screen.getByRole('button', { name: m.courses_csv_add() }));
    await waitFor(() => {
      expect(add).toHaveBeenCalledTimes(2);
    });
    expect(add.mock.calls[1]?.[1].map((row) => row.request_id)).toEqual(ids);
    await waitFor(() => {
      expect(value('episodes-csv')).toBe('');
    });
  });

  it('restores shared study edits and refuses invalid numeric text', async () => {
    const update = vi.spyOn(api, 'updateStudyUnit').mockResolvedValue(undefined);
    const view = render(StudyPanel, { itemId: 'i1' });
    await fireEvent.click(
      await screen.findByRole('button', { name: m.study_edit_aria({ title: 'Unit 1' }) }),
    );
    await input('unit-title-u1', '한국어 unit');
    await input('unit-amount-u1', 'invalid');
    view.unmount();
    render(StudyPanel, { itemId: 'i1' });
    await fireEvent.click(
      await screen.findByRole('button', { name: m.study_edit_aria({ title: 'Unit 1' }) }),
    );
    await restore();
    expect(value('unit-title-u1')).toBe('한국어 unit');
    expect(value('unit-amount-u1')).toBe('invalid');
    await fireEvent.submit(
      screen.getByRole('form', { name: m.study_unit_form({ title: 'Unit 1' }) }),
    );
    await screen.findByRole('alert');
    expect(update).not.toHaveBeenCalled();
  });

  it('keeps study creation request identity through remount and ignores late success after further typing', async () => {
    const add = vi.spyOn(api, 'addStudyUnits').mockRejectedValueOnce(new Error('offline'));
    const view = render(StudyPanel, { itemId: 'i1' });
    await screen.findByText('Unit 1');
    await input('study-unit-title', 'Draft one');
    await fireEvent.click(screen.getByRole('button', { name: m.study_add_unit() }));
    await screen.findByRole('alert');
    const id = add.mock.calls[0]?.[1][0]?.request_id;
    view.unmount();
    render(StudyPanel, { itemId: 'i1' });
    await restore();
    let finish: (count: number) => void = () => undefined;
    add.mockImplementation(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    await fireEvent.click(screen.getByRole('button', { name: m.study_add_unit() }));
    await waitFor(() => {
      expect(add).toHaveBeenCalledTimes(2);
    });
    expect(add.mock.calls[1]?.[1][0]?.request_id).toBe(id);
    await input('study-unit-title', 'Draft two');
    finish(1);
    await screen.findByText(m.study_unit_added());
    expect(value('study-unit-title')).toBe('Draft two');
  });

  it('hides live private study input on account change and restores it for its original owner', async () => {
    const view = render(StudyPanel, { itemId: 'i1' });
    await firstButton(m.study_mine());
    await input('unit-note-u1', 'A only');
    me.userId = 'learner-b';
    await waitFor(() => {
      expect(document.getElementById('unit-note-u1')).toBeNull();
    });
    expect(screen.queryByTestId('form-draft-offer')).toBeNull();
    view.unmount();
    me.userId = 'learner-a';
    render(StudyPanel, { itemId: 'i1' });
    await firstButton(m.study_mine());
    await restore();
    expect(value('unit-note-u1')).toBe('A only');
  });

  it('retains inputs and beforeunload protection when storage is unavailable', async () => {
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new Error('storage denied');
    });
    render(CoursePanel, { itemId: 'i1' });
    await screen.findByText('Episode 1');
    await input('episode-title', 'Memory only');
    const event = new Event('beforeunload', { cancelable: true });
    window.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(true);
    expect(value('episode-title')).toBe('Memory only');
  });

  it('keeps multiple form guards attached and only discards each confirmed form', async () => {
    render(CoursePanel, { itemId: 'i1' });
    await fireEvent.click(await screen.findByRole('button', { name: m.courses_edit() }));
    await input('course-instructor', 'Unsaved instructor');
    await input('episode-title', 'Unsaved episode');
    let navigated = false;
    const guard = router.guard;
    if (guard === undefined) throw new Error('Missing navigation guard');
    expect(
      guard(() => {
        navigated = true;
      }),
    ).toBe(false);
    await screen.findByRole('alertdialog');
    await fireEvent.click(screen.getByRole('button', { name: m.common_discard_keep() }));
    expect(navigated).toBe(false);
    expect(value('episode-title')).toBe('Unsaved episode');
    guard(() => {
      navigated = true;
    });
    await screen.findByRole('alertdialog');
    await fireEvent.click(screen.getByRole('button', { name: m.common_discard_confirm() }));
    expect(navigated).toBe(false);
    await screen.findByRole('alertdialog');
    await fireEvent.click(screen.getByRole('button', { name: m.common_discard_confirm() }));
    await waitFor(() => {
      expect(navigated).toBe(true);
    });
    expect(value('episode-title')).toBe('');
    expect(value('course-instructor')).toBe('');
  });

  it('does not remove the other form when a new episode succeeds', async () => {
    vi.spyOn(api, 'addEpisodes').mockResolvedValue(1);
    const view = render(CoursePanel, { itemId: 'i1' });
    await screen.findByText('Episode 1');
    await input('episode-title', 'Save this');
    await input('episodes-csv', 'Keep this,Chapter,10');
    await fireEvent.click(screen.getByRole('button', { name: m.courses_add_episode() }));
    await waitFor(() => {
      expect(value('episode-title')).toBe('');
    });
    view.unmount();
    render(CoursePanel, { itemId: 'i1' });
    await restore();
    expect(value('episodes-csv')).toBe('Keep this,Chapter,10');
    expect(value('episode-title')).toBe('');
  });

  it('keeps private study edits on a unit switch and refuses writes after permission loss', async () => {
    const save = vi.spyOn(api, 'setUnitProgress').mockResolvedValue(undefined);
    const view = render(StudyPanel, { itemId: 'i1' });
    await firstButton(m.study_mine());
    await input('unit-note-u1', 'Keep my note');
    const second = (await screen.findAllByRole('button', { name: m.study_mine() }))[1];
    if (second === undefined) throw new Error('Missing second unit');
    await fireEvent.click(second);
    await screen.findByRole('alertdialog');
    await fireEvent.click(screen.getByRole('button', { name: m.common_discard_keep() }));
    expect(value('unit-note-u1')).toBe('Keep my note');
    view.unmount();
    vi.mocked(api.studyPlan).mockResolvedValue({ ...plan, can_write: false });
    render(StudyPanel, { itemId: 'i1' });
    await firstButton(m.study_mine());
    await restore();
    await fireEvent.click(screen.getByRole('button', { name: m.common_save() }));
    expect(save).not.toHaveBeenCalled();
    expect(value('unit-note-u1')).toBe('Keep my note');
  });

  it('keeps a remounted study draft when an old instance finishes saving', async () => {
    let finish: (count: number) => void = () => undefined;
    vi.spyOn(api, 'addStudyUnits').mockImplementation(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    const old = render(StudyPanel, { itemId: 'i1' });
    await screen.findByText('Unit 1');
    await input('study-unit-title', 'First attempt');
    await fireEvent.click(screen.getByRole('button', { name: m.study_add_unit() }));
    old.unmount();
    const current = render(StudyPanel, { itemId: 'i1' });
    await restore();
    await input('study-unit-title', 'New instance input');
    finish(1);
    await new Promise((resolve) => setTimeout(resolve, 0));
    current.unmount();
    render(StudyPanel, { itemId: 'i1' });
    await restore();
    expect(value('study-unit-title')).toBe('New instance input');
  });
});
