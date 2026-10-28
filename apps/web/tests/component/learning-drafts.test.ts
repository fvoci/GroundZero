import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { tick } from 'svelte';

import { api, ApiError, type CourseView, type StudyPlanView } from '$lib/api/client';
import * as feedback from '$lib/features/feedback/feedback.svelte';
import CoursePanel from '$lib/features/courses/CoursePanel.svelte';
import StudyPanel from '$lib/features/study/StudyPanel.svelte';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';
import { router } from '$lib/router.svelte';

const course: CourseView = {
  access: 'write' as const,
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
  access: 'write' as const,
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

describe('learning pending request ownership', () => {
  const cases = [
    {
      name: 'course details',
      kind: 'course',
      field: 'course-instructor',
      open: () => firstButton(m.courses_edit()),
      mock: (pending: () => Promise<never>) =>
        vi.spyOn(api, 'setCourseDetails').mockImplementation(pending),
    },
    {
      name: 'shared episode',
      kind: 'course',
      field: 'ep-title-e1',
      open: () => firstButton(m.courses_edit_episode_aria({ number: 1 })),
      mock: (pending: () => Promise<never>) =>
        vi.spyOn(api, 'updateEpisode').mockImplementation(pending),
    },
    {
      name: 'private episode',
      kind: 'course',
      field: 'mine-note-e1',
      open: () => firstButton(m.courses_notes_aria({ number: 1 })),
      mock: (pending: () => Promise<never>) =>
        vi.spyOn(api, 'setEpisodeProgress').mockImplementation(pending),
    },
    {
      name: 'new episode',
      kind: 'course',
      field: 'episode-title',
      open: async () => {
        await screen.findByText('Episode 1');
      },
      mock: (pending: () => Promise<never>) =>
        vi.spyOn(api, 'addEpisodes').mockImplementation(pending),
    },
    {
      name: 'episode CSV',
      kind: 'course',
      field: 'episodes-csv',
      open: async () => {
        await screen.findByText('Episode 1');
      },
      mock: (pending: () => Promise<never>) =>
        vi.spyOn(api, 'addEpisodes').mockImplementation(pending),
    },
    {
      name: 'shared study unit',
      kind: 'study',
      field: 'unit-title-u1',
      open: () => firstButton(m.study_edit_aria({ title: 'Unit 1' })),
      mock: (pending: () => Promise<never>) =>
        vi.spyOn(api, 'updateStudyUnit').mockImplementation(pending),
    },
    {
      name: 'private study unit',
      kind: 'study',
      field: 'unit-note-u1',
      open: () => firstButton(m.study_mine()),
      mock: (pending: () => Promise<never>) =>
        vi.spyOn(api, 'setUnitProgress').mockImplementation(pending),
    },
    {
      name: 'new study unit',
      kind: 'study',
      field: 'study-unit-title',
      open: async () => {
        await screen.findByText('Unit 1');
      },
      mock: (pending: () => Promise<never>) =>
        vi.spyOn(api, 'addStudyUnits').mockImplementation(pending),
    },
  ];

  for (const entry of cases) {
    for (const transition of ['account', 'item']) {
      it(`${entry.name} resets busy on ${transition} change and ignores old completion`, async () => {
        const rejects: (() => void)[] = [];
        const pending = (): Promise<never> =>
          new Promise((_resolve, reject) => {
            rejects.push(() => {
              reject(new Error('delayed failure'));
            });
          });
        const request = entry.mock(pending);
        const view =
          entry.kind === 'course'
            ? render(CoursePanel, { itemId: 'i1' })
            : render(StudyPanel, { itemId: 'i1' });
        async function submit(text: string): Promise<void> {
          await input(entry.field, text);
          if (entry.field === 'episodes-csv') {
            await fireEvent.click(screen.getByRole('button', { name: m.courses_csv_add() }));
          } else {
            const form = document.getElementById(entry.field)?.closest('form');
            if (form == null) throw new Error('Missing form');
            await fireEvent.submit(form);
          }
        }
        await entry.open();
        await submit('First');
        await waitFor(() => {
          expect(request).toHaveBeenCalledTimes(1);
        });
        if (transition === 'account') {
          me.userId = 'learner-b';
          await tick();
        } else {
          await view.rerender({ itemId: 'i2' });
        }
        await entry.open();
        await submit('Second');
        await waitFor(() => {
          expect(request).toHaveBeenCalledTimes(2);
        });
        rejects[0]?.();
        await new Promise((resolve) => setTimeout(resolve, 0));
        await submit('Still pending');
        expect(request).toHaveBeenCalledTimes(2);
        rejects[1]?.();
        await new Promise((resolve) => setTimeout(resolve, 0));
        await submit('Current retry');
        await waitFor(() => {
          expect(request).toHaveBeenCalledTimes(3);
        });
        rejects[2]?.();
        await tick();
      });
    }
  }

  it('reports the submitted study title after successful form reset', async () => {
    vi.spyOn(api, 'updateStudyUnit').mockResolvedValue(undefined);
    const done = vi.spyOn(feedback, 'notifyDone');
    render(StudyPanel, { itemId: 'i1' });
    await firstButton(m.study_edit_aria({ title: 'Unit 1' }));
    await input('unit-title-u1', 'Submitted title');
    const form = document.getElementById('unit-title-u1')?.closest('form');
    if (form == null) throw new Error('Missing form');
    await fireEvent.submit(form);
    // 일반 완료는 토스트다(D-82).
    await waitFor(() => {
      expect(done).toHaveBeenCalledWith(m.study_unit_saved({ title: 'Submitted title' }));
    });
  });

  it.each(['course', 'study'])(
    'allows reopening and saving a successful shared %s edit',
    async (kind) => {
      const update =
        kind === 'course'
          ? vi.spyOn(api, 'updateEpisode').mockResolvedValue(undefined)
          : vi.spyOn(api, 'updateStudyUnit').mockResolvedValue(undefined);
      if (kind === 'course') render(CoursePanel, { itemId: 'i1' });
      else render(StudyPanel, { itemId: 'i1' });
      const field = kind === 'course' ? 'ep-title-e1' : 'unit-title-u1';
      const label =
        kind === 'course'
          ? m.courses_edit_episode_aria({ number: 1 })
          : m.study_edit_aria({ title: 'Unit 1' });
      for (const title of ['First revision', 'Second revision']) {
        await firstButton(label);
        await input(field, title);
        const form = document.getElementById(field)?.closest('form');
        if (form == null) throw new Error('Missing form');
        await fireEvent.submit(form);
        await waitFor(() => {
          expect(document.getElementById(field)).toBeNull();
        });
      }
      expect(update).toHaveBeenCalledTimes(2);
    },
  );
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

// D-27: 강좌 정보는 짧은 설정이다. 다시 열 때 영구 초안을 제안하지 않는다.
describe('course details draft after write access is withdrawn', () => {
  it('does not retain short course settings after write access is withdrawn', async () => {
    const save = vi.spyOn(api, 'setCourseDetails');
    const view = render(CoursePanel, { itemId: 'i1' });
    await firstButton(m.courses_edit());
    await input('course-instructor', 'Kept instructor');
    view.unmount();

    vi.mocked(api.course).mockResolvedValue({ ...course, access: 'archived' });
    render(CoursePanel, { itemId: 'i1' });
    await screen.findByText('Episode 1');
    expect(screen.queryByTestId('form-draft-offer')).toBeNull();
    expect(Object.values(localStorage).join('')).not.toContain('Kept instructor');
    expect(save).not.toHaveBeenCalled();
  });
});

describe('durable learning input', () => {
  it('keeps new episode numeric text on screen with a warning and discards it on remount', async () => {
    const add = vi.spyOn(api, 'addEpisodes').mockResolvedValue(1);
    const view = render(CoursePanel, { itemId: 'i1' });
    await screen.findByText('Episode 1');
    await input('episode-title', '새 강의 日本語');
    await input('episode-duration', '12:xx');
    expect(value('episode-duration')).toBe('12:xx');
    const unload = new Event('beforeunload', { cancelable: true });
    window.dispatchEvent(unload);
    expect(unload.defaultPrevented).toBe(true);
    expect(Object.values(localStorage).join('')).not.toContain('새 강의 日本語');
    view.unmount();
    render(CoursePanel, { itemId: 'i1' });
    await screen.findByText('Episode 1');
    expect(screen.queryByTestId('form-draft-offer')).toBeNull();
    expect(value('episode-title')).toBe('');
    expect(value('episode-duration')).toBe('');
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
    expect(value('mine-position-e1')).toBe('');
    expect(Object.values(localStorage).join('')).not.toContain('bad:position');
    expect(save).not.toHaveBeenCalled();
  });

  it('rejects invalid study creation numeric text on screen without storing it', async () => {
    const add = vi.spyOn(api, 'addStudyUnits');
    const view = render(StudyPanel, { itemId: 'i1' });
    await screen.findByText('Unit 1');
    await input('study-unit-title', '계속 읽기');
    await input('study-unit-amount', '12oops');
    await fireEvent.click(screen.getByRole('button', { name: m.study_add_unit() }));
    await screen.findByRole('alert');
    expect(value('study-unit-amount')).toBe('12oops');
    expect(add).not.toHaveBeenCalled();
    expect(Object.values(localStorage).join('')).not.toContain('12oops');
    view.unmount();
    render(StudyPanel, { itemId: 'i1' });
    await screen.findByText('Unit 1');
    expect(screen.queryByTestId('form-draft-offer')).toBeNull();
    expect(value('study-unit-title')).toBe('');
    expect(value('study-unit-amount')).toBe('');
  });

  // D-27: 재배치 설정은 화면 안에서 원문을 유지하고 이탈을 경고한다.
  it('validates and warns about reschedule settings without storing them', async () => {
    vi.mocked(api.studyPlan).mockResolvedValue({
      ...plan,
      units: plan.units.map((u) => ({ ...u, planned_on: '2026-01-02' })),
    });
    const send = vi.spyOn(api, 'planStudyUnits').mockResolvedValue(2);
    const view = render(StudyPanel, { itemId: 'i1' });
    await screen.findByRole('form', { name: m.study_replan_form() });
    await input('study-replan-start', '2026-12-01');
    await input('study-replan-per', '2x');
    const unload = new Event('beforeunload', { cancelable: true });
    window.dispatchEvent(unload);
    expect(unload.defaultPrevented).toBe(true);
    expect(Object.values(localStorage).join('')).not.toContain('2026-12-01');
    expect(value('study-replan-start')).toBe('2026-12-01');
    expect(value('study-replan-per')).toBe('2x');
    await fireEvent.submit(screen.getByRole('form', { name: m.study_replan_form() }));
    expect(await screen.findByRole('alert')).toHaveTextContent(m.study_per_day_invalid());
    expect(send).not.toHaveBeenCalled();
    await input('study-replan-per', '2');
    await fireEvent.submit(screen.getByRole('form', { name: m.study_replan_form() }));
    await waitFor(() => {
      expect(send).toHaveBeenCalledOnce();
    });
    expect(send.mock.calls[0]?.[1][0]?.planned_on).toBe('2026-12-01');
    view.unmount();
    render(StudyPanel, { itemId: 'i1' });
    await screen.findByRole('form', { name: m.study_replan_form() });
    expect(screen.queryByTestId('form-draft-offer')).toBeNull();
    expect(value('study-replan-per')).toBe('1');
  });

  it('restores private study notes without restoring short progress settings or sending them', async () => {
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
    expect(value('unit-done-u1')).toBe('0');
    expect(Object.values(localStorage).join('')).not.toContain('wrong');
    await waitFor(() => {
      expect(save).not.toHaveBeenCalled();
    });
  });

  it('retains course details on screen across a forbidden save and asks before cancelling', async () => {
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
    expect(value('course-instructor')).toBe('교수 / 日本語');
    expect(value('course-target_on')).toBe('2027-01-10');
    expect(Object.values(localStorage).join('')).not.toContain('교수 / 日本語');
    view.unmount();
    render(CoursePanel, { itemId: 'i1' });
    await screen.findByText('Episode 1');
    expect(screen.queryByTestId('form-draft-offer')).toBeNull();
    expect(save).toHaveBeenCalledTimes(1);
  });

  it('restores shared episode materials and its conflict base version without restoring settings', async () => {
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
    expect(value('ep-title-e1')).toBe('Episode 1');
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

  it('refuses invalid shared study numeric text without storing short edits', async () => {
    const update = vi.spyOn(api, 'updateStudyUnit').mockResolvedValue(undefined);
    const view = render(StudyPanel, { itemId: 'i1' });
    await fireEvent.click(
      await screen.findByRole('button', { name: m.study_edit_aria({ title: 'Unit 1' }) }),
    );
    await input('unit-title-u1', '한국어 unit');
    await input('unit-amount-u1', 'invalid');
    expect(value('unit-title-u1')).toBe('한국어 unit');
    expect(value('unit-amount-u1')).toBe('invalid');
    await fireEvent.submit(
      screen.getByRole('form', { name: m.study_unit_form({ title: 'Unit 1' }) }),
    );
    await screen.findByRole('alert');
    expect(update).not.toHaveBeenCalled();
    expect(Object.values(localStorage).join('')).not.toContain('한국어 unit');
    view.unmount();
    render(StudyPanel, { itemId: 'i1' });
    await firstButton(m.study_edit_aria({ title: 'Unit 1' }));
    expect(screen.queryByTestId('form-draft-offer')).toBeNull();
    expect(value('unit-title-u1')).toBe('Unit 1');
    expect(value('unit-amount-u1')).toBe('10');
  });

  it('keeps study creation retry identity on screen and ignores late success after further typing', async () => {
    const add = vi.spyOn(api, 'addStudyUnits').mockRejectedValueOnce(new Error('offline'));
    const done = vi.spyOn(feedback, 'notifyDone');
    render(StudyPanel, { itemId: 'i1' });
    await screen.findByText('Unit 1');
    await input('study-unit-title', 'Draft one');
    await fireEvent.click(screen.getByRole('button', { name: m.study_add_unit() }));
    await screen.findByRole('alert');
    const id = add.mock.calls[0]?.[1][0]?.request_id;
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
    // 일반 완료는 토스트다(D-82).
    await waitFor(() => {
      expect(done).toHaveBeenCalledWith(m.study_unit_added());
    });
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
    // 보관 상태에서는 개인 진도를 읽기만 한다. 읽기 전용 사용자는 개인 진도를 남길 수 있다(D-113 SR1-11).
    vi.mocked(api.studyPlan).mockResolvedValue({ ...plan, access: 'archived' });
    const readOnly = render(StudyPanel, { itemId: 'i1' });
    await firstButton(m.study_mine());
    expect(document.getElementById('unit-note-u1')).toHaveAttribute('readonly');
    expect(screen.queryByRole('button', { name: m.common_draft_restore() })).toBeNull();
    expect(screen.queryByRole('button', { name: m.common_save() })).toBeNull();
    expect(save).not.toHaveBeenCalled();
    readOnly.unmount();
    vi.mocked(api.studyPlan).mockResolvedValue(plan);
    render(StudyPanel, { itemId: 'i1' });
    await firstButton(m.study_mine());
    await restore();
    expect(value('unit-note-u1')).toBe('Keep my note');
  });

  it('keeps a remounted private study note when an old instance finishes saving', async () => {
    let finish: () => void = () => undefined;
    const save = vi.spyOn(api, 'setUnitProgress').mockImplementation(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    const old = render(StudyPanel, { itemId: 'i1' });
    await firstButton(m.study_mine());
    await input('unit-note-u1', 'First attempt');
    const form = document.getElementById('unit-note-u1')?.closest('form');
    if (form == null) throw new Error('Missing form');
    await fireEvent.submit(form);
    await waitFor(() => {
      expect(save).toHaveBeenCalledOnce();
    });
    old.unmount();
    const current = render(StudyPanel, { itemId: 'i1' });
    await firstButton(m.study_mine());
    await restore();
    await input('unit-note-u1', 'New instance input');
    finish();
    await new Promise((resolve) => setTimeout(resolve, 0));
    current.unmount();
    render(StudyPanel, { itemId: 'i1' });
    await firstButton(m.study_mine());
    await restore();
    expect(value('unit-note-u1')).toBe('New instance input');
  });
});
