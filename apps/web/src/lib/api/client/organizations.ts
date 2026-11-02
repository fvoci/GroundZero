import { expectOk, parseViewerList, request } from '$lib/shared/api';
import { bool, optInt, rec, str, type Json } from '$lib/shared/api/parse';
import { me } from '$lib/me.svelte';
import { untrack } from 'svelte';
import type {
  Organization,
  OrganizationLimits,
  OrganizationSummary,
  OrgRole,
} from '@fvoci/api-client';

// --- 조직 (D-111·D-112·D-124) --------------------------------------------

function orgRole(value: string): OrgRole {
  if (value === 'admin' || value === 'member') return value;
  throw new TypeError(`unknown organization role ${value}`);
}

export function parseOrganizationSummary(value: Json): OrganizationSummary {
  const o = rec(value, 'organization');
  return {
    id: str(o, 'id'),
    name: str(o, 'name'),
    role: orgRole(str(o, 'role')),
    public_links_enabled: bool(o, 'public_links_enabled'),
    created_at: str(o, 'created_at'),
  };
}

export function parseOrganizationLimits(value: Json): OrganizationLimits {
  const o = rec(value, 'organization limits');
  return {
    max_file_bytes: optInt(o, 'max_file_bytes'),
    max_storage_bytes: optInt(o, 'max_storage_bytes'),
    max_concurrent_uploads: optInt(o, 'max_concurrent_uploads'),
    activity_retention_days: optInt(o, 'activity_retention_days'),
    audit_retention_days: optInt(o, 'audit_retention_days'),
  };
}

export function parseOrganization(value: Json): Organization {
  const o = rec(value, 'organization');
  return {
    ...parseOrganizationSummary(value),
    limits: parseOrganizationLimits(o.limits),
  };
}

export const organizationsApi = {
  /** 내가 속한 조직(전환기 목록, D-111). 서버는 기본 조직을 정하지 않는다. */
  myOrganizations: async (viewerId = untrack(() => me.userId)) => {
    const response = await request('GET', '/api/me/organizations');
    return parseViewerList(response, viewerId, parseOrganizationSummary);
  },
  organization: (id: string) =>
    expectOk('GET', `/api/organizations/${encodeURIComponent(id)}`, parseOrganization),
};
