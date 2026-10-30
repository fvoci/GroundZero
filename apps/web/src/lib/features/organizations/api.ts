import type { components, Role, OrgRole } from '@fvoci/api-client';

import { parseInvitation, parseOrganization } from '$lib/api/client';
import { expectOk } from '$lib/shared/api';
import { none, parseList } from '$lib/shared/api/parse';
import {
  parseOrganizationMember,
  parseOrganizationInvitation,
  parseOrganizationWorkspace,
  parseOrganizationRemovalPreview,
  type OrganizationSettingsPatch,
} from './model';

type SetLastOrganization = components['schemas']['SetLastOrganization'];
type DisplaySettings = components['schemas']['MySettings'];

// #178·D-105: 마지막 조직 선택은 표시 설정과 독립된 명령이다.
export const organizationsApi = {
  setRole: (organizationId: string, userId: string, role: OrgRole) => {
    const body: components['schemas']['SetOrganizationRole'] = { role };
    return expectOk(
      'PUT',
      `/api/organizations/${encodeURIComponent(organizationId)}/members/${encodeURIComponent(userId)}`,
      none,
      body,
    );
  },
  removeMember: (organizationId: string, userId: string) =>
    expectOk(
      'DELETE',
      `/api/organizations/${encodeURIComponent(organizationId)}/members/${encodeURIComponent(userId)}`,
      none,
    ),
  removalPreview: (organizationId: string, userId: string) =>
    expectOk(
      'GET',
      `/api/organizations/${encodeURIComponent(organizationId)}/members/${encodeURIComponent(userId)}/removal-preview`,
      parseOrganizationRemovalPreview,
    ),
  invitations: (organizationId: string) =>
    expectOk('GET', `/api/organizations/${encodeURIComponent(organizationId)}/invitations`, (v) =>
      parseList(v, parseOrganizationInvitation),
    ),
  inviteOrganization: (organizationId: string, email: string, role: OrgRole) => {
    const body: components['schemas']['InviteOrganization'] = { email, role };
    return expectOk(
      'POST',
      `/api/organizations/${encodeURIComponent(organizationId)}/invitations`,
      parseOrganizationInvitation,
      body,
    );
  },
  workspaces: (organizationId: string) =>
    expectOk('GET', `/api/organizations/${encodeURIComponent(organizationId)}/workspaces`, (v) =>
      parseList(v, parseOrganizationWorkspace),
    ),
  joinAsAdmin: (organizationId: string, workspaceId: string) =>
    expectOk(
      'POST',
      `/api/organizations/${encodeURIComponent(organizationId)}/workspaces/${encodeURIComponent(workspaceId)}/join-as-admin`,
      none,
    ),
  offerOwnership: (organizationId: string, workspaceId: string, toUser: string) => {
    const body: components['schemas']['OfferOwnership'] = { to_user: toUser };
    return expectOk(
      'POST',
      `/api/organizations/${encodeURIComponent(organizationId)}/workspaces/${encodeURIComponent(workspaceId)}/ownership-offer`,
      none,
      body,
    );
  },
  restoreWorkspace: (organizationId: string, workspaceId: string) =>
    expectOk(
      'POST',
      `/api/organizations/${encodeURIComponent(organizationId)}/workspaces/${encodeURIComponent(workspaceId)}/restore`,
      none,
    ),
  settings: (organizationId: string, body: OrganizationSettingsPatch) =>
    expectOk(
      'PUT',
      `/api/organizations/${encodeURIComponent(organizationId)}/settings`,
      parseOrganization,
      body,
    ),
  members: (organizationId: string) =>
    expectOk('GET', `/api/organizations/${encodeURIComponent(organizationId)}/members`, (v) =>
      parseList(v, parseOrganizationMember),
    ),
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
