import type { Role, ShareRecipientRole } from '@fvoci/api-client';

import { m } from '$lib/paraglide/messages.js';

/** 역할 표시 이름(D-112: 관리자·멤버·읽기 전용). 색만이 아니라 이름으로 구분한다(D-66). 표시 언어가 바뀔 수 있어 부를 때 계산한다. */
export function roleLabel(role: Role | ShareRecipientRole): string {
  switch (role) {
    case 'admin':
      return m.members_role_admin();
    case 'member':
      return m.members_role_member();
    case 'readonly':
      return m.members_role_readonly();
    case 'outside_member':
      return m.members_role_outside_member();
  }
}

export const ROLES: Role[] = ['admin', 'member', 'readonly'];
export const DISPLAY_ROLES: ShareRecipientRole[] = [
  'admin',
  'member',
  'readonly',
  'outside_member',
];

export function asRole(value: string): Role | undefined {
  return value === 'admin' || value === 'member' || value === 'readonly' ? value : undefined;
}

/** 읽기 전용은 댓글을 포함해 쓰지 않는다(D-20). 쓰기 여부의 정본은 서버의 `app_access` 플래그이고 이 값은 역할 표시에만 쓴다. */
export function isReadonly(role: Role): boolean {
  return role === 'readonly';
}
