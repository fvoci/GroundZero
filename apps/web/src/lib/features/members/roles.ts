import type { Role } from '@fvoci/api-client';

/** 역할 표시 이름. 색만이 아니라 이름으로 구분한다(D-66). */
export const ROLE_LABEL: Record<Role, string> = {
  admin: '관리자',
  writer: '작성자',
  reader: '독자',
};

export const ROLES: Role[] = ['admin', 'writer', 'reader'];

export function asRole(value: string): Role | undefined {
  return value === 'admin' || value === 'writer' || value === 'reader' ? value : undefined;
}
