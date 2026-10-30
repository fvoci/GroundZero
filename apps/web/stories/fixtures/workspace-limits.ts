import type { MembersView, Organization, OrganizationLimits } from '@fvoci/api-client';

import { WORKSPACE } from './synthetic';

export const LIMITS_WORKSPACE = {
  ...WORKSPACE,
  name: '장기 공동연구와 자료 보존을 위한 공간 · Long-running shared laboratory',
};
export const WORKSPACE_LIMITS: OrganizationLimits = {
  max_file_bytes: 32 * 1024 ** 2,
  max_storage_bytes: 2 * 1024 ** 3,
  max_concurrent_uploads: 3,
  activity_retention_days: 60,
  audit_retention_days: null,
};
export const LIMITS_ORGANIZATION: Organization = {
  id: WORKSPACE.organization_id,
  name: '공동연구 조직 · Shared research organization',
  role: 'admin',
  public_links_enabled: true,
  created_at: '2026-10-30T00:00:00Z',
  limits: {
    max_file_bytes: 100 * 1024 ** 2,
    max_storage_bytes: 8 * 1024 ** 3,
    max_concurrent_uploads: 8,
    activity_retention_days: 90,
    audit_retention_days: 365,
  },
};
export const LIMITS_MEMBERS: MembersView = {
  can_manage_members: true,
  my_role: 'admin',
  i_am_owner: true,
  pending_owner: null,
  members: Array.from({ length: 50 }, (_, index) => ({
    user_id: index === 0 ? 'story-user' : `limits-member-${String(index)}`,
    display_name: `${String(index + 1)}. 공동연구 자료 관리와 장기 보존 담당자 · Research collaborator`,
    email: `research-collaborator-${String(index + 1)}.long-email-address@shared-laboratory.example.test`,
    role: index === 0 ? 'admin' : 'member',
    is_owner: index === 0,
  })),
};
