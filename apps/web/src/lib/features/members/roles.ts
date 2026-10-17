import type { Role } from '@fvoci/api-client';

import { m } from '$lib/paraglide/messages.js';

/** 역할 표시 이름. 색만이 아니라 이름으로 구분한다(D-66). 표시 언어가 바뀔 수 있어 부를 때 계산한다. */
export function roleLabel(role: Role): string {
  switch (role) {
    case 'admin':
      return m.members_role_admin();
    case 'writer':
      return m.members_role_writer();
    case 'reader':
      return m.members_role_reader();
  }
}

export const ROLES: Role[] = ['admin', 'writer', 'reader'];

export function asRole(value: string): Role | undefined {
  return value === 'admin' || value === 'writer' || value === 'reader' ? value : undefined;
}
