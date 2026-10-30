import { spyOn } from 'storybook/test';
import { api } from '$lib/api/client';
import { workspaceLimitsApi } from '$lib/features/members/limits-api';
import { me } from '$lib/me.svelte';
import { membersPath, type WorkspaceManagementTab } from '$lib/router.svelte';
import { setUpScreen } from './screens';
import {
  LIMITS_MEMBERS,
  LIMITS_ORGANIZATION,
  LIMITS_WORKSPACE,
  WORKSPACE_LIMITS,
} from './workspace-limits';

export function setUpWorkspaceManagement(tab: WorkspaceManagementTab): () => void {
  const cleanup = setUpScreen(membersPath(LIMITS_WORKSPACE.id, tab), {
    members: () => Promise.resolve(LIMITS_MEMBERS),
  });
  me.userId = 'story-user';
  spyOn(api, 'groups').mockResolvedValue([
    { id: 'review-group', name: '연구팀', members: ['story-user'] },
  ]);
  spyOn(api, 'invitations').mockResolvedValue([
    {
      id: 'invitation-1',
      email: 'review@example.test',
      role: 'member',
      expires_at: '2026-11-15T00:00:00Z',
    },
  ]);
  spyOn(api, 'organization').mockResolvedValue(LIMITS_ORGANIZATION);
  let saved = { ...WORKSPACE_LIMITS };
  spyOn(workspaceLimitsApi, 'get').mockImplementation(() => Promise.resolve(saved));
  spyOn(workspaceLimitsApi, 'set').mockImplementation((_id, patch) => {
    saved = { ...saved, ...patch };
    return Promise.resolve(saved);
  });
  return cleanup;
}
