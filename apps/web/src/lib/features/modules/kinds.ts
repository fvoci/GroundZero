// 모듈 기능 유형의 표시 이름 (D-14). 언어가 바뀔 수 있어 부를 때마다 문구를 만든다(D-81).
import type { ModuleKind } from '@fvoci/api-client';

import { MODULE_KINDS } from '$lib/api/client';
import { m } from '$lib/paraglide/messages.js';

export function kindLabel(kind: ModuleKind): string {
  switch (kind) {
    case 'project':
      return m.modules_kind_project();
    case 'research':
      return m.modules_kind_research();
    case 'study':
      return m.modules_kind_study();
    case 'course':
      return m.modules_kind_course();
    case 'hobby':
      return m.modules_kind_hobby();
  }
}

export function asKind(value: string): ModuleKind | undefined {
  return MODULE_KINDS.find((k) => k === value);
}

/** 서버와 같은 키 규칙: 2~10자 영문·숫자, 영문으로 시작, WIKI 예약. 소문자는 대문자로 바꾼다. */
export function normalizeKey(raw: string): string | undefined {
  const key = raw.trim().toUpperCase();
  if (!/^[A-Z][A-Z0-9]{1,9}$/.test(key) || key === 'WIKI') return undefined;
  return key;
}
