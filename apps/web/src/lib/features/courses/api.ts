import { expectOk } from '$lib/shared/api';
import { int, none, parseList, rec, str } from '$lib/shared/api/parse';
import {
  parseParticipant,
  learningAccess,
  type CourseDetailsView,
  type NewEpisodeInput,
  type EpisodeStatus,
} from './model';

export const courseApi = {
  setCourseDetails: (item: string, details: CourseDetailsView) =>
    expectOk('PUT', `/api/items/${encodeURIComponent(item)}/course`, none, { ...details }),
  addEpisodes: (item: string, episodes: NewEpisodeInput[]) =>
    expectOk(
      'POST',
      `/api/items/${encodeURIComponent(item)}/episodes`,
      (v) => int(rec(v, 'episodes added'), 'added'),
      { episodes: episodes.map((e) => ({ ...e })) },
    ),
  /** 회차 공통 정보 고치기. `version`은 읽은 판이며, 그 사이 다른 사람이 고쳤으면 409(아무것도 바뀌지 않음). */
  updateEpisode: (
    id: string,
    version: number,
    episode: Omit<NewEpisodeInput, 'request_id'> & { number: number },
  ) => expectOk('PUT', `/api/episodes/${encodeURIComponent(id)}`, none, { version, ...episode }),
  deleteEpisode: (id: string) =>
    expectOk('DELETE', `/api/episodes/${encodeURIComponent(id)}`, none),
  setEpisodeProgress: (
    id: string,
    progress: {
      status: EpisodeStatus;
      planned_on: string | null;
      last_position_seconds: number | null;
      note: string;
      /** 사용자 시간대의 오늘(완료한 날). */
      today: string;
    },
  ) => expectOk('PUT', `/api/episodes/${encodeURIComponent(id)}/progress`, none, { ...progress }),
  courseParticipants: (item: string, today: string) =>
    expectOk(
      'GET',
      `/api/items/${encodeURIComponent(item)}/course/participants?today=${encodeURIComponent(today)}`,
      (v) => parseList(v, parseParticipant),
    ),
};

export const courseResources = {
  info: (item: string) =>
    expectOk('GET', `/api/items/${encodeURIComponent(item)}/course`, (value) => {
      const info = rec(value, 'course');
      return { ...info, access: learningAccess(str(info, 'access')) };
    }),
  episodes: (item: string) =>
    expectOk('GET', `/api/items/${encodeURIComponent(item)}/episodes`, (value) =>
      parseList(value, (row) => rec(row, 'episode')),
    ),
  progress: (item: string) =>
    expectOk('GET', `/api/items/${encodeURIComponent(item)}/episode-progress`, (value) =>
      parseList(value, (row) => rec(row, 'episode progress')),
    ),
};
