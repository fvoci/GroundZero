import { expectOk } from '$lib/shared/api';
import { int, none, parseList, rec, str } from '$lib/shared/api/parse';
import { learningAccess, type EpisodeStatus } from '$lib/features/courses/model';
import type { StudyUnitKind, StudyParticipantView } from './model';

export const studyApi = {
  addStudyUnits: (
    item: string,
    units: {
      request_id: string;
      title: string;
      chapter: string;
      unit: StudyUnitKind;
      amount: number;
      planned_on: string | null;
    }[],
  ) =>
    expectOk(
      'POST',
      `/api/items/${encodeURIComponent(item)}/study/units`,
      (v) => int(rec(v, 'units added'), 'added'),
      { units: units.map((u) => ({ ...u })) },
    ),
  /** 공통 단위 고치기. `version`은 읽은 판이며, 그 사이 다른 사람이 고쳤으면 409(아무것도 바뀌지 않음). */
  updateStudyUnit: (
    id: string,
    version: number,
    unit: {
      number: number;
      title: string;
      chapter: string;
      unit: StudyUnitKind;
      amount: number;
      planned_on: string | null;
    },
  ) => expectOk('PUT', `/api/study-units/${encodeURIComponent(id)}`, none, { version, ...unit }),
  deleteStudyUnit: (id: string) =>
    expectOk('DELETE', `/api/study-units/${encodeURIComponent(id)}`, none),
  setUnitProgress: (
    id: string,
    progress: {
      status: EpisodeStatus;
      done_amount: number;
      planned_on: string | null;
      note: string;
      today: string;
    },
  ) =>
    expectOk('PUT', `/api/study-units/${encodeURIComponent(id)}/progress`, none, { ...progress }),
  planStudyUnits: (item: string, entries: { unit_id: string; planned_on: string | null }[]) =>
    expectOk(
      'PUT',
      `/api/items/${encodeURIComponent(item)}/study/plan`,
      (v) => int(rec(v, 'units planned'), 'planned'),
      { entries: entries.map((e) => ({ ...e })) },
    ),
  studyParticipants: (item: string, today: string) =>
    expectOk(
      'GET',
      `/api/items/${encodeURIComponent(item)}/study/participants?today=${encodeURIComponent(today)}`,
      (v) =>
        parseList(v, (p): StudyParticipantView => {
          const o = rec(p, 'study participant');
          return {
            user_id: str(o, 'user_id'),
            display_name: str(o, 'display_name'),
            done: int(o, 'done'),
            in_progress: int(o, 'in_progress'),
            overdue: int(o, 'overdue'),
            done_amount: int(o, 'done_amount'),
          };
        }),
    ),
};

export const studyResources = {
  info: (item: string) =>
    expectOk('GET', `/api/items/${encodeURIComponent(item)}/study`, (value) => {
      const info = rec(value, 'study');
      return { ...info, access: learningAccess(str(info, 'access')) };
    }),
  units: (item: string) =>
    expectOk('GET', `/api/items/${encodeURIComponent(item)}/study/units`, (value) =>
      parseList(value, (row) => rec(row, 'study unit')),
    ),
  progress: (item: string) =>
    expectOk('GET', `/api/items/${encodeURIComponent(item)}/study/progress`, (value) =>
      parseList(value, (row) => rec(row, 'study progress')),
    ),
};
