import type { components, OrgRole } from '@fvoci/api-client';
import { optStr, rec, str } from '$lib/shared/api/parse';

export type OrganizationMember = components['schemas']['OrganizationMember'];
export type OrganizationInvitation = components['schemas']['OrganizationInvitation'];
export type OrganizationWorkspace = components['schemas']['OrganizationWorkspace'];
export type OrganizationRemovalPreview = components['schemas']['OrganizationRemovalPreview'];
export type OrganizationSettingsPatch = components['schemas']['OrganizationSettingsPatch'];

export function parseOrgRole(value: unknown): OrgRole {
  if (value === 'admin' || value === 'member') return value;
  throw new TypeError('unknown organization role');
}

export function parseOrganizationMember(value: unknown): OrganizationMember {
  const o = rec(value, 'organization member');
  return {
    user_id: str(o, 'user_id'),
    display_name: str(o, 'display_name'),
    email: optStr(o, 'email'),
    role: parseOrgRole(o.role),
    created_at: str(o, 'created_at'),
  };
}
