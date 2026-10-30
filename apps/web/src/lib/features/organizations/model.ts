import type { components, OrgRole } from '@fvoci/api-client';
import { int, optStr, rec, str } from '$lib/shared/api/parse';

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

export function parseOrganizationInvitation(value: unknown): OrganizationInvitation {
  const o = rec(value, 'organization invitation');
  return {
    id: str(o, 'id'),
    email: str(o, 'email'),
    role: parseOrgRole(o.role),
    expires_at: str(o, 'expires_at'),
  };
}

export function parseOrganizationWorkspace(value: unknown): OrganizationWorkspace {
  const o = rec(value, 'organization workspace');
  const role = optStr(o, 'my_role');
  if (role !== null && role !== 'admin' && role !== 'member' && role !== 'readonly')
    throw new TypeError('unknown workspace role');
  return {
    id: str(o, 'id'),
    name: str(o, 'name'),
    owner_id: str(o, 'owner_id'),
    owner_name: str(o, 'owner_name'),
    member_count: int(o, 'member_count'),
    created_at: str(o, 'created_at'),
    deleted_at: optStr(o, 'deleted_at'),
    my_role: role,
  };
}

export function parseOrganizationRemovalPreview(value: unknown): OrganizationRemovalPreview {
  const o = rec(value, 'organization removal preview');
  return {
    workspaces: int(o, 'workspaces'),
    shares: int(o, 'shares'),
    assignments: int(o, 'assignments'),
  };
}
