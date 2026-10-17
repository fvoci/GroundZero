// 항목 정보의 표시 이름과 모듈 종류별 칸(D-33 프로젝트, D-34 연구, D-36 스터디, D-38 취미). 언어가 바뀔 수 있어
// 이름은 부를 때마다 만든다(D-81).
import type { ModuleKind } from '@fvoci/api-client';

import type { ItemDetailsView, ItemStatus } from '$lib/api/client';
import { formatDay } from '$lib/display';
import { m } from '$lib/paraglide/messages.js';

export function itemStatusLabel(status: ItemStatus): string {
  switch (status) {
    case 'planned':
      return m.modules_status_planned();
    case 'active':
      return m.modules_status_active();
    case 'on_hold':
      return m.modules_status_on_hold();
    case 'done':
      return m.modules_status_done();
    case 'cancelled':
      return m.modules_status_cancelled();
  }
}

/** 기간(`YYYY-MM-DD` ~ `YYYY-MM-DD`). 한쪽이 없으면 그쪽은 비운다. */
export function periodText(start?: string | null, end?: string | null): string {
  const day = (ymd: string): string => (ymd === '' ? '' : formatDay(ymd));
  return m.modules_date_range({ start: day(start ?? ''), end: day(end ?? '') });
}

type TextKey = Exclude<keyof ItemDetailsView, 'keywords'>;

export interface DetailField {
  key: TextKey;
  label: string;
  multiline: boolean;
}

/** 종류별 글자 칸. 연구는 키워드(쉼표로 여럿)가 따로 있다. */
export function detailFields(kind: ModuleKind): DetailField[] {
  switch (kind) {
    case 'research':
      return [
        { key: 'field', label: m.modules_field_field(), multiline: false },
        { key: 'question', label: m.modules_field_question(), multiline: true },
        { key: 'hypothesis', label: m.modules_field_hypothesis(), multiline: true },
        { key: 'model', label: m.modules_field_model(), multiline: true },
        { key: 'method', label: m.modules_field_method(), multiline: true },
        { key: 'sample', label: m.modules_field_sample(), multiline: false },
      ];
    case 'study':
      return [
        { key: 'level', label: m.modules_field_level(), multiline: false },
        { key: 'target', label: m.modules_field_target(), multiline: false },
      ];
    case 'hobby':
      return [{ key: 'activity_type', label: m.modules_field_activity_type(), multiline: false }];
    default:
      return [];
  }
}

/** 리더·멤버를 부르는 이름(연구는 책임자·공동 연구자, 스터디·취미는 참여자). */
export function peopleLabels(kind: ModuleKind): { leader: string; members: string } {
  if (kind === 'research')
    return { leader: m.modules_people_principal(), members: m.modules_people_co_researchers() };
  if (kind === 'study' || kind === 'hobby')
    return { leader: m.modules_people_leader(), members: m.modules_people_participants() };
  return { leader: m.modules_people_leader(), members: m.modules_people_members() };
}

/** 입력 칸 값 → 보낼 정보. 빈 칸은 빼고, 키워드는 쉼표로 나눈다. 종류에 없는 칸은 보내지 않는다. */
export function toDetails(
  kind: ModuleKind,
  values: Partial<Record<TextKey, string>>,
  keywords: string,
): ItemDetailsView {
  const out: ItemDetailsView = {};
  for (const f of detailFields(kind)) {
    const v = (values[f.key] ?? '').trim();
    if (v !== '') out[f.key] = v;
  }
  if (kind === 'research') {
    const list = [
      ...new Set(
        keywords
          .split(',')
          .map((k) => k.trim())
          .filter((k) => k !== ''),
      ),
    ];
    if (list.length > 0) out.keywords = list;
  }
  return out;
}
