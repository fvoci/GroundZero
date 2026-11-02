import { expectOk, parseViewerList, request } from '$lib/shared/api';
import { me } from '$lib/me.svelte';
import { untrack } from 'svelte';
import { access, bool, optStr, parseList, rec, str, type Json } from '$lib/shared/api/parse';
import type {
  Accepted,
  Group,
  Invitation,
  Member,
  MembersView,
  ReceivedInvitation,
  Role,
  Workspace,
} from '@fvoci/api-client';
import { none } from './common';

function role(value: string): Role {
  if (value === 'admin' || value === 'member' || value === 'readonly') return value;
  throw new TypeError(`unknown role ${value}`);
}

export function parseWorkspace(value: Json): Workspace {
  const o = rec(value, 'workspace');
  return {
    id: str(o, 'id'),
    organization_id: str(o, 'organization_id'),
    name: str(o, 'name'),
    role: role(str(o, 'role')),
    is_owner: bool(o, 'is_owner'),
    access: access(str(o, 'access')),
  };
}

/** 초대 수락 결과. 조직 초대는 워크스페이스가 없다(D-112·D-118). */
export function parseAccepted(value: Json): Accepted {
  const o = rec(value, 'accepted');
  return {
    kind: invitationTarget(str(o, 'kind')),
    organization_id: str(o, 'organization_id'),
    workspace_id: optStr(o, 'workspace_id'),
  };
}

export function parseMembersView(value: Json): MembersView {
  const o = rec(value, 'members view');
  const members = parseList(o.members, (m): Member => {
    const r = rec(m, 'member');
    return {
      user_id: str(r, 'user_id'),
      display_name: str(r, 'display_name'),
      email: optStr(r, 'email'),
      role: role(str(r, 'role')),
      is_owner: bool(r, 'is_owner'),
    };
  });
  return {
    members,
    my_role: role(str(o, 'my_role')),
    i_am_owner: bool(o, 'i_am_owner'),
    can_manage_members: bool(o, 'can_manage_members'),
    pending_owner: optStr(o, 'pending_owner'),
  };
}

export function parseInvitation(value: Json): Invitation {
  const o = rec(value, 'invitation');
  return {
    id: str(o, 'id'),
    email: str(o, 'email'),
    role: role(str(o, 'role')),
    expires_at: str(o, 'expires_at'),
  };
}

/** 초대 대상 종류. 조직 초대는 워크스페이스 칸이 null이다(D-112·D-118). */
function invitationTarget(value: string): ReceivedInvitation['kind'] {
  if (value === 'organization' || value === 'workspace') return value;
  throw new TypeError(`unknown invitation kind ${value}`);
}

export function parseReceivedInvitation(value: Json): ReceivedInvitation {
  const o = rec(value, 'received invitation');
  return {
    kind: invitationTarget(str(o, 'kind')),
    organization_id: str(o, 'organization_id'),
    organization_name: str(o, 'organization_name'),
    id: str(o, 'id'),
    workspace_id: optStr(o, 'workspace_id'),
    workspace_name: optStr(o, 'workspace_name'),
    role: role(str(o, 'role')),
    invited_by: str(o, 'invited_by'),
    expires_at: str(o, 'expires_at'),
  };
}

export function parseGroup(value: Json): Group {
  const o = rec(value, 'group');
  return {
    id: str(o, 'id'),
    name: str(o, 'name'),
    members: parseList(o.members, (m) => {
      if (typeof m !== 'string') throw new TypeError('expected member id');
      return m;
    }),
  };
}

export interface DeletedWorkspaceView {
  id: string;
  name: string;
  deleted_at: string;
  expires_at: string;
  expired: boolean;
}

function parseDeletedWorkspace(value: Json): DeletedWorkspaceView {
  const o = rec(value, 'deleted workspace');
  return {
    id: str(o, 'id'),
    name: str(o, 'name'),
    deleted_at: str(o, 'deleted_at'),
    expires_at: str(o, 'expires_at'),
    expired: bool(o, 'expired'),
  };
}

export const workspacesApi = {
  workspaces: async (viewerId = untrack(() => me.userId)) => {
    const response = await request('GET', '/api/workspaces');
    return parseViewerList(response, viewerId, parseWorkspace);
  },
  /** 조직 안 워크스페이스는 조직 멤버 누구나 만든다(D-112). */
  createWorkspace: (requestId: string, organizationId: string, name: string) =>
    expectOk('POST', '/api/workspaces', parseWorkspace, {
      request_id: requestId,
      organization_id: organizationId,
      name,
    }),
  members: (ws: string) =>
    expectOk('GET', `/api/workspaces/${encodeURIComponent(ws)}/members`, parseMembersView),
  setRole: (ws: string, userId: string, newRole: Role) =>
    expectOk(
      'PUT',
      `/api/workspaces/${encodeURIComponent(ws)}/members/${encodeURIComponent(userId)}`,
      none,
      { role: newRole },
    ),
  removeMember: (ws: string, userId: string) =>
    expectOk(
      'DELETE',
      `/api/workspaces/${encodeURIComponent(ws)}/members/${encodeURIComponent(userId)}`,
      none,
    ),
  leave: (ws: string) => expectOk('POST', `/api/workspaces/${encodeURIComponent(ws)}/leave`, none),
  /** 워크스페이스 지우기(소유자, 이름 확인). 영구 삭제되는 때를 돌려준다. */
  deleteWorkspace: (ws: string, name: string) =>
    expectOk(
      'DELETE',
      `/api/workspaces/${encodeURIComponent(ws)}`,
      (v) => str(rec(v, 'workspace deleted'), 'expires_at'),
      { name },
    ),
  deletedWorkspaces: () =>
    expectOk('GET', '/api/me/deleted-workspaces', (v) => parseList(v, parseDeletedWorkspace)),
  restoreWorkspace: (ws: string) =>
    expectOk('POST', `/api/me/deleted-workspaces/${encodeURIComponent(ws)}/restore`, none),
  purgeWorkspace: (ws: string) =>
    expectOk('DELETE', `/api/me/deleted-workspaces/${encodeURIComponent(ws)}`, none),
  invitations: (ws: string) =>
    expectOk('GET', `/api/workspaces/${encodeURIComponent(ws)}/invitations`, (v) =>
      parseList(v, parseInvitation),
    ),
  revokeInvitation: (id: string) =>
    expectOk('DELETE', `/api/invitations/${encodeURIComponent(id)}`, none),
  myInvitations: () =>
    expectOk('GET', '/api/invitations', (v) => parseList(v, parseReceivedInvitation)),
  /** 수락 결과의 `kind`로 조직 초대(워크스페이스 없음)와 워크스페이스 초대를 가른다(D-118). */
  acceptInvitation: (id: string) =>
    expectOk('POST', `/api/invitations/${encodeURIComponent(id)}/accept`, parseAccepted),
  declineInvitation: (id: string) =>
    expectOk('POST', `/api/invitations/${encodeURIComponent(id)}/decline`, none),
  offerOwnership: (ws: string, toUser: string) =>
    expectOk('POST', `/api/workspaces/${encodeURIComponent(ws)}/ownership`, none, {
      to_user: toUser,
    }),
  cancelOwnershipOffer: (ws: string) =>
    expectOk('DELETE', `/api/workspaces/${encodeURIComponent(ws)}/ownership`, none),
  acceptOwnership: (ws: string) =>
    expectOk('POST', `/api/workspaces/${encodeURIComponent(ws)}/ownership/accept`, none),
  groups: (ws: string) =>
    expectOk('GET', `/api/workspaces/${encodeURIComponent(ws)}/groups`, (v) =>
      parseList(v, parseGroup),
    ),
  createGroup: (ws: string, name: string) =>
    expectOk('POST', `/api/workspaces/${encodeURIComponent(ws)}/groups`, parseGroup, { name }),
  deleteGroup: (id: string) => expectOk('DELETE', `/api/groups/${encodeURIComponent(id)}`, none),
  setGroupMember: (groupId: string, userId: string, present: boolean) =>
    expectOk(
      present ? 'PUT' : 'DELETE',
      `/api/groups/${encodeURIComponent(groupId)}/members/${encodeURIComponent(userId)}`,
      none,
    ),
};
