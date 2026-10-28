// 스터디 화면은 정보·단위·개인 진행을 일정한 세 읽기로 조합한다.
import { str } from '$lib/shared/api/parse';
import { studyResources } from './api';
import { parseStudyPlanView, type StudyPlanView, type StudyUnitView } from './model';

function emptyProgress(): StudyUnitView['mine'] {
  return {
    status: 'not_started',
    done_amount: 0,
    planned_on: null,
    completed_on: null,
    note: '',
  };
}

export async function loadStudyView(item: string): Promise<StudyPlanView> {
  const [info, units, progress] = await Promise.all([
    studyResources.info(item),
    studyResources.units(item),
    studyResources.progress(item),
  ]);
  const mine = new Map(progress.map((row) => [str(row, 'unit_id'), row]));
  return parseStudyPlanView({
    ...info,
    can_write: info.access === 'write',
    units: units.map((unit) => ({ ...unit, mine: mine.get(str(unit, 'id')) ?? emptyProgress() })),
  });
}
