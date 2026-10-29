import { taskData as work } from '$lib/features/tasks/detail-data';
import type { MembersView, Workspace } from '@fvoci/api-client';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { CourseView, EpisodeView, StudyPlanView, StudyUnitView } from '$lib/api/client';
import { api, NetworkError } from '$lib/api/client';
import { formDraftTabKey, writeFormDraftAt } from '$lib/features/common/form-draft';
import CoursePanel from '$lib/features/courses/CoursePanel.svelte';
import DocumentActions from '$lib/features/documents/DocumentActions.svelte';
import MembersScreen from '$lib/features/members/MembersScreen.svelte';
import StudyPanel from '$lib/features/study/StudyPanel.svelte';
import StatusEditor from '$lib/features/tasks/StatusEditor.svelte';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';
import { ACCESS_DOCUMENT } from '../../stories/fixtures/access';
import { TEST_STATUSES } from './fixtures/task-statuses';

// R5 2차 delta 2묶음 확인 필요 2의 편집 금지 파일 처분: 강의·학습·문서 공유·복제·멤버 초대·그룹·업무 상태 폼도 저장하는
// 동안 입력과 다른 탭 초안의 되살리기·버리기를 잠근다. 그사이 바뀐 스냅숏은 성공을 버리게 해, 만들기는 다시 내면 새
// 요청 id로 하나 더 만들고(복제·회차·CSV·단위), 판이 있는 고치기는 기준 판을 못 옮겨 자기 저장과 409로 부딪친다(회차·단위).
// bits-ui Select(공유 권한·복제 대상·초대 역할)는 조상 fieldset의 disabled를 보지 않으므로 열리지 않는지도 본다.

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  localStorage.clear();
  me.userId = 'user-a';
  Element.prototype.hasPointerCapture = (): boolean => false;
  Element.prototype.setPointerCapture = (): void => undefined;
  Element.prototype.releasePointerCapture = (): void => undefined;
  Element.prototype.scrollIntoView = (): void => undefined;
});
afterEach(() => {
  vi.restoreAllMocks();
  localStorage.clear();
  me.userId = '';
});

function held<T>(): {
  promise: Promise<T>;
  resolve: (value: T) => void;
  reject: (error: unknown) => void;
} {
  let resolve: ((value: T) => void) | undefined;
  let reject: ((error: unknown) => void) | undefined;
  const promise = new Promise<T>((ok, fail) => {
    resolve = ok;
    reject = fail;
  });
  if (resolve === undefined || reject === undefined) throw new Error('promise');
  return { promise, resolve, reject };
}

// D4-F-1: 잠금은 저장이 끝나면 풀려야 한다. 붙잡은 요청을 연결 실패로 풀어(폼이 남고, 해제가 소유자 조건을 거치는
// 경로) 제안·입력이 다시 쓸 수 있는지 본다. 해제 경로를 하나 빠뜨리면 폼이 영구히 잠긴다.
const offline = (): NetworkError => new NetworkError('offline');

/** 같은 사용자의 다른 탭이 남긴 초안(이 탭에는 되살리기 제안으로 보인다). */
function otherTabDraft(name: string, input: unknown): void {
  const own = formDraftTabKey(name, 'user-a');
  if (own === undefined) throw new Error('draft key');
  writeFormDraftAt(own.replace(/:tab:.*$/, ':tab:other-tab'), {
    savedAt: '2026-10-27T00:00:00.000Z',
    input,
  });
}

function offerButtons(count: number): HTMLElement[] {
  const restore = screen.getAllByRole('button', { name: m.common_draft_restore() });
  const discard = screen.getAllByRole('button', { name: m.common_draft_discard() });
  expect(restore).toHaveLength(count);
  expect(discard).toHaveLength(count);
  return [...restore, ...discard];
}

/** 보이는 모든 초안 제안의 되살리기·버리기 단추가 잠겼는지. */
function expectOffersLocked(count: number): void {
  for (const b of offerButtons(count)) expect(b).toBeDisabled();
}

function field(id: string): HTMLElement {
  const el = document.getElementById(id);
  if (el === null) throw new Error(id);
  return el;
}

/** 저장이 끝난 뒤 제안과 대표 입력이 모두 다시 쓸 수 있는지. */
async function expectReleased(count: number, ...inputs: (() => HTMLElement)[]): Promise<void> {
  await waitFor(() => {
    for (const b of offerButtons(count)) expect(b).not.toBeDisabled();
    for (const input of inputs) expect(input()).not.toBeDisabled();
  });
}
const byId =
  (id: string): (() => HTMLElement) =>
  () =>
    field(id);

function formOf(id: string): HTMLFormElement {
  const form = field(id).closest('form');
  if (form === null) throw new Error(`form of ${id}`);
  return form;
}

/** 마우스로 눌러 본다(bits-ui는 pointerdown에서 연다). 열렸는지는 트리거의 `aria-expanded`로 본다. */
async function opens(el: HTMLElement): Promise<boolean> {
  await fireEvent.pointerDown(el, { button: 0, pointerType: 'mouse' });
  await new Promise((r) => setTimeout(r, 30));
  return el.getAttribute('aria-expanded') === 'true';
}

describe('course panel', () => {
  const episode: EpisodeView = {
    id: 'e1',
    number: 1,
    title: '벡터',
    chapter: '1부',
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
  const shared = {
    number: '1',
    title: '다른 탭 제목',
    chapter: '1부',
    duration: '12:30',
    url: '',
    materials: '',
    planned_on: '',
  };

  beforeEach(() => {
    vi.spyOn(api, 'course').mockResolvedValue(course);
    vi.spyOn(api, 'courseParticipants').mockResolvedValue([]);
  });

  it('locks the course details form and its offer while saving', async () => {
    otherTabDraft('course:i1:details', {
      ...course.details,
      subject: '다른 탭',
      starts_on: '',
      expires_on: '',
      target_on: '',
      base: '',
    });
    const save = held<undefined>();
    vi.spyOn(api, 'setCourseDetails').mockReturnValue(save.promise);
    render(CoursePanel, { itemId: 'i1' });
    await fireEvent.click(await screen.findByRole('button', { name: m.courses_edit() }));
    await fireEvent.input(field('course-subject'), { target: { value: '선형대수' } });
    await fireEvent.submit(formOf('course-subject'));
    await waitFor(() => {
      expect(api.setCourseDetails).toHaveBeenCalledTimes(1);
    });
    expectOffersLocked(1);
    expect(field('course-subject')).toBeDisabled();
    save.reject(offline());
    await expectReleased(1, byId('course-subject'));
  });

  it('locks the versioned episode form and its offer while saving', async () => {
    otherTabDraft('course:i1:episode:e1:edit', { ...shared, base: '', version: '1' });
    const save = held<undefined>();
    vi.spyOn(api, 'updateEpisode').mockReturnValue(save.promise);
    render(CoursePanel, { itemId: 'i1' });
    await fireEvent.click(
      await screen.findByRole('button', { name: m.courses_edit_episode_aria({ number: 1 }) }),
    );
    await fireEvent.input(field('ep-title-e1'), { target: { value: '벡터 공간' } });
    await fireEvent.submit(
      screen.getByRole('form', { name: m.courses_episode_form({ number: 1 }) }),
    );
    await waitFor(() => {
      expect(api.updateEpisode).toHaveBeenCalledTimes(1);
    });
    expectOffersLocked(1);
    expect(field('ep-title-e1')).toBeDisabled();
    save.reject(offline());
    await expectReleased(1, byId('ep-title-e1'));
  });

  it('locks my episode notes and their offer while saving', async () => {
    otherTabDraft('course:i1:episode:e1:mine', {
      planned_on: '',
      position: '',
      note: '다른 탭 메모',
      base: '',
    });
    const save = held<undefined>();
    vi.spyOn(api, 'setEpisodeProgress').mockReturnValue(save.promise);
    render(CoursePanel, { itemId: 'i1' });
    await fireEvent.click(
      await screen.findByRole('button', { name: m.courses_notes_aria({ number: 1 }) }),
    );
    await fireEvent.input(field('mine-note-e1'), { target: { value: '메모' } });
    await fireEvent.submit(formOf('mine-note-e1'));
    await waitFor(() => {
      expect(api.setEpisodeProgress).toHaveBeenCalledTimes(1);
    });
    expectOffersLocked(1);
    expect(field('mine-note-e1')).toBeDisabled();
    save.reject(offline());
    await expectReleased(1, byId('mine-note-e1'));
  });

  it('locks the new episode form, the CSV import and both offers while adding', async () => {
    otherTabDraft('course:i1:new', {
      title: '다른 탭 회차',
      chapter: '',
      duration: '10:00',
      planned_on: '',
      requestId: 'req-other',
      requestPayload: '',
    });
    otherTabDraft('course:i1:import', { text: 'First,One,10', sentText: '', ids: [] });
    const add = held<number>();
    vi.spyOn(api, 'addEpisodes').mockReturnValue(add.promise);
    render(CoursePanel, { itemId: 'i1' });
    await fireEvent.input(
      await screen.findByLabelText(m.courses_new_title(), { selector: '#episode-title' }),
      {
        target: { value: '행렬' },
      },
    );
    await fireEvent.input(field('episode-duration'), { target: { value: '12:30' } });
    await fireEvent.submit(formOf('episode-title'));
    await waitFor(() => {
      expect(api.addEpisodes).toHaveBeenCalledTimes(1);
    });
    expectOffersLocked(2);
    expect(field('episode-title')).toBeDisabled();
    expect(field('episodes-csv')).toBeDisabled();
    add.reject(offline());
    await expectReleased(2, byId('episode-title'), byId('episodes-csv'));
  });

  it('locks the CSV text and the new episode form while importing', async () => {
    otherTabDraft('course:i1:import', { text: 'Second,Two,20', sentText: '', ids: [] });
    const add = held<number>();
    vi.spyOn(api, 'addEpisodes').mockReturnValue(add.promise);
    render(CoursePanel, { itemId: 'i1' });
    await screen.findByText('벡터');
    await fireEvent.input(field('episodes-csv'), { target: { value: 'First,One,10' } });
    await fireEvent.click(screen.getByRole('button', { name: m.courses_csv_add() }));
    await waitFor(() => {
      expect(api.addEpisodes).toHaveBeenCalledTimes(1);
    });
    expectOffersLocked(1);
    expect(field('episodes-csv')).toBeDisabled();
    expect(field('episode-title')).toBeDisabled();
    add.reject(offline());
    await expectReleased(1, byId('episodes-csv'), byId('episode-title'));
  });
});

describe('study panel', () => {
  const unit: StudyUnitView = {
    id: 'u1',
    number: 1,
    title: '1장',
    chapter: '기초',
    unit: 'pages',
    amount: 20,
    // 지난 계획일이라 재배치 폼이 보인다.
    planned_on: '2020-01-01',
    mine: { status: 'not_started', done_amount: 0, planned_on: null, note: '', completed_on: null },
    version: 1,
  };
  const plan: StudyPlanView = { can_write: true, units: [unit] };

  beforeEach(() => {
    vi.spyOn(api, 'studyPlan').mockResolvedValue(plan);
    vi.spyOn(api, 'studyParticipants').mockResolvedValue([]);
  });

  it('locks the new unit form and its offer while adding', async () => {
    otherTabDraft('study:i1:new', {
      title: '다른 탭 단위',
      chapter: '',
      unit: 'pages',
      amount: '5',
      planned_on: '',
      requestId: 'req-other',
      requestPayload: '',
    });
    const add = held<number>();
    vi.spyOn(api, 'addStudyUnits').mockReturnValue(add.promise);
    render(StudyPanel, { itemId: 'i1' });
    await screen.findByText('1장');
    await fireEvent.input(field('study-unit-title'), { target: { value: '2장' } });
    await fireEvent.input(field('study-unit-amount'), { target: { value: '30' } });
    await fireEvent.submit(formOf('study-unit-title'));
    await waitFor(() => {
      expect(api.addStudyUnits).toHaveBeenCalledTimes(1);
    });
    expectOffersLocked(1);
    expect(field('study-unit-title')).toBeDisabled();
    expect(field('study-unit-kind')).toBeDisabled();
    add.reject(offline());
    await expectReleased(1, byId('study-unit-title'), byId('study-unit-kind'));
  });

  it('locks the versioned unit form and its offer while saving', async () => {
    otherTabDraft('study:i1:unit:u1:edit', {
      number: '1',
      title: '다른 탭 제목',
      chapter: '기초',
      unit: 'pages',
      amount: '20',
      planned_on: '',
      base: '',
      version: '1',
    });
    const save = held<undefined>();
    vi.spyOn(api, 'updateStudyUnit').mockReturnValue(save.promise);
    render(StudyPanel, { itemId: 'i1' });
    await fireEvent.click(
      await screen.findByRole('button', { name: m.study_edit_aria({ title: '1장' }) }),
    );
    await fireEvent.input(field('unit-title-u1'), { target: { value: '1장 개정' } });
    await fireEvent.submit(screen.getByRole('form', { name: m.study_unit_form({ title: '1장' }) }));
    await waitFor(() => {
      expect(api.updateStudyUnit).toHaveBeenCalledTimes(1);
    });
    expectOffersLocked(1);
    expect(field('unit-title-u1')).toBeDisabled();
    expect(field('unit-kind-u1')).toBeDisabled();
    save.reject(offline());
    await expectReleased(1, byId('unit-title-u1'), byId('unit-kind-u1'));
  });

  it('locks my unit progress and its offer while saving', async () => {
    otherTabDraft('study:i1:unit:u1:mine', {
      done_amount: '3',
      planned_on: '',
      note: '다른 탭 메모',
      base: '',
    });
    const save = held<undefined>();
    vi.spyOn(api, 'setUnitProgress').mockReturnValue(save.promise);
    render(StudyPanel, { itemId: 'i1' });
    await fireEvent.click(await screen.findByRole('button', { name: m.study_mine() }));
    await fireEvent.input(field('unit-note-u1'), { target: { value: '메모' } });
    await fireEvent.submit(formOf('unit-note-u1'));
    await waitFor(() => {
      expect(api.setUnitProgress).toHaveBeenCalledTimes(1);
    });
    expectOffersLocked(1);
    expect(field('unit-note-u1')).toBeDisabled();
    save.reject(offline());
    await expectReleased(1, byId('unit-note-u1'));
  });

  it('locks the replan form and its offer while rescheduling', async () => {
    otherTabDraft('study:i1:replan', { start: '2026-11-01', perDay: '3' });
    const replan = held<number>();
    vi.spyOn(api, 'planStudyUnits').mockReturnValue(replan.promise);
    render(StudyPanel, { itemId: 'i1' });
    await fireEvent.submit(await screen.findByRole('form', { name: m.study_replan_form() }));
    await waitFor(() => {
      expect(api.planStudyUnits).toHaveBeenCalledTimes(1);
    });
    expectOffersLocked(1);
    expect(field('study-replan-per')).toBeDisabled();
    replan.reject(offline());
    await expectReleased(1, byId('study-replan-per'));
  });
});

describe('document actions', () => {
  beforeEach(() => {
    vi.spyOn(api, 'shares').mockResolvedValue([]);
    vi.spyOn(api, 'publicLinks').mockResolvedValue([]);
    vi.spyOn(api, 'document').mockResolvedValue(ACCESS_DOCUMENT);
  });
  const mount = (): void => {
    render(DocumentActions, { doc: ACCESS_DOCUMENT, workspaces: [], onchanged: () => undefined });
  };

  it('locks the share form, its permission select and its offer while sharing', async () => {
    otherTabDraft(`document-share:${ACCESS_DOCUMENT.id}:org-1`, {
      email: 'other@example.test',
      permission: 'write',
    });
    const share = held<never>();
    vi.spyOn(api, 'share').mockReturnValue(share.promise);
    mount();
    await fireEvent.click(screen.getByRole('button', { name: m.documents_share_settings() }));
    await fireEvent.input(await screen.findByLabelText(m.documents_email()), {
      target: { value: 'reader@example.test' },
    });
    await fireEvent.submit(formOf('share-email'));
    await waitFor(() => {
      expect(api.share).toHaveBeenCalledTimes(1);
    });
    expectOffersLocked(1);
    expect(field('share-email')).toBeDisabled();
    const permission = (): HTMLElement =>
      screen.getByRole('button', { name: m.documents_share_permission() });
    expect(await opens(permission())).toBe(false);
    share.reject(offline());
    await expectReleased(1, byId('share-email'), permission);
    expect(await opens(permission())).toBe(true);
  });

  it('locks the copy form, its target select and its offer while copying', async () => {
    otherTabDraft(`document-copy:${ACCESS_DOCUMENT.id}:org-1`, {
      title: '다른 탭 사본',
      target: 'personal',
      baseline: '',
      pending: null,
    });
    const copy = held<typeof ACCESS_DOCUMENT>();
    vi.spyOn(api, 'copyDocument').mockReturnValue(copy.promise);
    mount();
    await fireEvent.click(screen.getByRole('button', { name: m.documents_more_actions() }));
    await fireEvent.click(await screen.findByRole('menuitem', { name: m.documents_copy() }));
    await screen.findByRole('dialog');
    await fireEvent.submit(formOf('copy-title'));
    await waitFor(() => {
      expect(api.copyDocument).toHaveBeenCalledTimes(1);
    });
    expectOffersLocked(1);
    expect(field('copy-title')).toBeDisabled();
    const target = (): HTMLElement => {
      const el = document.querySelector<HTMLElement>('[aria-labelledby="copy-target-label"]');
      if (el === null) throw new Error('copy target');
      return el;
    };
    expect(await opens(target())).toBe(false);
    copy.reject(offline());
    await expectReleased(1, byId('copy-title'), target);
    expect(await opens(target())).toBe(true);
  });
});

describe('members screen', () => {
  const workspace: Workspace = {
    access: 'owner',
    id: 'ws-1',
    organization_id: 'org-1',
    name: '연구실',
    role: 'admin',
    is_owner: true,
  };
  const view: MembersView = {
    can_manage_members: true,
    members: [
      {
        user_id: 'user-a',
        display_name: '관리자',
        email: 'admin@example.test',
        role: 'admin',
        is_owner: true,
      },
    ],
    my_role: 'admin',
    i_am_owner: true,
    pending_owner: null,
  };
  beforeEach(() => {
    vi.spyOn(api, 'members').mockResolvedValue(view);
    vi.spyOn(api, 'groups').mockResolvedValue([]);
    vi.spyOn(api, 'invitations').mockResolvedValue([]);
  });
  const mount = (): void => {
    render(MembersScreen, { workspace, meId: 'user-a', onchanged: () => undefined });
  };

  it('locks the invite form, its role select and its offer while inviting', async () => {
    otherTabDraft('members-invite:ws-1', { email: 'other@example.test', role: 'admin' });
    const invite = held<never>();
    vi.spyOn(api, 'invite').mockReturnValue(invite.promise);
    mount();
    await fireEvent.input(await screen.findByLabelText(m.members_email()), {
      target: { value: 'new@example.test' },
    });
    await fireEvent.submit(formOf('invite-email'));
    await waitFor(() => {
      expect(api.invite).toHaveBeenCalledTimes(1);
    });
    expectOffersLocked(1);
    expect(field('invite-email')).toBeDisabled();
    const role = (): HTMLElement =>
      screen.getByRole('button', { name: new RegExp(`^${m.members_invite_role()}`) });
    expect(await opens(role())).toBe(false);
    invite.reject(offline());
    await expectReleased(1, byId('invite-email'), role);
    expect(await opens(role())).toBe(true);
  });

  it('locks the new group form and its offer while creating', async () => {
    otherTabDraft('members-group:ws-1', '다른 탭 그룹');
    const create = held<never>();
    vi.spyOn(api, 'createGroup').mockReturnValue(create.promise);
    mount();
    await fireEvent.input(await screen.findByLabelText(m.members_new_group_name()), {
      target: { value: '새 그룹' },
    });
    await fireEvent.submit(formOf('group-name'));
    await waitFor(() => {
      expect(api.createGroup).toHaveBeenCalledTimes(1);
    });
    expectOffersLocked(1);
    expect(field('group-name')).toBeDisabled();
    create.reject(offline());
    await expectReleased(1, byId('group-name'));
  });
});

describe('task status editor', () => {
  it('locks the screen-only status form while creating a status', async () => {
    otherTabDraft('task-status:i:new', {
      name: '다른 탭 상태',
      color: '',
      category: 'todo',
      position: String(TEST_STATUSES.length),
    });
    const create = held<never>();
    vi.spyOn(work, 'createTaskStatus').mockReturnValue(create.promise);
    render(StatusEditor, {
      itemId: 'i',
      statuses: TEST_STATUSES,
      archived: false,
      onchanged: vi.fn(),
    });
    await fireEvent.click(screen.getByRole('button', { name: m.tasks_status_add() }));
    await fireEvent.input(screen.getByLabelText(m.tasks_status_name()), {
      target: { value: '공동 검토' },
    });
    await fireEvent.click(screen.getByRole('button', { name: m.common_save() }));
    await waitFor(() => {
      expect(work.createTaskStatus).toHaveBeenCalledTimes(1);
    });
    expect(screen.queryByTestId('form-draft-offer')).toBeNull();
    expect(screen.getByLabelText(m.tasks_status_name())).toBeDisabled();
    create.reject(offline());
    await waitFor(() => {
      expect(screen.getByLabelText(m.tasks_status_name())).not.toBeDisabled();
    });
  });
});
