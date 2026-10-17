// 항목 정보의 표시 이름과 모듈 종류별 칸(D-33 프로젝트, D-34 연구, D-36 스터디, D-38 취미).
import type { ModuleKind } from '@fvoci/api-client';

import type { ItemDetailsView, ItemStatus } from '$lib/api/client';

export const ITEM_STATUS_LABEL: Record<ItemStatus, string> = {
  planned: '계획',
  active: '진행',
  on_hold: '보류',
  done: '완료',
  cancelled: '취소',
};

type TextKey = Exclude<keyof ItemDetailsView, 'keywords'>;

export interface DetailField {
  key: TextKey;
  label: string;
  multiline: boolean;
}

const RESEARCH: DetailField[] = [
  { key: 'field', label: '분야', multiline: false },
  { key: 'question', label: '연구 질문', multiline: true },
  { key: 'hypothesis', label: '가설', multiline: true },
  { key: 'model', label: '모형', multiline: true },
  { key: 'method', label: '방법', multiline: true },
  { key: 'sample', label: '표본', multiline: false },
];

/** 종류별 글자 칸. 연구는 키워드(쉼표로 여럿)가 따로 있다. */
export function detailFields(kind: ModuleKind): DetailField[] {
  switch (kind) {
    case 'research':
      return RESEARCH;
    case 'study':
      return [
        { key: 'level', label: '수준', multiline: false },
        { key: 'target', label: '목표 분량', multiline: false },
      ];
    case 'hobby':
      return [{ key: 'activity_type', label: '활동 유형', multiline: false }];
    default:
      return [];
  }
}

/** 리더·멤버를 부르는 이름(연구는 책임자·공동 연구자, 스터디·취미는 참여자). */
export function peopleLabels(kind: ModuleKind): { leader: string; members: string } {
  if (kind === 'research') return { leader: '책임자', members: '공동 연구자' };
  if (kind === 'study' || kind === 'hobby') return { leader: '리더', members: '참여자' };
  return { leader: '리더', members: '멤버' };
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
