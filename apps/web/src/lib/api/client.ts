// API 클라이언트. 타입은 계약에서 생성한 것(@fvoci/api-client)을 쓰고, 응답은 unknown으로 받아
// 실제 형태를 확인한다(생성 타입은 런타임 검증을 대신하지 않는다 — docs/development.md 3절).
import type {
  AuthInfo,
  Group,
  Health,
  Invitation,
  Item,
  Me,
  Member,
  MembersView,
  Module,
  ModuleKind,
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
  placement: 'wiki' | 'module' | 'personal';
  /** 모듈 항목 아래 문서면 그 항목. */
  item_id: string | null;
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
  /** 노출을 줄이는 관리(공유·공개 링크 철회, 비공개 전환)를 할 수 있는지. 보관 중에도 소유 권한자는 참. */
  can_manage: boolean;
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
  if (placement !== 'wiki' && placement !== 'module' && placement !== 'personal') {
    throw new TypeError('unknown placement');
  }
  const workspaceId = o.workspace_id ?? null;
  const reference = o.reference ?? null;
  if (workspaceId !== null && typeof workspaceId !== 'string') throw new TypeError('workspace_id');
  if (reference !== null && typeof reference !== 'string') throw new TypeError('reference');
  return {
    id: str(o, 'id'),
    workspace_id: workspaceId,
    item_id: optStr(o, 'item_id'),
    placement,
    reference,
    title: str(o, 'title'),
    body: parseDocNode(o.body),
    schema_version: int(o, 'schema_version'),
    revision: int(o, 'revision'),
    updated_at: str(o, 'updated_at'),
    access: access(str(o, 'access')),
    can_edit: bool(o, 'can_edit'),
    visibility: visibility(str(o, 'visibility')),
    allow_copy: bool(o, 'allow_copy'),
    can_manage: bool(o, 'can_manage'),
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
  /** 대상에서 번호가 새로 매겨지는지(같은 모듈 안 이동이면 유지). */
  reference_changes: boolean;
  /** 이동해도 유지되는 활성 공개 링크 수. */
  active_public_links: number;
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
    reference_changes: bool(o, 'reference_changes'),
    active_public_links: int(o, 'active_public_links'),
  };
}

export interface PublicLinkView {
  id: string;
  created_at: string;
  expires_at: string | null;
  active: boolean;
}

export interface PublicDocumentView {
  title: string;
  body: DocNode;
  updated_at: string;
}

export function parsePublicLink(value: Json): PublicLinkView {
  const o = rec(value, 'public link');
  return {
    id: str(o, 'id'),
    created_at: str(o, 'created_at'),
    expires_at: optStr(o, 'expires_at'),
    active: bool(o, 'active'),
  };
}

export function parsePublicDocument(value: Json): PublicDocumentView {
  const o = rec(value, 'public document');
  return { title: str(o, 'title'), body: parseDocNode(o.body), updated_at: str(o, 'updated_at') };
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

export const MODULE_KINDS: readonly ModuleKind[] = [
  'project',
  'research',
  'study',
  'course',
  'hobby',
] as const;

function moduleKind(value: string): ModuleKind {
  const kind = MODULE_KINDS.find((k) => k === value);
  if (kind === undefined) throw new TypeError(`unknown module kind ${value}`);
  return kind;
}

function access(value: string): Access {
  if (value === 'owner' || value === 'write' || value === 'read') return value;
  throw new TypeError('unknown access');
}

function visibility(value: string): 'workspace' | 'private' {
  if (value === 'workspace' || value === 'private') return value;
  throw new TypeError('unknown visibility');
}

export function parseModule(value: Json): Module {
  const o = rec(value, 'module');
  return {
    id: str(o, 'id'),
    workspace_id: str(o, 'workspace_id'),
    kind: moduleKind(str(o, 'kind')),
    name: str(o, 'name'),
    key: str(o, 'key'),
    previous_keys: strings(o.previous_keys),
    archived: bool(o, 'archived'),
  };
}

/** 화면에서 쓰는 항목. 공개 범위를 좁힌 것 외에는 계약의 Item과 같다. */
export type ItemView = Item & { visibility: 'workspace' | 'private' };

export function parseItem(value: Json): ItemView {
  const o = rec(value, 'item');
  return {
    id: str(o, 'id'),
    module_id: str(o, 'module_id'),
    workspace_id: str(o, 'workspace_id'),
    reference: optStr(o, 'reference'),
    title: str(o, 'title'),
    description: str(o, 'description'),
    visibility: visibility(str(o, 'visibility')),
    access: access(str(o, 'access')),
    archived: bool(o, 'archived'),
    updated_at: str(o, 'updated_at'),
  };
}

// --- @ 참조 -------------------------------------------------------------

export type ReferenceKind = 'item' | 'document' | 'user';

function referenceKind(value: string): ReferenceKind {
  if (value === 'item' || value === 'document' || value === 'user') return value;
  throw new TypeError(`unknown reference kind ${value}`);
}

export interface ReferenceCandidateView {
  kind: ReferenceKind;
  id: string;
  title: string;
  reference: string | null;
  workspace_id: string | null;
  workspace_name: string | null;
}

/** 해석 결과. 볼 수 없거나 없는 대상은 visible=false이고 나머지는 null이다. */
export interface ResolvedReferenceView {
  kind: ReferenceKind;
  id: string;
  visible: boolean;
  title: string | null;
  reference: string | null;
  workspace_id: string | null;
  item_id: string | null;
}

export interface BacklinkView {
  id: string;
  reference: string | null;
  title: string;
  workspace_id: string | null;
  item_id: string | null;
}

export interface NotificationView {
  id: string;
  kind: string;
  document_id: string;
  document_title: string;
  document_reference: string | null;
  workspace_id: string | null;
  item_id: string | null;
  /** 워크스페이스를 함께 쓰지 않으면 없을 수 있다. */
  actor_name: string | null;
  created_at: string;
  read: boolean;
}

export function parseCandidate(value: Json): ReferenceCandidateView {
  const o = rec(value, 'reference candidate');
  return {
    kind: referenceKind(str(o, 'kind')),
    id: str(o, 'id'),
    title: str(o, 'title'),
    reference: optStr(o, 'reference'),
    workspace_id: optStr(o, 'workspace_id'),
    workspace_name: optStr(o, 'workspace_name'),
  };
}

export function parseResolved(value: Json): ResolvedReferenceView {
  const o = rec(value, 'resolved reference');
  return {
    kind: referenceKind(str(o, 'kind')),
    id: str(o, 'id'),
    visible: bool(o, 'visible'),
    title: optStr(o, 'title'),
    reference: optStr(o, 'reference'),
    workspace_id: optStr(o, 'workspace_id'),
    item_id: optStr(o, 'item_id'),
  };
}

export function parseBacklink(value: Json): BacklinkView {
  const o = rec(value, 'backlink');
  return {
    id: str(o, 'id'),
    reference: optStr(o, 'reference'),
    title: str(o, 'title'),
    workspace_id: optStr(o, 'workspace_id'),
    item_id: optStr(o, 'item_id'),
  };
}

export function parseNotification(value: Json): NotificationView {
  const o = rec(value, 'notification');
  return {
    id: str(o, 'id'),
    kind: str(o, 'kind'),
    document_id: str(o, 'document_id'),
    document_title: str(o, 'document_title'),
    document_reference: optStr(o, 'document_reference'),
    workspace_id: optStr(o, 'workspace_id'),
    item_id: optStr(o, 'item_id'),
    actor_name: optStr(o, 'actor_name'),
    created_at: str(o, 'created_at'),
    read: bool(o, 'read'),
  };
}

/** 이동 대상: 워크스페이스 위키, 또는 그 워크스페이스의 항목. */
export interface MoveTargetInput {
  workspaceId: string;
  itemId: string | null;
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
  movePreview: (id: string, target: MoveTargetInput) =>
    expectOk('POST', `/api/documents/${encodeURIComponent(id)}/move/preview`, parseMovePreview, {
      target_workspace_id: target.workspaceId,
      target_item_id: target.itemId,
    }),
  referenceCandidates: (q: string, workspaceId: string | null) => {
    const params = new URLSearchParams({ q });
    if (workspaceId !== null) params.set('workspace_id', workspaceId);
    return expectOk('GET', `/api/reference-candidates?${params.toString()}`, (v) =>
      parseList(v, parseCandidate),
    );
  },
  resolveReferences: (refs: { kind: ReferenceKind; id: string }[]) =>
    expectOk('POST', '/api/references/resolve', (v) => parseList(v, parseResolved), { refs }),
  documentBacklinks: (id: string) =>
    expectOk('GET', `/api/documents/${encodeURIComponent(id)}/backlinks`, (v) =>
      parseList(v, parseBacklink),
    ),
  itemBacklinks: (id: string) =>
    expectOk('GET', `/api/items/${encodeURIComponent(id)}/backlinks`, (v) =>
      parseList(v, parseBacklink),
    ),
  notifications: () =>
    expectOk('GET', '/api/me/notifications', (v) => parseList(v, parseNotification)),
  /** id가 null이면 모두 읽음. */
  markNotificationsRead: (id: string | null) =>
    expectOk('POST', '/api/me/notifications/read', none, { id }),
  modules: (ws: string) =>
    expectOk('GET', `/api/workspaces/${encodeURIComponent(ws)}/modules`, (v) =>
      parseList(v, parseModule),
    ),
  module: (id: string) => expectOk('GET', `/api/modules/${encodeURIComponent(id)}`, parseModule),
  createModule: (
    ws: string,
    input: { requestId: string; kind: ModuleKind; name: string; key: string },
  ) =>
    expectOk('POST', `/api/workspaces/${encodeURIComponent(ws)}/modules`, parseModule, {
      request_id: input.requestId,
      kind: input.kind,
      name: input.name,
      key: input.key,
    }),
  updateModule: (id: string, name: string, key: string) =>
    expectOk('PUT', `/api/modules/${encodeURIComponent(id)}`, parseModule, { name, key }),
  archiveModule: (id: string, archived: boolean) =>
    expectOk('PUT', `/api/modules/${encodeURIComponent(id)}/archived`, parseModule, { archived }),
  items: (moduleId: string) =>
    expectOk('GET', `/api/modules/${encodeURIComponent(moduleId)}/items`, (v) =>
      parseList(v, parseItem),
    ),
  createItem: (moduleId: string, requestId: string, title: string) =>
    expectOk('POST', `/api/modules/${encodeURIComponent(moduleId)}/items`, parseItem, {
      request_id: requestId,
      title,
    }),
  item: (id: string) => expectOk('GET', `/api/items/${encodeURIComponent(id)}`, parseItem),
  updateItem: (
    id: string,
    input: { title: string; description: string; visibility: 'workspace' | 'private' },
  ) => expectOk('PUT', `/api/items/${encodeURIComponent(id)}`, parseItem, input),
  archiveItem: (id: string, archived: boolean) =>
    expectOk('PUT', `/api/items/${encodeURIComponent(id)}/archived`, parseItem, { archived }),
  itemDocuments: (itemId: string) =>
    expectOk('GET', `/api/items/${encodeURIComponent(itemId)}/documents`, (v) =>
      parseList(v, parseSummary),
    ),
  createItemDocument: (itemId: string, requestId: string, title: string) =>
    expectOk('POST', `/api/items/${encodeURIComponent(itemId)}/documents`, parseDocument, {
      request_id: requestId,
      title,
    }),
  publicLinks: (id: string) =>
    expectOk('GET', `/api/documents/${encodeURIComponent(id)}/public-links`, (v) =>
      parseList(v, parsePublicLink),
    ),
  /** 링크 주소는 이때 한 번만 받는다. */
  createPublicLink: (id: string, expiresAt: string | null) =>
    expectOk(
      'POST',
      `/api/documents/${encodeURIComponent(id)}/public-links`,
      (v) => {
        const o = rec(v, 'created link');
        return { id: str(o, 'id'), url: str(o, 'url') };
      },
      expiresAt === null ? {} : { expires_at: expiresAt },
    ),
  revokePublicLink: (linkId: string) =>
    expectOk('DELETE', `/api/public-links/${encodeURIComponent(linkId)}`, none),
  publicDocument: (token: string) =>
    expectOk('GET', `/api/public/${encodeURIComponent(token)}`, parsePublicDocument),
  /** 확인한 미리보기와 함께 이동한다. 그사이 접근 변화가 바뀌면 `{ kind: 'changed', preview }`. */
  async moveDocument(
    id: string,
    target: MoveTargetInput,
    acknowledged: MovePreviewView,
  ): Promise<
    { kind: 'moved'; outcome: MoveOutcomeView } | { kind: 'changed'; preview: MovePreviewView }
  > {
    const { status, json } = await request(
      'POST',
      `/api/documents/${encodeURIComponent(id)}/move`,
      {
        target_workspace_id: target.workspaceId,
        target_item_id: target.itemId,
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
