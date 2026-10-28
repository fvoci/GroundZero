import type { components, Role } from '@fvoci/api-client';

import { parseInvitation } from '$lib/api/client';
import { expectOk } from '$lib/shared/api';

type SetLastOrganization = components['schemas']['SetLastOrganization'];
type DisplaySettings = components['schemas']['MySettings'];

// #178·D-105: 마지막 조직 선택은 표시 설정과 독립된 명령이다.
export const organizationsApi = {
  setLastOrganization: (body: SetLastOrganization) =>
    expectOk('PUT', '/api/me/last-organization', () => undefined, body),
  setDisplaySettings: (body: DisplaySettings) =>
    expectOk('PUT', '/api/me/settings', () => undefined, body),
  invite: (workspaceId: string, email: string, role: Role) => {
    const body: components['schemas']['CreateInvitation'] = { email, role };
    return expectOk(
      'POST',
      `/api/workspaces/${encodeURIComponent(workspaceId)}/invitations`,
      parseInvitation,
      body,
    );
  },
};
