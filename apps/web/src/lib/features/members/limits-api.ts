import type { OrganizationLimits } from '@fvoci/api-client';

import { parseOrganizationLimits } from '$lib/api/client';
import { expectOk } from '$lib/shared/api';

export const workspaceLimitsApi = {
  get: (workspaceId: string) =>
    expectOk(
      'GET',
      `/api/workspaces/${encodeURIComponent(workspaceId)}/limits`,
      parseOrganizationLimits,
    ),
  set: (workspaceId: string, patch: OrganizationLimits) =>
    expectOk(
      'PUT',
      `/api/workspaces/${encodeURIComponent(workspaceId)}/limits`,
      parseOrganizationLimits,
      patch,
    ),
};
