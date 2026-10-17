// 감사 기록의 동작·대상 이름(D-57). 모르는 코드는 그대로 보인다.
export const AUDIT_ACTION: Record<string, string> = {
  create: '만듦',
  rename: '이름 바꿈',
  move_in: '옮겨 옴',
  move_out: '옮겨 감',
  reparent: '위치 바꿈',
  copy: '복제',
  delete: '지움',
  restore: '되살림',
  purge: '영구 삭제',
  restore_revision: '이전 리비전으로 되돌림',
  upload: '올림',
  delete_version: '버전 지움',
  share: '공유',
  unshare: '공유 해제',
  create_public_link: '공개 링크 만듦',
  revoke_public_link: '공개 링크 거둠',
  change_visibility: '공개 범위 바꿈',
  change_copy_policy: '복제·다운로드 설정 바꿈',
  change_key: '키 바꿈',
  invite: '초대',
  revoke: '초대 취소',
  join: '들어옴',
  decline: '초대 거절',
  change_role: '역할 바꿈',
  leave: '나감',
  remove: '내보냄',
  offer_ownership: '소유권 인계 제안',
  cancel_ownership_offer: '소유권 인계 취소',
  accept_ownership: '소유권 받음',
  add_group_member: '그룹에 넣음',
  remove_group_member: '그룹에서 뺌',
  assign: '담당 지정',
  unassign: '담당 해제',
  link_predecessor: '선행 업무 연결',
  unlink_predecessor: '선행 업무 해제',
  reschedule: '일정 바꿈',
  set_recurrence: '반복 설정',
  exclude_occurrence: '회차 뺌',
  set_trash_retention: '휴지통 보존 기간 바꿈',
  export: '반출',
};

export const AUDIT_TARGET: Record<string, string> = {
  document: '문서',
  task: '업무',
  item: '항목',
  module: '모듈',
  event: '일정',
  file: '파일',
  member: '멤버',
  invitation: '초대',
  group: '그룹',
  workspace: '워크스페이스',
  user: '내 자료',
};

/** 기록의 detail에서 사람이 알아볼 이름(제목·이름·이메일). */
export function auditSubject(detail: unknown): string {
  if (typeof detail !== 'object' || detail === null) return '';
  for (const key of ['title', 'name', 'email', 'to']) {
    const value: unknown = Reflect.get(detail, key);
    if (typeof value === 'string' && value !== '') return value;
  }
  return '';
}
