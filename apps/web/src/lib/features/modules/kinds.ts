// 모듈 기능 유형의 표시 이름 (D-14).
import type { ModuleKind } from '@fvoci/api-client';

import { MODULE_KINDS } from '$lib/api/client';

export const KIND_LABEL: Record<ModuleKind, string> = {
  project: '프로젝트',
  research: '연구',
  study: '스터디',
  course: '강좌',
  hobby: '취미',
};

export function asKind(value: string): ModuleKind | undefined {
  return MODULE_KINDS.find((k) => k === value);
}

/** 서버와 같은 키 규칙: 2~10자 영문·숫자, 영문으로 시작, WIKI 예약. 소문자는 대문자로 바꾼다. */
export function normalizeKey(raw: string): string | undefined {
  const key = raw.trim().toUpperCase();
  if (!/^[A-Z][A-Z0-9]{1,9}$/.test(key) || key === 'WIKI') return undefined;
  return key;
}
