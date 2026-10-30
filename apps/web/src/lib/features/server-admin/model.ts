import type { components } from '@fvoci/api-client';
import { int, rec, str } from '$lib/shared/api/parse';

export type ServerOrganization = components['schemas']['ServerOrganization'];
export type ServerAdmin = components['schemas']['ServerAdmin'];
export type CreateOrganization = components['schemas']['CreateOrganization'];

export function parseServerOrganization(value: unknown): ServerOrganization {
  const o = rec(value, 'server organization');
  return {
    id: str(o, 'id'),
    name: str(o, 'name'),
    member_count: int(o, 'member_count'),
    workspace_count: int(o, 'workspace_count'),
    created_at: str(o, 'created_at'),
  };
}

export function parseServerAdmin(value: unknown): ServerAdmin {
  const o = rec(value, 'server administrator');
  return {
    user_id: str(o, 'user_id'),
    display_name: str(o, 'display_name'),
    created_at: str(o, 'created_at'),
  };
}

export function parseCreatedOrganization(
  value: unknown,
): components['schemas']['CreatedOrganization'] {
  return { id: str(rec(value, 'created organization'), 'id') };
}
