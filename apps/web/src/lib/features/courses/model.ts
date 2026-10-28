import type { components } from '@fvoci/api-client';
import { int, optInt, optStr, parseList, rec, str, type Json } from '$lib/shared/api/parse';

export type EpisodeStatus = 'not_started' | 'in_progress' | 'done';

export interface CourseDetailsView extends Required<components['schemas']['CourseDetails']> {
  subject: string;
  instructor: string;
  platform: string;
  url: string;
  textbook: string;
  starts_on: string | null;
  expires_on: string | null;
  target_on: string | null;
}

export interface EpisodeView extends Required<components['schemas']['Episode']> {
  id: string;
  number: number;
  title: string;
  chapter: string;
  duration_seconds: number;
  url: string;
  materials: string;
  planned_on: string | null;
  mine: {
    status: EpisodeStatus;
    planned_on: string | null;
    completed_on: string | null;
    watch_count: number;
    last_position_seconds: number | null;
    note: string;
  };
  /** 공통 정보의 판. 고칠 때 읽은 판을 보낸다(그 사이 다른 사람이 고쳤으면 409). */
  version: number;
}

export interface CourseView {
  details: CourseDetailsView;
  episodes: EpisodeView[];
  access: components['schemas']['LearningAccess'];
}

export interface ParticipantView extends Required<components['schemas']['Participant']> {
  user_id: string;
  display_name: string;
  done: number;
  in_progress: number;
  overdue: number;
  done_seconds: number;
}

export interface NewEpisodeInput {
  request_id: string;
  number?: number;
  title: string;
  chapter: string;
  duration_seconds: number;
  url: string;
  materials: string;
  planned_on: string | null;
}

export function episodeStatus(value: string): EpisodeStatus {
  if (value === 'not_started' || value === 'in_progress' || value === 'done') return value;
  throw new TypeError(`unknown episode status ${value}`);
}

export function parseCourseView(value: Json): CourseView {
  const o = rec(value, 'course');
  const d = rec(o.details, 'course details');
  return {
    details: {
      subject: str(d, 'subject'),
      instructor: str(d, 'instructor'),
      platform: str(d, 'platform'),
      url: str(d, 'url'),
      textbook: str(d, 'textbook'),
      starts_on: optStr(d, 'starts_on'),
      expires_on: optStr(d, 'expires_on'),
      target_on: optStr(d, 'target_on'),
    },
    episodes: parseList(o.episodes, (v): EpisodeView => {
      const e = rec(v, 'episode');
      const m = rec(e.mine, 'episode progress');
      return {
        id: str(e, 'id'),
        number: int(e, 'number'),
        title: str(e, 'title'),
        chapter: str(e, 'chapter'),
        duration_seconds: int(e, 'duration_seconds'),
        url: str(e, 'url'),
        materials: str(e, 'materials'),
        planned_on: optStr(e, 'planned_on'),
        mine: {
          status: episodeStatus(str(m, 'status')),
          planned_on: optStr(m, 'planned_on'),
          completed_on: optStr(m, 'completed_on'),
          watch_count: int(m, 'watch_count'),
          last_position_seconds: optInt(m, 'last_position_seconds'),
          note: str(m, 'note'),
        },
        version: int(e, 'version'),
      };
    }),
    access: learningAccess(str(o, 'access')),
  };
}

export function parseParticipant(value: Json): ParticipantView {
  const o = rec(value, 'participant');
  return {
    user_id: str(o, 'user_id'),
    display_name: str(o, 'display_name'),
    done: int(o, 'done'),
    in_progress: int(o, 'in_progress'),
    overdue: int(o, 'overdue'),
    done_seconds: int(o, 'done_seconds'),
  };
}

export function learningAccess(value: string): components['schemas']['LearningAccess'] {
  if (value === 'write' || value === 'read' || value === 'archived') return value;
  throw new TypeError(`unknown learning access ${value}`);
}
