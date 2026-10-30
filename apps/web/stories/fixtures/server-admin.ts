import { spyOn } from 'storybook/test';
import { serverAdminApi } from '$lib/features/server-admin/api';
import type { ServerAdmin, ServerOrganization } from '$lib/features/server-admin/model';
import { serverAdminPath, type ServerAdminTab } from '$lib/router.svelte';
import { ORGANIZATION_MANAGEMENT, ORGANIZATION_MEMBERS } from './organizations';
import { setUpScreen } from './screens';

// Reuse the 50 synthetic members (with long names and emails); these APIs expose metadata only.
export const SERVER_ORGANIZATIONS: ServerOrganization[] = [
  {
    id: ORGANIZATION_MANAGEMENT.id,
    name: ORGANIZATION_MANAGEMENT.name,
    member_count: ORGANIZATION_MEMBERS.length,
    workspace_count: 12,
    created_at: ORGANIZATION_MANAGEMENT.created_at,
  },
  {
    id: '31000000-0000-4000-8000-000000000005',
    name: '아주긴이름을띄어쓰기없이사용하는서버관리목록의합성조직과공동연구실',
    member_count: 1,
    workspace_count: 0,
    created_at: '2026-10-29T00:00:00Z',
  },
];
export const SERVER_ADMINS: ServerAdmin[] = ORGANIZATION_MEMBERS.map((member) => ({
  user_id: member.user_id,
  display_name: member.display_name,
  created_at: member.created_at,
}));

export function setUpServerAdmin(tab: ServerAdminTab): () => void {
  const cleanup = setUpScreen(serverAdminPath(tab));
  let organizations = [...SERVER_ORGANIZATIONS];
  const mocks = [
    spyOn(serverAdminApi, 'organizations').mockImplementation(() => Promise.resolve(organizations)),
    spyOn(serverAdminApi, 'admins').mockResolvedValue(SERVER_ADMINS),
    spyOn(serverAdminApi, 'createOrganization').mockImplementation((body) => {
      const id = body.request_id;
      organizations = [
        ...organizations,
        {
          id,
          name: body.name,
          member_count: 1,
          workspace_count: 0,
          created_at: '2026-10-30T00:00:00Z',
        },
      ];
      return Promise.resolve({ id });
    }),
  ];
  return () => {
    for (const mock of mocks) mock.mockRestore();
    cleanup();
  };
}
