import type { Organization } from '@fvoci/api-client';
import { spyOn } from 'storybook/test';
import { api } from '$lib/api/client';
import { organizationsApi } from '$lib/features/organizations/api';
import type {
  OrganizationInvitation,
  OrganizationMember,
  OrganizationWorkspace,
} from '$lib/features/organizations/model';
import { org } from '$lib/org.svelte';
import { organizationPath, type OrganizationTab } from '$lib/router.svelte';
import { setUpScreen } from './screens';

export const ORGANIZATION_MANAGEMENT: Organization = {
  id: '31000000-0000-4000-8000-000000000001',
  name: '긴 이름을 쓰는 여러 연구실과 프로젝트 팀이 함께 일하는 합성 조직',
  role: 'admin',
  public_links_enabled: true,
  created_at: '2026-10-01T00:00:00Z',
  limits: {
    max_file_bytes: 100 * 1024 ** 2,
    max_storage_bytes: 10 * 1024 ** 3,
    max_concurrent_uploads: 3,
    activity_retention_days: 365,
    audit_retention_days: null,
  },
};
export const ORGANIZATION_MEMBERS: OrganizationMember[] = Array.from(
  { length: 50 },
  (_, index) => ({
    user_id: index === 0 ? 'story-user' : `synthetic-member-${String(index)}`,
    display_name:
      index === 0
        ? '긴 이름을 가진 조직 관리자와 연구 책임자'
        : `여러 연구실이 함께 쓰는 아주 긴 합성 멤버 이름 ${String(index)}`,
    email: `long.synthetic.organization.member.address.${String(index)}@organization-management.example.test`,
    role: index < 2 ? 'admin' : 'member',
    created_at: '2026-10-01T00:00:00Z',
  }),
);
export const ORGANIZATION_INVITATIONS: OrganizationInvitation[] = [
  {
    id: 'invite-active',
    email: 'long.invited.member.address.for.narrow.layout@organization-management.example.test',
    role: 'member',
    expires_at: '2099-12-31T00:00:00Z',
  },
  {
    id: 'invite-expired',
    email: 'expired.administrator@organization-management.example.test',
    role: 'admin',
    expires_at: '2000-01-01T00:00:00Z',
  },
];
export const ORGANIZATION_WORKSPACES: OrganizationWorkspace[] = [
  {
    id: '31000000-0000-4000-8000-000000000002',
    name: '긴 이름을 쓰는 공동 연구 공간과 함께 보는 기록',
    owner_id: 'owner',
    owner_name: '긴 이름을 가진 합성 공간 소유자',
    member_count: 50,
    created_at: '2026-10-01T00:00:00Z',
    my_role: null,
    deleted_at: null,
  },
  {
    id: '31000000-0000-4000-8000-000000000003',
    name: '이미 참여한 공간',
    owner_id: 'owner',
    owner_name: '공간 소유자',
    member_count: 4,
    created_at: '2026-10-01T00:00:00Z',
    my_role: 'member',
    deleted_at: null,
  },
  {
    id: '31000000-0000-4000-8000-000000000004',
    name: '보존 기간 안에 복원할 수 있는 지운 공간',
    owner_id: 'owner',
    owner_name: '공간 소유자',
    member_count: 3,
    created_at: '2026-10-01T00:00:00Z',
    my_role: null,
    deleted_at: '2026-10-20T00:00:00Z',
  },
];

export function setUpOrganizations(tab: OrganizationTab): () => void {
  const cleanup = setUpScreen(organizationPath(ORGANIZATION_MANAGEMENT.id, tab));
  org.load('story-user', [ORGANIZATION_MANAGEMENT], ORGANIZATION_MANAGEMENT.id);
  const mocks = [
    spyOn(api, 'organization').mockResolvedValue(ORGANIZATION_MANAGEMENT),
    spyOn(organizationsApi, 'members').mockResolvedValue(ORGANIZATION_MEMBERS),
    spyOn(organizationsApi, 'invitations').mockResolvedValue(ORGANIZATION_INVITATIONS),
    spyOn(organizationsApi, 'workspaces').mockResolvedValue(ORGANIZATION_WORKSPACES),
    spyOn(organizationsApi, 'setRole').mockResolvedValue(undefined),
    spyOn(organizationsApi, 'removeMember').mockResolvedValue(undefined),
    spyOn(organizationsApi, 'removalPreview').mockResolvedValue({
      workspaces: 2,
      shares: 0,
      assignments: 3,
    }),
    spyOn(organizationsApi, 'inviteOrganization').mockImplementation((_id, email, role) =>
      Promise.resolve({ id: 'new-invite', email, role, expires_at: '2099-12-31T00:00:00Z' }),
    ),
    spyOn(api, 'revokeInvitation').mockResolvedValue(undefined),
    spyOn(organizationsApi, 'joinAsAdmin').mockResolvedValue(undefined),
    spyOn(organizationsApi, 'offerOwnership').mockResolvedValue(undefined),
    spyOn(organizationsApi, 'restoreWorkspace').mockResolvedValue(undefined),
    spyOn(organizationsApi, 'settings').mockImplementation((_id, patch) => {
      const { name, public_links_enabled, ...limits } = patch;
      return Promise.resolve({
        ...ORGANIZATION_MANAGEMENT,
        name: name ?? ORGANIZATION_MANAGEMENT.name,
        public_links_enabled: public_links_enabled ?? ORGANIZATION_MANAGEMENT.public_links_enabled,
        limits: { ...ORGANIZATION_MANAGEMENT.limits, ...limits },
      });
    }),
  ];
  return () => {
    for (const mock of mocks) mock.mockRestore();
    cleanup();
  };
}
