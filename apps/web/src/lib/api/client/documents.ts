import { EXPECTED_USER, request, expectOk, fail } from '$lib/shared/api';
import { access, bool, int, optStr, parseList, rec, str, type Json } from '$lib/shared/api/parse';
import type { Share, SharePermission, ShareRecipient, ShareRecipientRole } from '@fvoci/api-client';
import { none, visibility } from './common';

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
  /** 트리의 부모. 없거나 볼 수 없으면 null. */
  parent_id: string | null;
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
  /** 공유·공개 링크·설정을 바꿀 수 있는지. 보관된 항목의 문서도 워크스페이스 관리자는 참(D-113 ⑯). */
  can_manage: boolean;
  /** 휴지통으로 옮길 수 있는지. */
  can_delete: boolean;
  /** 템플릿으로 표시됨(D-27). */
  is_template: boolean;
  /** 내보낼 수 있는지(D-75). 첨부 내려받기와 같은 규칙. */
  can_export: boolean;
  /**
   * 단건 GET 응답의 강한 ETag(D-77). If-None-Match 재검증에만 쓴다.
   */
  etag?: string;
}

function withEtag(doc: DocumentView, etag: string | null | undefined): DocumentView {
  return typeof etag === 'string' && etag !== '' ? { ...doc, etag } : doc;
}

export function parseDocNode(value: unknown): DocNode {
  const o = rec(value, 'document body');
  const type = o.type;
  if (type !== 'doc') throw new TypeError('document body must have type "doc"');
  return { ...o, type };
}

// --- 항목별 업무 상태 정의 (D-116) ----------------------------------------

// --- 내 업무 (D-120) ---------------------------------------------------

export function parseSummary(value: Json): SummaryView {
  const o = rec(value, 'document summary');
  return {
    id: str(o, 'id'),
    reference: optStr(o, 'reference'),
    title: str(o, 'title'),
    revision: int(o, 'revision'),
    updated_at: str(o, 'updated_at'),
    parent_id: optStr(o, 'parent_id'),
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
    parent_id: optStr(o, 'parent_id'),
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
    can_delete: bool(o, 'can_delete'),
    is_template: bool(o, 'is_template'),
    can_export: bool(o, 'can_export'),
  };
}

export interface SummaryView {
  id: string;
  reference: string | null;
  title: string;
  revision: number;
  updated_at: string;
  /** 트리의 부모. 없거나 볼 수 없으면 null(최상위로 보인다). */
  parent_id: string | null;
}

export interface SharedView {
  id: string;
  title: string;
  permission: 'read' | 'write';
  shared_by: string;
  updated_at: string;
}

/** 옵션인 nullable 응답 칸은 런타임 파서가 null로 통일한다. */
interface ShareIdentity {
  email: string | null;
  recipient_role: ShareRecipientRole | null;
}

export type ShareView = Pick<Share, 'id' | 'display_name' | 'permission' | 'max_permission'> &
  ShareIdentity;

export type ShareRecipientView = Omit<ShareRecipient, 'email' | 'recipient_role'> & ShareIdentity;

/** 이동 전후의 사용자별 접근 등급 변화(D-114). `gain`이면 from_grade가, `lose`면 to_grade가 null이다. */
export interface MoveAccessChangeView {
  /** `gain`·`lose`·`change`(서버 `app_move_preview`). */
  change: string;
  user_id: string;
  display_name: string;
  from_grade: string | null;
  to_grade: string | null;
}

export interface MovePreviewView {
  changes: MoveAccessChangeView[];
  /** 이전 리비전 기록과 댓글, 첨부 파일(이전 버전 포함)도 함께 옮겨진다. */
  history_moves_with_document: boolean;
  /** 대상에서 번호가 새로 매겨지는지(같은 모듈 안 이동이면 유지). */
  reference_changes: boolean;
  /** 이동해도 유지되는 활성 공개 링크 수. */
  active_public_links: number;
  /** 원래 자리에 남는 하위 문서 수(볼 수 있는 것). */
  children_left_behind: number;
}

export interface MoveOutcomeView {
  workspace_id: string;
  reference: string;
  /** 옮긴 뒤 내가 볼 수 없으면 null. */
  document: DocumentView | null;
}

function permission(value: string): SharePermission {
  if (value === 'read' || value === 'write') return value;
  throw new TypeError('unknown permission');
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
  return {
    id: str(o, 'id'),
    email: optStr(o, 'email'),
    display_name: str(o, 'display_name'),
    permission: permission(str(o, 'permission')),
    recipient_role: recipientRole(optStr(o, 'recipient_role')),
    max_permission: permission(str(o, 'max_permission')),
  };
}

function recipientRole(value: string | null): ShareRecipientRole | null {
  if (
    value === null ||
    value === 'owner' ||
    value === 'admin' ||
    value === 'member' ||
    value === 'readonly' ||
    value === 'outside_member'
  )
    return value;
  throw new TypeError('unknown share recipient role');
}

function parseShareRecipient(value: Json): ShareRecipientView {
  const o = rec(value, 'share recipient');
  return {
    user_id: str(o, 'user_id'),
    email: optStr(o, 'email'),
    display_name: str(o, 'display_name'),
    recipient_role: recipientRole(optStr(o, 'recipient_role')),
    max_permission: permission(str(o, 'max_permission')),
  };
}

function parseMoveAccessChange(value: Json): MoveAccessChangeView {
  const o = rec(value, 'move access change');
  return {
    change: str(o, 'change'),
    user_id: str(o, 'user_id'),
    display_name: str(o, 'display_name'),
    from_grade: optStr(o, 'from_grade'),
    to_grade: optStr(o, 'to_grade'),
  };
}

export function parseMovePreview(value: Json): MovePreviewView {
  const o = rec(value, 'move preview');
  return {
    changes: parseList(o.changes, parseMoveAccessChange),
    history_moves_with_document: bool(o, 'history_moves_with_document'),
    reference_changes: bool(o, 'reference_changes'),
    active_public_links: int(o, 'active_public_links'),
    children_left_behind: int(o, 'children_left_behind'),
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

/** 이동 대상: 워크스페이스 위키, 또는 그 워크스페이스의 항목. */
export interface MoveTargetInput {
  workspaceId: string;
  itemId: string | null;
}

export const documentsApi = {
  wikiDocuments: (workspaceId: string) =>
    expectOk('GET', `/api/workspaces/${encodeURIComponent(workspaceId)}/wiki/documents`, (v) =>
      parseList(v, parseSummary),
    ),
  createWikiDocument: (
    workspaceId: string,
    requestId: string,
    title: string,
    parentId: string | null = null,
    templateId: string | null = null,
  ) =>
    expectOk(
      'POST',
      `/api/workspaces/${encodeURIComponent(workspaceId)}/wiki/documents`,
      parseDocument,
      { request_id: requestId, title, parent_id: parentId, template_id: templateId },
    ),
  /** 트리에서 부모·순서를 바꾼다. parentId가 null이면 최상위, beforeId가 있으면 그 형제 바로 앞. */
  setParent: (id: string, parentId: string | null, beforeId: string | null) =>
    expectOk('PUT', `/api/documents/${encodeURIComponent(id)}/parent`, parseDocument, {
      parent_id: parentId,
      before_id: beforeId,
    }),
  /** 문서 한 건. 브라우저 캐시가 ETag로 서버에 재검증하고(304면 저장본), 응답의 ETag를 문서에 붙인다(D-77). */
  async document(this: void, id: string): Promise<DocumentView> {
    const { status, json, etag } = await request('GET', `/api/documents/${encodeURIComponent(id)}`);
    if (status !== 200) fail(status, json);
    return withEtag(parseDocument(json), etag);
  },
  myNotes: () => expectOk('GET', '/api/me/notes', (v) => parseList(v, parseSummary)),
  /** 내 노트 만들기. `expectedUser`는 이 만들기를 시작한 화면의 사용자 id(서버가 세션과 다르면 409 `account_changed`). */
  async createNote(
    this: void,
    expectedUser: string,
    requestId: string,
    title: string,
    parentId: string | null = null,
    templateId: string | null = null,
  ): Promise<DocumentView> {
    const { status, json } = await request(
      'POST',
      '/api/me/notes',
      { request_id: requestId, title, parent_id: parentId, template_id: templateId },
      { [EXPECTED_USER]: expectedUser },
    );
    if (status < 200 || status >= 300) fail(status, json);
    return parseDocument(json);
  },
  /** 공유받은 문서는 현재 조직에서 받은 것만(D-111). */
  sharedWithMe: (organizationId: string) =>
    expectOk('GET', `/api/me/shared?organization_id=${encodeURIComponent(organizationId)}`, (v) =>
      parseList(v, parseSharedView),
    ),
  shares: (id: string) =>
    expectOk('GET', `/api/documents/${encodeURIComponent(id)}/shares`, (v) =>
      parseList(v, parseShareView),
    ),
  shareRecipient: (id: string, email: string, organizationId: string) => {
    const query = new URLSearchParams({ email, organization_id: organizationId });
    return expectOk(
      'GET',
      `/api/documents/${encodeURIComponent(id)}/share-recipient?${query.toString()}`,
      parseShareRecipient,
    );
  },
  share: (id: string, email: string, perm: SharePermission, organizationId: string) =>
    expectOk('POST', `/api/documents/${encodeURIComponent(id)}/shares`, parseShareView, {
      email,
      permission: perm,
      organization_id: organizationId,
    }),
  unshare: (shareId: string) =>
    expectOk('DELETE', `/api/document-shares/${encodeURIComponent(shareId)}`, none),
  copyDocument: (id: string, requestId: string, targetWorkspaceId: string | null, title: string) =>
    expectOk('POST', `/api/documents/${encodeURIComponent(id)}/copy`, parseDocument, {
      request_id: requestId,
      target_workspace_id: targetWorkspaceId,
      title,
    }),
  /** 쓸 수 있는 문서 템플릿(D-27): 그 워크스페이스의 것과 내 개인 노트 템플릿. */
  documentTemplates: (workspaceId: string | null) =>
    expectOk(
      'GET',
      workspaceId === null
        ? '/api/document-templates'
        : `/api/document-templates?workspace_id=${encodeURIComponent(workspaceId)}`,
      (v) =>
        parseList(v, (t) => {
          const o = rec(t, 'document template');
          return {
            id: str(o, 'id'),
            title: str(o, 'title'),
            reference: optStr(o, 'reference'),
            personal: bool(o, 'personal'),
          };
        }),
    ),
  setDocumentTemplate: (id: string, template: boolean) =>
    expectOk('PUT', `/api/documents/${encodeURIComponent(id)}/template`, none, { template }),
  itemDocuments: (itemId: string) =>
    expectOk('GET', `/api/items/${encodeURIComponent(itemId)}/documents`, (v) =>
      parseList(v, parseSummary),
    ),
  createItemDocument: (
    itemId: string,
    requestId: string,
    title: string,
    parentId: string | null = null,
    templateId: string | null = null,
  ) =>
    expectOk('POST', `/api/items/${encodeURIComponent(itemId)}/documents`, parseDocument, {
      request_id: requestId,
      title,
      parent_id: parentId,
      template_id: templateId,
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
};
