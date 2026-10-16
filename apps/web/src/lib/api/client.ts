// API 클라이언트. 타입은 계약에서 생성한 것(@fvoci/api-client)을 쓰고, 응답은 unknown으로 받아
// 실제 형태를 확인한다(생성 타입은 런타임 검증을 대신하지 않는다 — docs/development.md 3절).
import type {
  AuthInfo,
  Group,
  Health,
  Invitation,
  Me,
  Member,
  MembersView,
  ReceivedInvitation,
  Role,
  Workspace,
} from '@fvoci/api-client';

/** Tiptap JSON 노드. 본문 최상위는 `type: "doc"`. */
export interface DocNode {
  type: string;
  [key: string]: unknown;
}

/** 화면에서 쓰는 문서. 본문을 구체화한 것 외에는 계약의 Document와 같다. */
export type Access = 'owner' | 'write' | 'read';

export interface DocumentView {
  id: string;
  /** 워크스페이스 문서면 소유 워크스페이스, 개인 노트는 null. */
  workspace_id: string | null;
  placement: 'wiki' | 'personal';
  /** 사람이 보는 참조(WIKI-3). 개인 노트는 null. */
  reference: string | null;
  title: string;
  body: DocNode;
  schema_version: number;
  revision: number;
  updated_at: string;
  access: Access;
  can_edit: boolean;
  visibility: 'workspace' | 'private';
  allow_copy: boolean;
}

export class ApiError extends Error {
  constructor(
    readonly status: number,
    readonly code: string,
    message: string,
  ) {
    super(message);
    this.name = 'ApiError';
  }
}

/** 네트워크 실패(응답 없음). 같은 request_id로 재시도할 수 있다. */
export class NetworkError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'NetworkError';
  }
}

type Json = unknown;

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function str(o: Record<string, unknown>, key: string): string {
  const v = o[key];
  if (typeof v !== 'string') throw new TypeError(`expected string at ${key}`);
  return v;
}

function int(o: Record<string, unknown>, key: string): number {
  const v = o[key];
  if (typeof v !== 'number' || !Number.isSafeInteger(v))
    throw new TypeError(`expected integer at ${key}`);
  return v;
}

function bool(o: Record<string, unknown>, key: string): boolean {
  const v = o[key];
  if (typeof v !== 'boolean') throw new TypeError(`expected boolean at ${key}`);
  return v;
}

function rec(value: unknown, what: string): Record<string, unknown> {
  if (!isRecord(value)) throw new TypeError(`expected object for ${what}`);
  return value;
}

function role(value: string): Role {
  if (value === 'admin' || value === 'writer' || value === 'reader') return value;
  throw new TypeError(`unknown role ${value}`);
}

export function parseDocNode(value: unknown): DocNode {
  const o = rec(value, 'document body');
  const type = o.type;
  if (type !== 'doc') throw new TypeError('document body must have type "doc"');
  return { ...o, type };
}

export function parseHealth(value: Json): Health {
  const o = rec(value, 'health');
  return { status: str(o, 'status'), database: str(o, 'database') };
}

export function parseMe(value: Json): Me {
  const o = rec(value, 'me');
  const email = o.email;
  return {
    id: str(o, 'id'),
    display_name: str(o, 'display_name'),
    email: typeof email === 'string' ? email : null,
  };
}

export function parseWorkspace(value: Json): Workspace {
  const o = rec(value, 'workspace');
  return {
    id: str(o, 'id'),
    name: str(o, 'name'),
    role: role(str(o, 'role')),
    is_owner: bool(o, 'is_owner'),
  };
}

export function parseSummary(value: Json): SummaryView {
  const o = rec(value, 'document summary');
  return {
    id: str(o, 'id'),
    reference: optStr(o, 'reference'),
    title: str(o, 'title'),
    revision: int(o, 'revision'),
    updated_at: str(o, 'updated_at'),
  };
}

export function parseDocument(value: Json): DocumentView {
  const o = rec(value, 'document');
  const placement = str(o, 'placement');
  if (placement !== 'wiki' && placement !== 'personal') throw new TypeError('unknown placement');
  const access = str(o, 'access');
  if (access !== 'owner' && access !== 'write' && access !== 'read') {
    throw new TypeError('unknown access');
  }
  const visibility = str(o, 'visibility');
  if (visibility !== 'workspace' && visibility !== 'private') {
    throw new TypeError('unknown visibility');
  }
  const workspaceId = o.workspace_id ?? null;
  const reference = o.reference ?? null;
  if (workspaceId !== null && typeof workspaceId !== 'string') throw new TypeError('workspace_id');
  if (reference !== null && typeof reference !== 'string') throw new TypeError('reference');
  return {
    id: str(o, 'id'),
    workspace_id: workspaceId,
    placement,
    reference,
    title: str(o, 'title'),
    body: parseDocNode(o.body),
    schema_version: int(o, 'schema_version'),
    revision: int(o, 'revision'),
    updated_at: str(o, 'updated_at'),
    access,
    can_edit: bool(o, 'can_edit'),
    visibility,
    allow_copy: bool(o, 'allow_copy'),
  };
}

export interface SummaryView {
  id: string;
  reference: string | null;
  title: string;
  revision: number;
  updated_at: string;
}

export interface SharedView {
  id: string;
  title: string;
  permission: 'read' | 'write';
  shared_by: string;
  updated_at: string;
}

export interface ShareView {
  id: string;
  email: string;
  permission: 'read' | 'write';
}

export interface MovePreviewView {
  gains: string[];
  loses: string[];
  /** 이전 리비전 기록도 함께 옮겨진다. */
  history_moves_with_document: boolean;
}

export interface MoveOutcomeView {
  workspace_id: string;
  reference: string;
  /** 옮긴 뒤 내가 볼 수 없으면 null. */
  document: DocumentView | null;
}

function permission(value: string): 'read' | 'write' {
  if (value === 'read' || value === 'write') return value;
  throw new TypeError('unknown permission');
}

function strings(value: Json): string[] {
  return parseList(value, (s) => {
    if (typeof s !== 'string') throw new TypeError('expected string');
    return s;
  });
}

export function parseSharedView(value: Json): SharedView {
  const o = rec(value, 'shared document');
  return {
    id: str(o, 'id'),
    title: str(o, 'title'),
    permission: permission(str(o, 'permission')),
    shared_by: str(o, 'shared_by'),
    updated_at: str(o, 'updated_at'),
  };
}

export function parseShareView(value: Json): ShareView {
  const o = rec(value, 'share');
  return { id: str(o, 'id'), email: str(o, 'email'), permission: permission(str(o, 'permission')) };
}

export function parseMovePreview(value: Json): MovePreviewView {
  const o = rec(value, 'move preview');
  return {
    gains: strings(o.gains),
    loses: strings(o.loses),
    history_moves_with_document: bool(o, 'history_moves_with_document'),
  };
}

export function parseMoveOutcome(value: Json): MoveOutcomeView {
  const o = rec(value, 'move outcome');
  const doc = o.document ?? null;
  return {
    workspace_id: str(o, 'workspace_id'),
    reference: str(o, 'reference'),
    document: doc === null ? null : parseDocument(doc),
  };
}

function parseList<T>(value: Json, item: (v: Json) => T): T[] {
  if (!Array.isArray(value)) throw new TypeError('expected array');
  return value.map(item);
}

function optStr(o: Record<string, unknown>, key: string): string | null {
  const v = o[key];
  if (v === null || v === undefined) return null;
  if (typeof v !== 'string') throw new TypeError(`expected string or null at ${key}`);
  return v;
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

export function parseReceivedInvitation(value: Json): ReceivedInvitation {
  const o = rec(value, 'received invitation');
  return {
    id: str(o, 'id'),
    workspace_id: str(o, 'workspace_id'),
    workspace_name: str(o, 'workspace_name'),
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

const none = (): undefined => undefined;

async function request(
  method: string,
  path: string,
  body?: Json,
): Promise<{ status: number; json: Json }> {
  let response: Response;
  try {
    const init: RequestInit = {
      method,
      credentials: 'same-origin',
      headers: body === undefined ? {} : { 'Content-Type': 'application/json' },
    };
    if (body !== undefined) init.body = JSON.stringify(body);
    response = await fetch(path, init);
  } catch (error: unknown) {
    throw new NetworkError(error instanceof Error ? error.message : 'network error');
  }
  const text = await response.text();
  let json: Json = null;
  if (text !== '') {
    try {
      json = JSON.parse(text);
    } catch {
      json = null;
    }
  }
  return { status: response.status, json };
}

function fail(status: number, json: Json): never {
  if (isRecord(json) && typeof json.error === 'string') {
    const message = typeof json.message === 'string' ? json.message : json.error;
    throw new ApiError(status, json.error, message);
  }
  throw new ApiError(status, 'unexpected', `unexpected response (${String(status)})`);
}

async function expectOk<T>(
  method: string,
  path: string,
  parse: (v: Json) => T,
  body?: Json,
): Promise<T> {
  const { status, json } = await request(method, path, body);
  if (status < 200 || status >= 300) fail(status, json);
  return parse(json);
}

export const api = {
  health: () => expectOk('GET', '/healthz', parseHealth),
  authInfo: () =>
    expectOk('GET', '/api/auth-info', (v): AuthInfo => ({
      oidc_enabled: bool(rec(v, 'auth info'), 'oidc_enabled'),
    })),
  /** 로그인하지 않았으면 undefined. */
  async me(): Promise<Me | undefined> {
    const { status, json } = await request('GET', '/api/me');
    if (status === 401) return undefined;
    if (status !== 200) fail(status, json);
    return parseMe(json);
  },
  async logout(): Promise<string> {
    return expectOk('POST', '/auth/logout', (v) => str(rec(v, 'logout'), 'redirect_to'));
  },
  workspaces: () => expectOk('GET', '/api/workspaces', (v) => parseList(v, parseWorkspace)),
  createWorkspace: (requestId: string, name: string) =>
    expectOk('POST', '/api/workspaces', parseWorkspace, { request_id: requestId, name }),
  wikiDocuments: (workspaceId: string) =>
    expectOk('GET', `/api/workspaces/${encodeURIComponent(workspaceId)}/wiki/documents`, (v) =>
      parseList(v, parseSummary),
    ),
  createWikiDocument: (workspaceId: string, requestId: string, title: string) =>
    expectOk(
      'POST',
      `/api/workspaces/${encodeURIComponent(workspaceId)}/wiki/documents`,
      parseDocument,
      {
        request_id: requestId,
        title,
      },
    ),
  document: (id: string) =>
    expectOk('GET', `/api/documents/${encodeURIComponent(id)}`, parseDocument),
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
  invitations: (ws: string) =>
    expectOk('GET', `/api/workspaces/${encodeURIComponent(ws)}/invitations`, (v) =>
      parseList(v, parseInvitation),
    ),
  invite: (ws: string, email: string, newRole: Role) =>
    expectOk('POST', `/api/workspaces/${encodeURIComponent(ws)}/invitations`, parseInvitation, {
      email,
      role: newRole,
    }),
  revokeInvitation: (id: string) =>
    expectOk('DELETE', `/api/invitations/${encodeURIComponent(id)}`, none),
  myInvitations: () =>
    expectOk('GET', '/api/invitations', (v) => parseList(v, parseReceivedInvitation)),
  acceptInvitation: (id: string) =>
    expectOk('POST', `/api/invitations/${encodeURIComponent(id)}/accept`, (v) =>
      str(rec(v, 'accepted'), 'workspace_id'),
    ),
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
  myNotes: () => expectOk('GET', '/api/me/notes', (v) => parseList(v, parseSummary)),
  createNote: (requestId: string, title: string) =>
    expectOk('POST', '/api/me/notes', parseDocument, { request_id: requestId, title }),
  sharedWithMe: () => expectOk('GET', '/api/me/shared', (v) => parseList(v, parseSharedView)),
  setSettings: (id: string, visibility: 'workspace' | 'private', allowCopy: boolean) =>
    expectOk('PUT', `/api/documents/${encodeURIComponent(id)}/settings`, parseDocument, {
      visibility,
      allow_copy: allowCopy,
    }),
  shares: (id: string) =>
    expectOk('GET', `/api/documents/${encodeURIComponent(id)}/shares`, (v) =>
      parseList(v, parseShareView),
    ),
  share: (id: string, email: string, perm: 'read' | 'write') =>
    expectOk('POST', `/api/documents/${encodeURIComponent(id)}/shares`, parseShareView, {
      email,
      permission: perm,
    }),
  unshare: (shareId: string) =>
    expectOk('DELETE', `/api/document-shares/${encodeURIComponent(shareId)}`, none),
  copyDocument: (id: string, requestId: string, targetWorkspaceId: string | null, title: string) =>
    expectOk('POST', `/api/documents/${encodeURIComponent(id)}/copy`, parseDocument, {
      request_id: requestId,
      target_workspace_id: targetWorkspaceId,
      title,
    }),
  movePreview: (id: string, targetWorkspaceId: string) =>
    expectOk('POST', `/api/documents/${encodeURIComponent(id)}/move/preview`, parseMovePreview, {
      target_workspace_id: targetWorkspaceId,
    }),
  /** 확인한 미리보기와 함께 이동한다. 그사이 접근 변화가 바뀌면 `{ kind: 'changed', preview }`. */
  async moveDocument(
    id: string,
    targetWorkspaceId: string,
    acknowledged: MovePreviewView,
  ): Promise<
    { kind: 'moved'; outcome: MoveOutcomeView } | { kind: 'changed'; preview: MovePreviewView }
  > {
    const { status, json } = await request(
      'POST',
      `/api/documents/${encodeURIComponent(id)}/move`,
      {
        target_workspace_id: targetWorkspaceId,
        acknowledged: { gains: acknowledged.gains, loses: acknowledged.loses },
      },
    );
    if (status === 409 && isRecord(json) && 'gains' in json) {
      return { kind: 'changed', preview: parseMovePreview(json) };
    }
    if (status !== 200) fail(status, json);
    return { kind: 'moved', outcome: parseMoveOutcome(json) };
  },
  /** 저장. 충돌이면 `{ kind: 'conflict', current }`. */
  async saveDocument(
    id: string,
    input: { requestId: string; baseRevision: number; title: string; body: DocNode },
  ): Promise<
    { kind: 'saved'; document: DocumentView } | { kind: 'conflict'; current: DocumentView }
  > {
    const { status, json } = await request('PUT', `/api/documents/${encodeURIComponent(id)}`, {
      request_id: input.requestId,
      base_revision: input.baseRevision,
      title: input.title,
      body: input.body,
    });
    if (status === 409) {
      return { kind: 'conflict', current: parseDocument(rec(json, 'conflict').current) };
    }
    if (status !== 200) fail(status, json);
    return { kind: 'saved', document: parseDocument(json) };
  },
};

/** 요청 ID. 재시도 때 같은 값을 다시 보내면 서버가 중복을 막는다. */
export function newRequestId(): string {
  return crypto.randomUUID();
}

export type HealthResult =
  | { kind: 'ok'; health: Health }
  | { kind: 'degraded'; health: Health }
  | { kind: 'unreachable'; message: string };

/** 상태 확인. 503도 형태가 맞으면 degraded로 본다. */
export async function fetchHealth(fetchImpl: typeof fetch = fetch): Promise<HealthResult> {
  let response: Response;
  try {
    response = await fetchImpl('/healthz');
  } catch (error: unknown) {
    const message = error instanceof Error ? error.message : 'network error';
    return { kind: 'unreachable', message };
  }
  const body: unknown = await response.json().catch(() => undefined);
  let health: Health;
  try {
    health = parseHealth(body);
  } catch {
    return { kind: 'unreachable', message: `unexpected response (${String(response.status)})` };
  }
  return response.ok ? { kind: 'ok', health } : { kind: 'degraded', health };
}
