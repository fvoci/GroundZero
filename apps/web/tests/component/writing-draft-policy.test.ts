import './fixtures/authenticated';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { api, NetworkError, type ItemView } from '$lib/api/client';
import RecordsPanel from '$lib/features/records/RecordsPanel.svelte';
import CoursePanel from '$lib/features/courses/CoursePanel.svelte';
import StudyPanel from '$lib/features/study/StudyPanel.svelte';
import ZoteroImport from '$lib/features/records/ZoteroImport.svelte';
import ChecklistForm from '$lib/features/checklists/ChecklistForm.svelte';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';
import { router } from '$lib/router.svelte';

const item: ItemView = {
  id: 'draft-item',
  module_id: 'm1',
  workspace_id: 'ws',
  reference: 'PRJ-1',
  title: 'item',
  description: '',
  visibility: 'workspace',
  access: 'write',
  archived: false,
  updated_at: '',
  status: 'active',
  is_template: false,
  version: 1,
  starts_on: null,
  ends_on: null,
};
const course = {
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
  episodes: [
    {
      id: 'e1',
      number: 1,
      title: 'Episode 1',
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
const plan = {
  access: 'write' as const,
  units: [
    {
      id: 'u1',
      number: 1,
      title: 'Unit 1',
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
const stored = () =>
  [localStorage, sessionStorage]
    .flatMap((storage) => Object.keys(storage).map((key) => storage.getItem(key) ?? ''))
    .join('\n');
function field(id: string): HTMLInputElement | HTMLTextAreaElement {
  const value = document.getElementById(id);
  if (value instanceof HTMLInputElement || value instanceof HTMLTextAreaElement) return value;
  throw new Error(`missing ${id}`);
}
const edit = (id: string, value: string) => fireEvent.input(field(id), { target: { value } });
function warns(): boolean {
  const event = new Event('beforeunload', { cancelable: true });
  window.dispatchEvent(event);
  return event.defaultPrevented;
}
const restore = async () =>
  fireEvent.click(await screen.findByRole('button', { name: m.common_draft_restore() }));

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  localStorage.clear();
  me.userId = 'user-a';
  vi.spyOn(api, 'course').mockResolvedValue(course);
  vi.spyOn(api, 'courseParticipants').mockResolvedValue([]);
  vi.spyOn(api, 'studyPlan').mockResolvedValue(plan);
  vi.spyOn(api, 'studyParticipants').mockResolvedValue([]);
  vi.spyOn(api, 'recordKinds').mockResolvedValue([
    {
      kind: 'risk',
      label: '위험',
      module_kind: 'project',
      date_label: null,
      fields: [
        { key: 'notes', label: 'notes', type: 'long_text', choices: [] },
        { key: 'amount', label: 'amount', type: 'number', choices: [] },
      ],
      file_roles: [],
    },
  ]);
  vi.spyOn(api, 'itemRecords').mockResolvedValue([]);
  vi.spyOn(api, 'itemFiles').mockResolvedValue([]);
  vi.spyOn(api, 'members').mockResolvedValue({
    members: [],
    my_role: 'member',
    can_manage_members: false,
    i_am_owner: false,
    pending_owner: null,
  });
});
afterEach(() => {
  cleanup();
  router.guard = undefined;
  vi.restoreAllMocks();
});

it('preserves record writing without short fields or raw retry payloads and retains exact retry identity', async () => {
  const save = vi.spyOn(api, 'createRecord').mockRejectedValue(new NetworkError('offline'));
  render(RecordsPanel, { item, kind: 'project', canWrite: true });
  await fireEvent.click(await screen.findByRole('button', { name: '새 위험' }));
  await edit('record-risk-title', 'SHORT RECORD TITLE');
  await edit('record-risk-amount', '123456');
  await edit('record-risk-body', 'LONG RECORD BODY\nsecond line');
  await edit('record-risk-notes', 'LONG FIELD NOTES');
  await waitFor(() => {
    expect(stored()).toContain('LONG RECORD BODY');
  });
  expect(stored()).not.toContain('SHORT RECORD TITLE');
  expect(stored()).not.toContain('123456');
  await fireEvent.click(screen.getByRole('button', { name: '위험 더하기' }));
  await waitFor(() => {
    expect(save).toHaveBeenCalledOnce();
  });
  await waitFor(() => {
    expect(field('record-risk-title')).not.toBeDisabled();
  });
  const requestId = save.mock.calls[0]?.[1];
  expect(stored()).not.toContain('SHORT RECORD TITLE');
  expect(stored()).not.toContain('123456');
  cleanup();
  render(RecordsPanel, { item, kind: 'project', canWrite: true });
  await restore();
  expect(field('record-risk-title')).toHaveValue('');
  expect(field('record-risk-amount')).toHaveValue('');
  expect(field('record-risk-body')).toHaveValue('LONG RECORD BODY\nsecond line');
  expect(field('record-risk-notes')).toHaveValue('LONG FIELD NOTES');
  await edit('record-risk-title', 'SHORT RECORD TITLE');
  await edit('record-risk-amount', '123456');
  await fireEvent.click(screen.getByRole('button', { name: '위험 더하기' }));
  await waitFor(() => {
    expect(save).toHaveBeenCalledTimes(2);
  });
  expect(save.mock.calls[1]?.[1]).toBe(requestId);
});

it('keeps a short course setting only on screen and guards leaving', async () => {
  render(CoursePanel, { itemId: 'course' });
  await fireEvent.click(await screen.findByRole('button', { name: m.courses_edit() }));
  await edit('course-instructor', 'SHORT INSTRUCTOR');
  expect(warns()).toBe(true);
  expect(stored()).not.toContain('SHORT INSTRUCTOR');
  cleanup();
  render(CoursePanel, { itemId: 'course' });
  await screen.findByText('Episode 1');
  expect(screen.queryByTestId('form-draft-offer')).toBeNull();
});

it('keeps new episode and study unit settings out of durable storage', async () => {
  render(CoursePanel, { itemId: 'course' });
  await screen.findByText('Episode 1');
  await edit('episode-title', 'SHORT NEW EPISODE');
  expect(warns()).toBe(true);
  expect(stored()).not.toContain('SHORT NEW EPISODE');
  cleanup();
  render(StudyPanel, { itemId: 'study' });
  await screen.findByText('Unit 1');
  await edit('study-unit-title', 'SHORT NEW UNIT');
  expect(warns()).toBe(true);
  expect(stored()).not.toContain('SHORT NEW UNIT');
});

it('preserves course materials while short shared episode fields stay on screen', async () => {
  render(CoursePanel, { itemId: 'course' });
  await fireEvent.click(
    await screen.findByRole('button', { name: m.courses_edit_episode_aria({ number: 1 }) }),
  );
  await edit('ep-title-e1', 'SHORT EPISODE TITLE');
  await edit('ep-materials-e1', 'LONG COURSE MATERIALS');
  await waitFor(() => {
    expect(stored()).toContain('LONG COURSE MATERIALS');
  });
  expect(stored()).not.toContain('SHORT EPISODE TITLE');
  cleanup();
  render(CoursePanel, { itemId: 'course' });
  await fireEvent.click(
    await screen.findByRole('button', { name: m.courses_edit_episode_aria({ number: 1 }) }),
  );
  await restore();
  expect(field('ep-title-e1')).toHaveValue('Episode 1');
  expect(field('ep-materials-e1')).toHaveValue('LONG COURSE MATERIALS');
});

it('preserves a course note while its position stays on screen', async () => {
  render(CoursePanel, { itemId: 'course' });
  await fireEvent.click(
    await screen.findByRole('button', { name: m.courses_notes_aria({ number: 1 }) }),
  );
  await edit('mine-note-e1', 'LONG PRIVATE COURSE NOTE');
  await fireEvent.input(screen.getByLabelText(m.courses_last_position()), {
    target: { value: '12:34' },
  });
  await waitFor(() => {
    expect(stored()).toContain('LONG PRIVATE COURSE NOTE');
  });
  expect(stored()).not.toContain('12:34');
  cleanup();
  render(CoursePanel, { itemId: 'course' });
  await fireEvent.click(
    await screen.findByRole('button', { name: m.courses_notes_aria({ number: 1 }) }),
  );
  await restore();
  expect(field('mine-note-e1')).toHaveValue('LONG PRIVATE COURSE NOTE');
  expect(screen.getByLabelText(m.courses_last_position())).toHaveValue('');
});

it('preserves a study note while its amount and replan settings stay on screen', async () => {
  render(StudyPanel, { itemId: 'study' });
  await fireEvent.click(await screen.findByRole('button', { name: m.study_mine() }));
  await edit('unit-note-u1', 'LONG PRIVATE STUDY NOTE');
  await edit('unit-done-u1', '77777');
  await edit('study-replan-start', '2026-11-05');
  await waitFor(() => {
    expect(stored()).toContain('LONG PRIVATE STUDY NOTE');
  });
  expect(stored()).not.toContain('77777');
  expect(stored()).not.toContain('2026-11-05');
  cleanup();
  render(StudyPanel, { itemId: 'study' });
  await fireEvent.click(await screen.findByRole('button', { name: m.study_mine() }));
  await restore();
  expect(field('unit-note-u1')).toHaveValue('LONG PRIVATE STUDY NOTE');
  expect(field('unit-done-u1')).toHaveValue('0');
});

it('keeps course episode settings dirty after restoring materials', async () => {
  render(CoursePanel, { itemId: 'course' });
  await fireEvent.click(
    await screen.findByRole('button', { name: m.courses_edit_episode_aria({ number: 1 }) }),
  );
  await edit('ep-materials-e1', 'RESTORED MATERIALS');
  cleanup();
  render(CoursePanel, { itemId: 'course' });
  await fireEvent.click(
    await screen.findByRole('button', { name: m.courses_edit_episode_aria({ number: 1 }) }),
  );
  await edit('ep-title-e1', 'UNSAVED EPISODE TITLE');
  await edit('ep-planned-e1', '2026-11-17');
  await restore();
  expect(field('ep-title-e1')).toHaveValue('UNSAVED EPISODE TITLE');
  expect(field('ep-planned-e1')).toHaveValue('2026-11-17');
  expect(field('ep-materials-e1')).toHaveValue('RESTORED MATERIALS');
  expect(stored()).not.toContain('UNSAVED EPISODE TITLE');
  expect(stored()).not.toContain('2026-11-17');
  // Once writing returns to its base, only the unsaved settings must still guard leaving.
  await edit('ep-materials-e1', '');
  expect(warns()).toBe(true);
  await edit('ep-title-e1', 'Episode 1');
  expect(warns()).toBe(true);
  await edit('ep-planned-e1', '');
  expect(warns()).toBe(false);
});

it('keeps course personal settings dirty after restoring a note', async () => {
  render(CoursePanel, { itemId: 'course' });
  await fireEvent.click(
    await screen.findByRole('button', { name: m.courses_notes_aria({ number: 1 }) }),
  );
  await edit('mine-note-e1', 'RESTORED COURSE NOTE');
  cleanup();
  render(CoursePanel, { itemId: 'course' });
  await fireEvent.click(
    await screen.findByRole('button', { name: m.courses_notes_aria({ number: 1 }) }),
  );
  await edit('mine-position-e1', '11:22');
  await edit('mine-planned-e1', '2026-11-18');
  await restore();
  expect(field('mine-position-e1')).toHaveValue('11:22');
  expect(field('mine-planned-e1')).toHaveValue('2026-11-18');
  expect(field('mine-note-e1')).toHaveValue('RESTORED COURSE NOTE');
  expect(stored()).not.toContain('11:22');
  expect(stored()).not.toContain('2026-11-18');
  await edit('mine-note-e1', '');
  expect(warns()).toBe(true);
  await edit('mine-position-e1', '');
  expect(warns()).toBe(true);
  await edit('mine-planned-e1', '');
  expect(warns()).toBe(false);
});

it('keeps study personal settings dirty after restoring a note', async () => {
  render(StudyPanel, { itemId: 'study' });
  await fireEvent.click(await screen.findByRole('button', { name: m.study_mine() }));
  await edit('unit-note-u1', 'RESTORED STUDY NOTE');
  cleanup();
  render(StudyPanel, { itemId: 'study' });
  await fireEvent.click(await screen.findByRole('button', { name: m.study_mine() }));
  await edit('unit-done-u1', '17');
  await edit('unit-plan-u1', '2026-11-19');
  await restore();
  expect(field('unit-done-u1')).toHaveValue('17');
  expect(field('unit-plan-u1')).toHaveValue('2026-11-19');
  expect(field('unit-note-u1')).toHaveValue('RESTORED STUDY NOTE');
  expect(stored()).not.toContain('2026-11-19');
  await edit('unit-note-u1', '');
  expect(warns()).toBe(true);
  await edit('unit-done-u1', '0');
  expect(warns()).toBe(true);
  await edit('unit-plan-u1', '');
  expect(warns()).toBe(false);
});

it('keeps Zotero settings and its API key on screen without any durable copy', async () => {
  render(ZoteroImport, { itemId: 'item', onimported: vi.fn() });
  await fireEvent.click(screen.getByRole('button', { name: m.records_zotero_import() }));
  await edit('zotero-id', '987654321');
  await edit('zotero-key', 'synthetic-test-only');
  expect(warns()).toBe(true);
  expect(stored()).not.toContain('987654321');
  expect(stored()).not.toContain('synthetic-test-only');
  cleanup();
  render(ZoteroImport, { itemId: 'item', onimported: vi.fn() });
  expect(screen.queryByTestId('form-draft-offer')).toBeNull();
});

it('keeps shared checklist entry text on screen and reports unsaved input to its parent guard', async () => {
  const unsaved = vi.fn();
  render(ChecklistForm, {
    idPrefix: 'policy',
    items: [],
    canEdit: true,
    onadd: vi.fn().mockResolvedValue(undefined),
    ontoggle: vi.fn().mockResolvedValue(undefined),
    onrename: vi.fn().mockResolvedValue(undefined),
    onremove: vi.fn().mockResolvedValue(undefined),
    onunsaved: unsaved,
  });
  const input = screen.getByRole('textbox');
  await fireEvent.input(input, { target: { value: 'SHORT CHECKLIST ENTRY' } });
  await waitFor(() => {
    expect(unsaved).toHaveBeenLastCalledWith(true);
  });
  expect(stored()).not.toContain('SHORT CHECKLIST ENTRY');
});
