import type { components } from '@fvoci/api-client';
import { bool, int, optStr, parseList, rec, str, type Json } from '$lib/shared/api/parse';

import { episodeStatus, type EpisodeStatus } from '$lib/features/courses/model';

// --- 스터디 계획 (D-36) ------------------------------------------------------

export const STUDY_UNIT_KINDS = ['pages', 'problems', 'minutes', 'items'] as const;
export type StudyUnitKind = (typeof STUDY_UNIT_KINDS)[number];

export interface StudyUnitView extends Required<components['schemas']['StudyUnit']> {
  id: string;
  number: number;
  title: string;
  chapter: string;
  unit: StudyUnitKind;
  amount: number;
  planned_on: string | null;
  mine: {
    status: EpisodeStatus;
    done_amount: number;
    planned_on: string | null;
    completed_on: string | null;
    note: string;
  };
  /** 공통 정보의 판. 고칠 때 읽은 판을 보낸다(그 사이 다른 사람이 고쳤으면 409). */
  version: number;
}

export interface StudyPlanView {
  units: StudyUnitView[];
  can_write: boolean;
}

export interface StudyParticipantView extends Required<components['schemas']['StudyParticipant']> {
  user_id: string;
  display_name: string;
  done: number;
  in_progress: number;
  overdue: number;
  done_amount: number;
}

function studyUnitKind(value: string): StudyUnitKind {
  const found = STUDY_UNIT_KINDS.find((k) => k === value);
  if (found === undefined) throw new TypeError(`unknown study unit ${value}`);
  return found;
}

export function parseStudyPlanView(value: Json): StudyPlanView {
  const o = rec(value, 'study plan');
  return {
    can_write: bool(o, 'can_write'),
    units: parseList(o.units, (u): StudyUnitView => {
      const r = rec(u, 'study unit');
      const m = rec(r.mine, 'unit progress');
      return {
        id: str(r, 'id'),
        number: int(r, 'number'),
        title: str(r, 'title'),
        chapter: str(r, 'chapter'),
        unit: studyUnitKind(str(r, 'unit')),
        amount: int(r, 'amount'),
        planned_on: optStr(r, 'planned_on'),
        mine: {
          status: episodeStatus(str(m, 'status')),
          done_amount: int(m, 'done_amount'),
          planned_on: optStr(m, 'planned_on'),
          completed_on: optStr(m, 'completed_on'),
          note: str(m, 'note'),
        },
        version: int(r, 'version'),
      };
    }),
  };
}
