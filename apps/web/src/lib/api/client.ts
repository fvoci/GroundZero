import { systemApi, readTrash } from '$lib/features/trash/api';
export type { TrashEntryView, TrashView } from '$lib/features/trash/api';
import { filesApi } from '$lib/features/files/api';
export { fileContentUrl } from '$lib/features/files/api';
export type { FileView, FileVersionView, UploadView, FileTarget } from '$lib/features/files/model';
import { parseEvent, type EventView } from '$lib/features/events/model';
import { timeData } from '$lib/features/time/data';
export { TIME_KINDS, parseTimeEntry } from '$lib/features/time/model';
export type {
  TimeKind,
  TimeTargetKind,
  TimeTargetView,
  TimeEntryView,
  TimeEntryInput,
  TimeSummaryRowView,
  TaskTimeView,
  ItemTimeView,
} from '$lib/features/time/model';
import { EXPECTED_USER, request, expectOk, fail } from '$lib/shared/api';
export { ApiError, NetworkError } from '$lib/shared/api';
import {
  access,
  bool,
  int,
  isRecord,
  optInt,
  optStr,
  parseList,
  rec,
  statusCategory,
  str,
  strings,
  type Json,
} from '$lib/shared/api/parse';
export { statusCategory, parseTaskStatusDefinition } from '$lib/shared/api/parse';
import { type TaskStatus } from '$lib/features/tasks/model';
export * from '$lib/features/tasks/model';
// API 클라이언트. 타입은 계약에서 생성한 것(@fvoci/api-client)을 쓰고, 응답은 unknown으로 받아
// 실제 형태를 확인한다(생성 타입은 런타임 검증을 대신하지 않는다 — D-09 docs/decisions/delivery.md#d-09).
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
import type {
  Accepted,
  MyTask,
  MyTaskCounts,
  Organization,
  OrganizationLimits,
  OrganizationSummary,
  OrgRole,
  StatusCategory,
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
  /** 실시간 공동 편집 중(D-70). 본문은 실시간 연결로 고치고 저장 API는 제목만 바꾼다. */
  realtime: boolean;
  /** 내보낼 수 있는지(D-75). 첨부 내려받기와 같은 규칙. */
  can_export: boolean;
  /**
   * 단건 조회·저장·복원·충돌 응답이 준 ETag(D-77). 저장 때 `If-Match`로 보낸다. 다른 경로(목록·설정 변경)에서 받은
   * 문서에는 없다.
   */
  etag?: string;
}

/** 강한 ETag인지. CDN 등이 압축하며 약하게(`W/`) 바꾼 값은 `If-Match`에 쓰지 않는다(약한 값은 맞지 않음, D-77). */
export function isStrongEtag(etag: string | undefined): etag is string {
  return etag !== undefined && etag.startsWith('"') && etag.endsWith('"') && etag.length >= 2;
}

function withEtag(doc: DocumentView, etag: string | null | undefined): DocumentView {
  return typeof etag === 'string' && etag !== '' ? { ...doc, etag } : doc;
}

/** 409(기준 리비전이 지남)·412(`If-Match`가 맞지 않음)의 서버 최신본. 둘 다 초안은 클라이언트가 보존한다. */
function conflictCurrent(status: number, json: Json): DocumentView | undefined {
  if ((status !== 409 && status !== 412) || !isRecord(json) || json.current === undefined) {
    return undefined;
  }
  const body = rec(json, 'conflict');
  return withEtag(parseDocument(body.current), typeof body.etag === 'string' ? body.etag : null);
}

function role(value: string): Role {
  if (value === 'admin' || value === 'member' || value === 'readonly') return value;
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
  const build = rec(o.build, 'build');
  return {
    status: str(o, 'status'),
    database: str(o, 'database'),
    build: { version: str(build, 'version'), source: str(build, 'source') },
  };
}

export function parseMe(value: Json): Me {
  const o = rec(value, 'me');
  const email = o.email;
  return {
    id: str(o, 'id'),
    display_name: str(o, 'display_name'),
    email: typeof email === 'string' ? email : null,
    time_zone: typeof o.time_zone === 'string' ? o.time_zone : '',
    locale: typeof o.locale === 'string' ? o.locale : '',
    week_start: typeof o.week_start === 'string' ? o.week_start : '',
    hour_cycle: typeof o.hour_cycle === 'string' ? o.hour_cycle : '',
    theme: o.theme === 'light' || o.theme === 'dark' ? o.theme : 'system',
    last_organization_id: optStr(o, 'last_organization_id'),
    limits: parseInputLimits(rec(o.limits, 'me.limits')),
  };
}

/** 서버 입력 한도(#79). 계약상 필수이며 네 칸 모두 정수다. */
function parseInputLimits(o: Record<string, unknown>): Me['limits'] {
  return {
    max_title_chars: int(o, 'max_title_chars'),
    max_document_bytes: int(o, 'max_document_bytes'),
    max_references_per_document: int(o, 'max_references_per_document'),
    max_document_depth: int(o, 'max_document_depth'),
  };
}

export function parseWorkspace(value: Json): Workspace {
  const o = rec(value, 'workspace');
  return {
    id: str(o, 'id'),
    organization_id: str(o, 'organization_id'),
    name: str(o, 'name'),
    role: role(str(o, 'role')),
    is_owner: bool(o, 'is_owner'),
    realtime_enabled: bool(o, 'realtime_enabled'),
    access: access(str(o, 'access')),
  };
}

// --- 조직 (D-111·D-112·D-124) --------------------------------------------

function orgRole(value: string): OrgRole {
  if (value === 'admin' || value === 'member') return value;
  throw new TypeError(`unknown organization role ${value}`);
}

export function parseOrganizationSummary(value: Json): OrganizationSummary {
  const o = rec(value, 'organization');
  return {
    id: str(o, 'id'),
    name: str(o, 'name'),
    role: orgRole(str(o, 'role')),
    public_links_enabled: bool(o, 'public_links_enabled'),
    created_at: str(o, 'created_at'),
  };
}

export function parseOrganizationLimits(value: Json): OrganizationLimits {
  const o = rec(value, 'organization limits');
  return {
    max_file_bytes: optInt(o, 'max_file_bytes'),
    max_storage_bytes: optInt(o, 'max_storage_bytes'),
    max_concurrent_uploads: optInt(o, 'max_concurrent_uploads'),
    notification_rate_per_hour: optInt(o, 'notification_rate_per_hour'),
    activity_retention_days: optInt(o, 'activity_retention_days'),
    audit_retention_days: optInt(o, 'audit_retention_days'),
  };
}

export function parseOrganization(value: Json): Organization {
  const o = rec(value, 'organization');
  return {
    ...parseOrganizationSummary(value),
    limits: parseOrganizationLimits(o.limits),
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

// --- 항목별 업무 상태 정의 (D-116) ----------------------------------------

// --- 내 업무 (D-120) ---------------------------------------------------

export type MyTaskBucket = 'overdue' | 'today' | 'upcoming' | 'none' | 'done';

export interface MyTaskView extends Omit<MyTask, 'bucket' | 'status_category'> {
  bucket: MyTaskBucket;
  status_category: StatusCategory;
}

export interface MyTasksView {
  counts: MyTaskCounts;
  workspaces: { workspace_id: string; workspace_name: string; tasks: MyTaskView[] }[];
}

function myTaskBucket(value: string): MyTaskBucket {
  if (
    value === 'overdue' ||
    value === 'today' ||
    value === 'upcoming' ||
    value === 'none' ||
    value === 'done'
  )
    return value;
  throw new TypeError(`unknown bucket ${value}`);
}

function parseMyTask(value: Json): MyTaskView {
  const o = rec(value, 'my task');
  return {
    id: str(o, 'id'),
    item_id: str(o, 'item_id'),
    reference: optStr(o, 'reference'),
    title: str(o, 'title'),
    status: str(o, 'status'),
    status_category: statusCategory(str(o, 'status_category')),
    priority: str(o, 'priority'),
    start_date: optStr(o, 'start_date'),
    due_date: optStr(o, 'due_date'),
    bucket: myTaskBucket(str(o, 'bucket')),
    updated_at: str(o, 'updated_at'),
  };
}

export function parseMyTasks(value: Json): MyTasksView {
  const o = rec(value, 'my tasks');
  const c = rec(o.counts, 'my task counts');
  return {
    counts: {
      overdue: int(c, 'overdue'),
      today: int(c, 'today'),
      upcoming: int(c, 'upcoming'),
      undated: int(c, 'undated'),
      done: int(c, 'done'),
    },
    workspaces: parseList(o.workspaces, (v) => {
      const w = rec(v, 'workspace tasks');
      return {
        workspace_id: str(w, 'workspace_id'),
        workspace_name: str(w, 'workspace_name'),
        tasks: parseList(w.tasks, parseMyTask),
      };
    }),
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
    realtime: bool(o, 'realtime'),
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

export interface ShareView {
  id: string;
  email: string | null;
  display_name: string;
  permission: 'read' | 'write';
}

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

function permission(value: string): 'read' | 'write' {
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
    icon: str(o, 'icon'),
    description: str(o, 'description'),
    access: access(str(o, 'access')),
  };
}

/** 항목 상태(D-33): 계획·진행·보류·완료·취소. */
export const ITEM_STATUSES = ['planned', 'active', 'on_hold', 'done', 'cancelled'] as const;
export type ItemStatus = (typeof ITEM_STATUSES)[number];

function itemStatus(value: string): ItemStatus {
  const found = ITEM_STATUSES.find((s) => s === value);
  if (found === undefined) throw new TypeError(`unknown item status ${value}`);
  return found;
}

/** 화면에서 쓰는 항목. 공개 범위·상태를 좁힌 것 외에는 계약의 Item과 같다. */
export type ItemView = Item & { visibility: 'workspace' | 'private'; status: ItemStatus };

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
    status: itemStatus(str(o, 'status')),
    starts_on: optStr(o, 'starts_on'),
    ends_on: optStr(o, 'ends_on'),
    is_template: bool(o, 'is_template'),
    version: int(o, 'version'),
  };
}

/** 모듈 종류별 항목 정보(D-34 연구, D-36 스터디, D-38 취미). 종류에 없는 칸은 보내지 않는다. */
export interface ItemDetailsView {
  field?: string;
  keywords?: string[];
  question?: string;
  hypothesis?: string;
  model?: string;
  method?: string;
  sample?: string;
  level?: string;
  target?: string;
  activity_type?: string;
}

export interface PersonRefView {
  user_id: string;
  display_name: string;
}

export interface ItemProfileView {
  leader: PersonRefView | null;
  goal: string;
  scope: string;
  starts_on: string | null;
  ends_on: string | null;
  status: ItemStatus;
  details: ItemDetailsView;
  members: PersonRefView[];
  groups: { id: string; name: string }[];
  can_edit: boolean;
  can_manage_members: boolean;
  /** 항목 정보(리더·목표·범위·기간·상태·종류별 정보)의 판. 고칠 때 연 때의 판을 보낸다(D-104). */
  version: number;
}

const DETAIL_TEXT_KEYS = [
  'field',
  'question',
  'hypothesis',
  'model',
  'method',
  'sample',
  'level',
  'target',
  'activity_type',
] as const;

function parsePerson(value: Json): PersonRefView {
  const o = rec(value, 'person');
  return { user_id: str(o, 'user_id'), display_name: str(o, 'display_name') };
}

export function parseItemProfile(value: Json): ItemProfileView {
  const o = rec(value, 'item profile');
  const d = rec(o.details ?? {}, 'item details');
  const details: ItemDetailsView = {};
  for (const key of DETAIL_TEXT_KEYS) {
    const v = optStr(d, key);
    if (v !== null) details[key] = v;
  }
  if (d.keywords !== undefined) details.keywords = strings(d.keywords);
  return {
    leader: o.leader === null || o.leader === undefined ? null : parsePerson(o.leader),
    goal: str(o, 'goal'),
    scope: str(o, 'scope'),
    starts_on: optStr(o, 'starts_on'),
    ends_on: optStr(o, 'ends_on'),
    status: itemStatus(str(o, 'status')),
    details,
    members: parseList(o.members, parsePerson),
    groups: parseList(o.groups, (g) => {
      const r = rec(g, 'group');
      return { id: str(r, 'id'), name: str(r, 'name') };
    }),
    can_edit: bool(o, 'can_edit'),
    can_manage_members: bool(o, 'can_manage_members'),
    version: int(o, 'version'),
  };
}

// --- 항목 기록 (D-33·D-34·D-36·D-38) -------------------------------------

export type RecordFieldType =
  | 'text'
  | 'long_text'
  | 'date'
  | 'number'
  | 'choice'
  | 'url'
  | 'tags'
  | 'person'
  | 'people'
  | 'course'
  /** 이 강좌의 회차(D-123 강의 질문·복습). */
  | 'episode';

const RECORD_FIELD_TYPES: readonly RecordFieldType[] = [
  'text',
  'long_text',
  'date',
  'number',
  'choice',
  'url',
  'tags',
  'person',
  'people',
  'course',
  'episode',
];

export interface ChoiceView {
  value: string;
  label: string;
}

export interface RecordFieldDef {
  key: string;
  label: string;
  type: RecordFieldType;
  choices: ChoiceView[];
}

export interface RecordKindView {
  kind: string;
  label: string;
  module_kind: string;
  date_label: string | null;
  fields: RecordFieldDef[];
  file_roles: ChoiceView[];
}

/** 칸 값: 글자·수·날짜·고른 값·사람 id·사람 id 목록·태그 목록. */
export type RecordFieldValue = string | number | string[];

export interface RecordFileView {
  role: string;
  file_id: string | null;
  file_name: string;
  version: number;
  sha256: string;
  size: number;
  available: boolean;
}

export interface RecordView {
  id: string;
  kind: string;
  title: string;
  occurred_on: string | null;
  body: string;
  fields: Record<string, RecordFieldValue>;
  files: RecordFileView[];
  created_by: string;
  created_by_name: string | null;
  updated_at: string;
  version: number;
  can_edit: boolean;
  can_delete: boolean;
}

export interface RecordInput {
  title: string;
  occurred_on: string | null;
  body: string;
  fields: Record<string, RecordFieldValue>;
  files: { file_id: string; version: number | null; role: string }[];
}

function parseChoice(value: Json): ChoiceView {
  const o = rec(value, 'choice');
  return { value: str(o, 'value'), label: str(o, 'label') };
}

export function parseRecordKind(value: Json): RecordKindView {
  const o = rec(value, 'record kind');
  return {
    kind: str(o, 'kind'),
    label: str(o, 'label'),
    module_kind: str(o, 'module_kind'),
    date_label: optStr(o, 'date_label'),
    fields: parseList(o.fields, (f): RecordFieldDef => {
      const r = rec(f, 'record field');
      const type = RECORD_FIELD_TYPES.find((t) => t === r.type);
      if (type === undefined) throw new TypeError('unknown record field type');
      return {
        key: str(r, 'key'),
        label: str(r, 'label'),
        type,
        choices: parseList(r.choices, parseChoice),
      };
    }),
    file_roles: parseList(o.file_roles, parseChoice),
  };
}

function recordFieldValue(value: unknown): RecordFieldValue | undefined {
  if (typeof value === 'string' || typeof value === 'number') return value;
  if (Array.isArray(value) && value.every((v) => typeof v === 'string')) return value;
  return undefined;
}

export function parseRecord(value: Json): RecordView {
  const o = rec(value, 'record');
  const raw = rec(o.fields ?? {}, 'record fields');
  const fields: Record<string, RecordFieldValue> = {};
  for (const [key, v] of Object.entries(raw)) {
    const parsed = recordFieldValue(v);
    if (parsed !== undefined) fields[key] = parsed;
  }
  return {
    id: str(o, 'id'),
    kind: str(o, 'kind'),
    title: str(o, 'title'),
    occurred_on: optStr(o, 'occurred_on'),
    body: str(o, 'body'),
    fields,
    files: parseList(o.files, (f): RecordFileView => {
      const r = rec(f, 'record file');
      return {
        role: str(r, 'role'),
        file_id: optStr(r, 'file_id'),
        file_name: str(r, 'file_name'),
        version: int(r, 'version'),
        sha256: str(r, 'sha256'),
        size: int(r, 'size'),
        available: bool(r, 'available'),
      };
    }),
    created_by: str(o, 'created_by'),
    created_by_name: optStr(o, 'created_by_name'),
    updated_at: str(o, 'updated_at'),
    version: int(o, 'version'),
    can_edit: bool(o, 'can_edit'),
    can_delete: bool(o, 'can_delete'),
  };
}

// --- Zotero 읽기 (D-34) ------------------------------------------------------

export interface ZoteroLibraryInput {
  api_key: string;
  library_type: 'user' | 'group';
  library_id: string;
}

// --- 저장한 보기 (D-31) ------------------------------------------------------

// --- 스터디 계획 (D-36) ------------------------------------------------------

export const STUDY_UNIT_KINDS = ['pages', 'problems', 'minutes', 'items'] as const;
export type StudyUnitKind = (typeof STUDY_UNIT_KINDS)[number];

export interface StudyUnitView {
  id: string;
  number: number;
  title: string;
  chapter: string;
  unit: StudyUnitKind;
  amount: number;
  planned_on: string | null;
  mine: {
    status: EpisodeStatus;
    done_amount: number;
    planned_on: string | null;
    completed_on: string | null;
    note: string;
  };
  /** 공통 정보의 판. 고칠 때 읽은 판을 보낸다(그 사이 다른 사람이 고쳤으면 409). */
  version: number;
}

export interface StudyPlanView {
  units: StudyUnitView[];
  can_write: boolean;
}

export interface StudyParticipantView {
  user_id: string;
  display_name: string;
  done: number;
  in_progress: number;
  overdue: number;
  done_amount: number;
}

function studyUnitKind(value: string): StudyUnitKind {
  const found = STUDY_UNIT_KINDS.find((k) => k === value);
  if (found === undefined) throw new TypeError(`unknown study unit ${value}`);
  return found;
}

export function parseStudyPlan(value: Json): StudyPlanView {
  const o = rec(value, 'study plan');
  return {
    can_write: bool(o, 'can_write'),
    units: parseList(o.units, (u): StudyUnitView => {
      const r = rec(u, 'study unit');
      const m = rec(r.mine, 'unit progress');
      return {
        id: str(r, 'id'),
        number: int(r, 'number'),
        title: str(r, 'title'),
        chapter: str(r, 'chapter'),
        unit: studyUnitKind(str(r, 'unit')),
        amount: int(r, 'amount'),
        planned_on: optStr(r, 'planned_on'),
        mine: {
          status: episodeStatus(str(m, 'status')),
          done_amount: int(m, 'done_amount'),
          planned_on: optStr(m, 'planned_on'),
          completed_on: optStr(m, 'completed_on'),
          note: str(m, 'note'),
        },
        version: int(r, 'version'),
      };
    }),
  };
}

// --- @ 참조 -------------------------------------------------------------

export type ReferenceKind = 'item' | 'document' | 'task' | 'user';

function referenceKind(value: string): ReferenceKind {
  if (value === 'item' || value === 'document' || value === 'task' || value === 'user')
    return value;
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

// --- 리비전 -------------------------------------------------------------

export interface RevisionSummaryView {
  revision: number;
  title: string;
  author_name: string | null;
  created_at: string;
  restored_from: number | null;
  /**
   * 저장한 사람이 고친 사람 대신 남긴 리비전(실시간 편집, D-95)이면 실제로 고친 사람. 보통 리비전은 null, 기록이 없으면 빈
   * 목록. 이름을 볼 수 없는 사람은 null.
   */
  edited_by_names: (string | null)[] | null;
}

export interface RevisionView extends RevisionSummaryView {
  body: DocNode;
  schema_version: number;
}

/** 이름 목록(항목은 이름이나 null). 없으면 null. */
function optNameList(o: Record<string, unknown>, key: string): (string | null)[] | null {
  const v = o[key];
  if (v === null || v === undefined) return null;
  if (!Array.isArray(v)) throw new TypeError(`expected array or null at ${key}`);
  return v.map((name: unknown) => {
    if (name === null || typeof name === 'string') return name;
    throw new TypeError(`expected string or null in ${key}`);
  });
}

export function parseRevisionSummary(value: Json): RevisionSummaryView {
  const o = rec(value, 'revision');
  return {
    revision: int(o, 'revision'),
    title: str(o, 'title'),
    author_name: optStr(o, 'author_name'),
    created_at: str(o, 'created_at'),
    restored_from: optInt(o, 'restored_from'),
    edited_by_names: optNameList(o, 'edited_by_names'),
  };
}

export function parseRevision(value: Json): RevisionView {
  const o = rec(value, 'revision');
  return {
    ...parseRevisionSummary(o),
    body: parseDocNode(o.body),
    schema_version: int(o, 'schema_version'),
  };
}

export interface CalendarTaskView {
  id: string;
  item_id: string;
  reference: string | null;
  title: string;
  status: TaskStatus;
  status_category: StatusCategory;
  milestone: boolean;
  start_date: string | null;
  due_date: string | null;
}

export interface EventOccurrenceView {
  event_id: string;
  /** 일정 시간대 기준의 회차 날짜. */
  occurrence: string;
  start_date: string | null;
  end_date: string | null;
  starts_at: string | null;
  ends_at: string | null;
  /** 반복 회차의 현지 시각이 그 시간대에 없어 건너뜀(D-81). */
  skipped: boolean;
}

/** 아직 만들지 않은 반복 업무 회차(예정). */
export interface ProjectedTaskView {
  series_id: string;
  root_task_id: string;
  item_id: string;
  title: string;
  milestone: boolean;
  occurrence: string;
  start_date: string | null;
  due_date: string | null;
}

export const CALENDAR_DATE_KINDS = [
  'episode',
  'study_unit',
  'submission',
  'revision_due',
  'deliverable',
] as const;
export type CalendarDateKind = (typeof CALENDAR_DATE_KINDS)[number];

/** 업무·일정 밖의 날짜(D-80): 강의·스터디 계획일, 투고·산출물 날짜. */
export interface CalendarDateView {
  kind: CalendarDateKind;
  id: string;
  item_id: string;
  title: string;
  date: string;
  /** 내 계획일(본인만 봄). */
  personal: boolean;
  done: boolean;
}

export interface CalendarView {
  tasks: CalendarTaskView[];
  projected_tasks: ProjectedTaskView[];
  events: EventView[];
  occurrences: EventOccurrenceView[];
  dates: CalendarDateView[];
  items: { id: string; reference: string | null; title: string }[];
}

function parseCalendar(value: Json): CalendarView {
  const o = rec(value, 'calendar');
  return {
    tasks: parseList(o.tasks, (v): CalendarTaskView => {
      const t = rec(v, 'calendar task');
      return {
        id: str(t, 'id'),
        item_id: str(t, 'item_id'),
        reference: optStr(t, 'reference'),
        title: str(t, 'title'),
        status: str(t, 'status'),
        status_category: statusCategory(str(t, 'status_category')),
        milestone: bool(t, 'milestone'),
        start_date: optStr(t, 'start_date'),
        due_date: optStr(t, 'due_date'),
      };
    }),
    projected_tasks: parseList(o.projected_tasks, (v): ProjectedTaskView => {
      const p = rec(v, 'projected task');
      return {
        series_id: str(p, 'series_id'),
        root_task_id: str(p, 'root_task_id'),
        item_id: str(p, 'item_id'),
        title: str(p, 'title'),
        milestone: bool(p, 'milestone'),
        occurrence: str(p, 'occurrence'),
        start_date: optStr(p, 'start_date'),
        due_date: optStr(p, 'due_date'),
      };
    }),
    events: parseList(o.events, parseEvent),
    dates: parseList(o.dates, (v): CalendarDateView => {
      const d = rec(v, 'calendar date');
      const kind = CALENDAR_DATE_KINDS.find((k) => k === str(d, 'kind'));
      if (kind === undefined) throw new TypeError('unknown calendar date kind');
      return {
        kind,
        id: str(d, 'id'),
        item_id: str(d, 'item_id'),
        title: str(d, 'title'),
        date: str(d, 'date'),
        personal: bool(d, 'personal'),
        done: bool(d, 'done'),
      };
    }),
    occurrences: parseList(o.occurrences, (v): EventOccurrenceView => {
      const e = rec(v, 'occurrence');
      return {
        event_id: str(e, 'event_id'),
        occurrence: str(e, 'occurrence'),
        start_date: optStr(e, 'start_date'),
        end_date: optStr(e, 'end_date'),
        starts_at: optStr(e, 'starts_at'),
        ends_at: optStr(e, 'ends_at'),
        skipped: e.skipped === true,
      };
    }),
    items: parseList(o.items, (v) => {
      const i = rec(v, 'calendar item');
      return { id: str(i, 'id'), reference: optStr(i, 'reference'), title: str(i, 'title') };
    }),
  };
}

export type MarkTarget = 'item' | 'document' | 'task';

export interface HomeTaskView {
  id: string;
  item_id: string;
  workspace_id: string;
  reference: string | null;
  title: string;
  // 홈 계약의 상태는 항목별 사용자 정의 문자열이고 집계는 status_category(D-116).
  status: string;
  status_category: StatusCategory;
  due_date: string | null;
  bucket: 'overdue' | 'today' | 'upcoming' | null;
}

export interface HomeView {
  today: string;
  up_next: HomeTaskView[];
  /** 묶음마다의 전체 수. `up_next`는 묶음마다 50개까지라 이보다 적을 수 있다. */
  up_next_counts: { overdue: number; today: number; upcoming: number };
  recent_tasks: HomeTaskView[];
  recent_documents: {
    id: string;
    title: string;
    reference: string | null;
    workspace_id: string | null;
    item_id: string | null;
    updated_at: string;
  }[];
  active_items: {
    id: string;
    workspace_id: string;
    module_kind: ModuleKind;
    reference: string | null;
    title: string;
    tasks_done: number;
    tasks_total: number;
    can_write: boolean;
  }[];
  pins: MarkedTargetView[];
  favorites: MarkedTargetView[];
}

export interface MarkedTargetView {
  kind: MarkTarget;
  id: string;
  label: string;
  workspace_id: string | null;
  item_id: string | null;
}

function markTarget(value: string): MarkTarget {
  if (value === 'item' || value === 'document' || value === 'task') return value;
  throw new TypeError(`unknown mark target ${value}`);
}

function parseHomeTask(value: Json): HomeTaskView {
  const o = rec(value, 'home task');
  const bucket = optStr(o, 'bucket');
  if (bucket !== null && bucket !== 'overdue' && bucket !== 'today' && bucket !== 'upcoming')
    throw new TypeError(`unknown bucket ${bucket}`);
  return {
    id: str(o, 'id'),
    item_id: str(o, 'item_id'),
    workspace_id: str(o, 'workspace_id'),
    reference: optStr(o, 'reference'),
    title: str(o, 'title'),
    status: str(o, 'status'),
    due_date: optStr(o, 'due_date'),
    status_category: statusCategory(str(o, 'status_category')),
    bucket,
  };
}

function parseMarked(value: Json): MarkedTargetView {
  const o = rec(value, 'marked target');
  return {
    kind: markTarget(str(o, 'kind')),
    id: str(o, 'id'),
    label: str(o, 'label'),
    workspace_id: optStr(o, 'workspace_id'),
    item_id: optStr(o, 'item_id'),
  };
}

function parseUpNextCounts(value: unknown): HomeView['up_next_counts'] {
  const c = rec(value, 'up next counts');
  return { overdue: int(c, 'overdue'), today: int(c, 'today'), upcoming: int(c, 'upcoming') };
}

function parseHome(value: Json): HomeView {
  const o = rec(value, 'home');
  return {
    today: str(o, 'today'),
    up_next: parseList(o.up_next, parseHomeTask),
    up_next_counts: parseUpNextCounts(o.up_next_counts),
    recent_tasks: parseList(o.recent_tasks, parseHomeTask),
    recent_documents: parseList(o.recent_documents, (v) => {
      const d = rec(v, 'home document');
      return {
        id: str(d, 'id'),
        title: str(d, 'title'),
        reference: optStr(d, 'reference'),
        workspace_id: optStr(d, 'workspace_id'),
        item_id: optStr(d, 'item_id'),
        updated_at: str(d, 'updated_at'),
      };
    }),
    active_items: parseList(o.active_items, (v) => {
      const i = rec(v, 'home item');
      return {
        id: str(i, 'id'),
        workspace_id: str(i, 'workspace_id'),
        module_kind: moduleKind(str(i, 'module_kind')),
        reference: optStr(i, 'reference'),
        title: str(i, 'title'),
        tasks_done: int(i, 'tasks_done'),
        tasks_total: int(i, 'tasks_total'),
        can_write: bool(i, 'can_write'),
      };
    }),
    pins: parseList(o.pins, parseMarked),
    favorites: parseList(o.favorites, parseMarked),
  };
}

export type EpisodeStatus = 'not_started' | 'in_progress' | 'done';

export interface CourseDetailsView {
  subject: string;
  instructor: string;
  platform: string;
  url: string;
  textbook: string;
  starts_on: string | null;
  expires_on: string | null;
  target_on: string | null;
}

export interface EpisodeView {
  id: string;
  number: number;
  title: string;
  chapter: string;
  duration_seconds: number;
  url: string;
  materials: string;
  planned_on: string | null;
  mine: {
    status: EpisodeStatus;
    planned_on: string | null;
    completed_on: string | null;
    watch_count: number;
    last_position_seconds: number | null;
    note: string;
  };
  /** 공통 정보의 판. 고칠 때 읽은 판을 보낸다(그 사이 다른 사람이 고쳤으면 409). */
  version: number;
}

export interface CourseView {
  details: CourseDetailsView;
  episodes: EpisodeView[];
  can_write: boolean;
}

export interface ParticipantView {
  user_id: string;
  display_name: string;
  done: number;
  in_progress: number;
  overdue: number;
  done_seconds: number;
}

export interface NewEpisodeInput {
  request_id: string;
  number?: number;
  title: string;
  chapter: string;
  duration_seconds: number;
  url: string;
  materials: string;
  planned_on: string | null;
}

function episodeStatus(value: string): EpisodeStatus {
  if (value === 'not_started' || value === 'in_progress' || value === 'done') return value;
  throw new TypeError(`unknown episode status ${value}`);
}

function parseCourse(value: Json): CourseView {
  const o = rec(value, 'course');
  const d = rec(o.details, 'course details');
  return {
    details: {
      subject: str(d, 'subject'),
      instructor: str(d, 'instructor'),
      platform: str(d, 'platform'),
      url: str(d, 'url'),
      textbook: str(d, 'textbook'),
      starts_on: optStr(d, 'starts_on'),
      expires_on: optStr(d, 'expires_on'),
      target_on: optStr(d, 'target_on'),
    },
    episodes: parseList(o.episodes, (v): EpisodeView => {
      const e = rec(v, 'episode');
      const m = rec(e.mine, 'episode progress');
      return {
        id: str(e, 'id'),
        number: int(e, 'number'),
        title: str(e, 'title'),
        chapter: str(e, 'chapter'),
        duration_seconds: int(e, 'duration_seconds'),
        url: str(e, 'url'),
        materials: str(e, 'materials'),
        planned_on: optStr(e, 'planned_on'),
        mine: {
          status: episodeStatus(str(m, 'status')),
          planned_on: optStr(m, 'planned_on'),
          completed_on: optStr(m, 'completed_on'),
          watch_count: int(m, 'watch_count'),
          last_position_seconds: optInt(m, 'last_position_seconds'),
          note: str(m, 'note'),
        },
        version: int(e, 'version'),
      };
    }),
    can_write: bool(o, 'can_write'),
  };
}

function parseParticipant(value: Json): ParticipantView {
  const o = rec(value, 'participant');
  return {
    user_id: str(o, 'user_id'),
    display_name: str(o, 'display_name'),
    done: int(o, 'done'),
    in_progress: int(o, 'in_progress'),
    overdue: int(o, 'overdue'),
    done_seconds: int(o, 'done_seconds'),
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

export type SearchKind = 'document' | 'task' | 'item' | 'event' | 'file' | 'record';

/** 검색 필터(D-39). 상태·담당자를 주면 업무만 나온다. 날짜는 YYYY-MM-DD(끝 포함). */
export interface SearchFilters {
  kind?: SearchKind;
  moduleId?: string;
  /** 업무 상태 범주(D-116). 상태 이름이 아니라 범주로 거르며, 주면 업무만 나온다. */
  statusCategory?: StatusCategory;
  /** `me` 또는 사용자 id. */
  assignee?: string;
  from?: string;
  to?: string;
  /** 날짜의 하루를 정하는 IANA 시간대(설정의 시간대). 경계는 서버가 계산한다(D-101). */
  tz?: string;
  /** 작성자: `me` 또는 사용자 id(그 사람이 만든 것만). */
  author?: string;
  /** 제목에서만 찾기. */
  titleOnly?: boolean;
}

export interface SearchResultView {
  kind: SearchKind;
  id: string;
  title: string;
  reference: string | null;
  workspace_id: string | null;
  item_id: string | null;
  document_id: string | null;
  snippet: string | null;
}

function parseSearchResult(value: Json): SearchResultView {
  const o = rec(value, 'search result');
  const kind = str(o, 'kind');
  if (
    kind !== 'document' &&
    kind !== 'task' &&
    kind !== 'item' &&
    kind !== 'event' &&
    kind !== 'file' &&
    kind !== 'record'
  )
    throw new TypeError(`unknown search kind ${kind}`);
  return {
    kind,
    id: str(o, 'id'),
    title: str(o, 'title'),
    reference: optStr(o, 'reference'),
    workspace_id: optStr(o, 'workspace_id'),
    item_id: optStr(o, 'item_id'),
    document_id: optStr(o, 'document_id'),
    snippet: optStr(o, 'snippet'),
  };
}

/** 이동 대상: 워크스페이스 위키, 또는 그 워크스페이스의 항목. */
export interface MoveTargetInput {
  workspaceId: string;
  itemId: string | null;
}

const none = (): undefined => undefined;

/**
 * 편집·초안을 시작한 사용자 id를 서버에 알리는 머리. 서버는 세션의 사용자와 비교만 하고
 * 권한은 세션으로만 정한다.
 */
/** 판 충돌 409가 함께 보낸 서버 최신본(`current`). 판 충돌이 아닌 409(보관 등)와 다른 응답은 undefined다. */
function versionConflict(status: number, json: Json): unknown {
  if (status !== 409 || !isRecord(json) || json.current === undefined) return undefined;
  return json.current;
}

export const api = {
  // 다른 조각 소유 호출부를 위한 재노출. 구현·계약은 시간 도메인 한 곳에 있다.
  ...timeData,
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
  /**
   * 내 표시 설정(D-50 시간대, D-81 언어·주 시작·시각 표시, D-109 화면 밝기). 빈 문자열은 고르지 않음(브라우저·로케일
   * 제안을 씀). 화면 밝기는 `system`·`light`·`dark`만 받는다. 보내지 않은 칸은 서버가 그대로 둔다.
   */
  setDisplaySettings: (settings: {
    time_zone?: string;
    locale?: string;
    week_start?: string;
    hour_cycle?: string;
    theme?: string;
    last_organization_id?: string | null;
  }) => expectOk('PUT', '/api/me/settings', none, settings),
  async logout(): Promise<string> {
    return expectOk('POST', '/auth/logout', (v) => str(rec(v, 'logout'), 'redirect_to'));
  },
  /** 내가 속한 조직(전환기 목록, D-111). 서버는 기본 조직을 정하지 않는다. */
  myOrganizations: () =>
    expectOk('GET', '/api/me/organizations', (v) => parseList(v, parseOrganizationSummary)),
  organization: (id: string) =>
    expectOk('GET', `/api/organizations/${encodeURIComponent(id)}`, parseOrganization),
  workspaces: () => expectOk('GET', '/api/workspaces', (v) => parseList(v, parseWorkspace)),
  /** 조직 안 워크스페이스는 조직 멤버 누구나 만든다(D-112). */
  createWorkspace: (requestId: string, organizationId: string, name: string) =>
    expectOk('POST', '/api/workspaces', parseWorkspace, {
      request_id: requestId,
      organization_id: organizationId,
      name,
    }),
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
  async document(id: string): Promise<DocumentView> {
    const { status, json, etag } = await request('GET', `/api/documents/${encodeURIComponent(id)}`);
    if (status !== 200) fail(status, json);
    return withEtag(parseDocument(json), etag);
  },
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
  ...filesApi,
  course: (item: string) =>
    expectOk('GET', `/api/items/${encodeURIComponent(item)}/course`, parseCourse),
  setCourseDetails: (item: string, details: CourseDetailsView) =>
    expectOk('PUT', `/api/items/${encodeURIComponent(item)}/course`, none, { ...details }),
  addEpisodes: (item: string, episodes: NewEpisodeInput[]) =>
    expectOk(
      'POST',
      `/api/items/${encodeURIComponent(item)}/episodes`,
      (v) => int(rec(v, 'episodes added'), 'added'),
      { episodes: episodes.map((e) => ({ ...e })) },
    ),
  /** 회차 공통 정보 고치기. `version`은 읽은 판이며, 그 사이 다른 사람이 고쳤으면 409(아무것도 바뀌지 않음). */
  updateEpisode: (
    id: string,
    version: number,
    episode: Omit<NewEpisodeInput, 'request_id'> & { number: number },
  ) => expectOk('PUT', `/api/episodes/${encodeURIComponent(id)}`, none, { version, ...episode }),
  deleteEpisode: (id: string) =>
    expectOk('DELETE', `/api/episodes/${encodeURIComponent(id)}`, none),
  setEpisodeProgress: (
    id: string,
    progress: {
      status: EpisodeStatus;
      planned_on: string | null;
      last_position_seconds: number | null;
      note: string;
      /** 사용자 시간대의 오늘(완료한 날). */
      today: string;
    },
  ) => expectOk('PUT', `/api/episodes/${encodeURIComponent(id)}/progress`, none, { ...progress }),
  courseParticipants: (item: string, today: string) =>
    expectOk(
      'GET',
      `/api/items/${encodeURIComponent(item)}/course/participants?today=${encodeURIComponent(today)}`,
      (v) => parseList(v, parseParticipant),
    ),
  studyPlan: (item: string) =>
    expectOk('GET', `/api/items/${encodeURIComponent(item)}/study`, parseStudyPlan),
  addStudyUnits: (
    item: string,
    units: {
      request_id: string;
      title: string;
      chapter: string;
      unit: StudyUnitKind;
      amount: number;
      planned_on: string | null;
    }[],
  ) =>
    expectOk(
      'POST',
      `/api/items/${encodeURIComponent(item)}/study/units`,
      (v) => int(rec(v, 'units added'), 'added'),
      { units: units.map((u) => ({ ...u })) },
    ),
  /** 공통 단위 고치기. `version`은 읽은 판이며, 그 사이 다른 사람이 고쳤으면 409(아무것도 바뀌지 않음). */
  updateStudyUnit: (
    id: string,
    version: number,
    unit: {
      number: number;
      title: string;
      chapter: string;
      unit: StudyUnitKind;
      amount: number;
      planned_on: string | null;
    },
  ) => expectOk('PUT', `/api/study-units/${encodeURIComponent(id)}`, none, { version, ...unit }),
  deleteStudyUnit: (id: string) =>
    expectOk('DELETE', `/api/study-units/${encodeURIComponent(id)}`, none),
  setUnitProgress: (
    id: string,
    progress: {
      status: EpisodeStatus;
      done_amount: number;
      planned_on: string | null;
      note: string;
      today: string;
    },
  ) =>
    expectOk('PUT', `/api/study-units/${encodeURIComponent(id)}/progress`, none, { ...progress }),
  planStudyUnits: (item: string, entries: { unit_id: string; planned_on: string | null }[]) =>
    expectOk(
      'PUT',
      `/api/items/${encodeURIComponent(item)}/study/plan`,
      (v) => int(rec(v, 'units planned'), 'planned'),
      { entries: entries.map((e) => ({ ...e })) },
    ),
  studyParticipants: (item: string, today: string) =>
    expectOk(
      'GET',
      `/api/items/${encodeURIComponent(item)}/study/participants?today=${encodeURIComponent(today)}`,
      (v) =>
        parseList(v, (p): StudyParticipantView => {
          const o = rec(p, 'study participant');
          return {
            user_id: str(o, 'user_id'),
            display_name: str(o, 'display_name'),
            done: int(o, 'done'),
            in_progress: int(o, 'in_progress'),
            overdue: int(o, 'overdue'),
            done_amount: int(o, 'done_amount'),
          };
        }),
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
  invite: (ws: string, email: string, newRole: Role) =>
    expectOk('POST', `/api/workspaces/${encodeURIComponent(ws)}/invitations`, parseInvitation, {
      email,
      role: newRole,
    }),
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
  myNotes: () => expectOk('GET', '/api/me/notes', (v) => parseList(v, parseSummary)),
  /** 내 노트 만들기. `expectedUser`는 이 만들기를 시작한 화면의 사용자 id(서버가 세션과 다르면 409 `account_changed`). */
  async createNote(
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
  share: (id: string, email: string, perm: 'read' | 'write', organizationId: string) =>
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
  /** Zotero 컬렉션(D-34). 키는 이 요청에만 쓰고 서버가 저장하지 않는다. */
  zoteroCollections: (library: ZoteroLibraryInput) =>
    expectOk(
      'POST',
      '/api/zotero/collections',
      (v) =>
        parseList(v, (c) => {
          const o = rec(c, 'zotero collection');
          return { key: str(o, 'key'), name: str(o, 'name') };
        }),
      { ...library },
    ),
  /** Zotero 문헌을 연구 항목의 문헌 기록으로 가져온다(같은 문헌은 서지만 새로). */
  zoteroImport: (itemId: string, library: ZoteroLibraryInput, collectionKey: string | null) =>
    expectOk(
      'POST',
      `/api/items/${encodeURIComponent(itemId)}/zotero/import`,
      (v) => {
        const o = rec(v, 'zotero import');
        return {
          created: int(o, 'created'),
          updated: int(o, 'updated'),
          skipped: int(o, 'skipped'),
        };
      },
      { ...library, collection_key: collectionKey },
    ),
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
  recordKinds: () => expectOk('GET', '/api/record-kinds', (v) => parseList(v, parseRecordKind)),
  itemRecords: (itemId: string) =>
    expectOk('GET', `/api/items/${encodeURIComponent(itemId)}/records`, (v) =>
      parseList(v, parseRecord),
    ),
  createRecord: (itemId: string, requestId: string, kind: string, input: RecordInput) =>
    expectOk(
      'POST',
      `/api/items/${encodeURIComponent(itemId)}/records`,
      (v) => str(rec(v, 'created record'), 'id'),
      {
        request_id: requestId,
        kind,
        ...input,
        fields: { ...input.fields },
        files: [...input.files],
      },
    ),
  updateRecord: (id: string, version: number, input: RecordInput) =>
    expectOk(
      'PUT',
      `/api/records/${encodeURIComponent(id)}`,
      (v) => int(rec(v, 'record version'), 'version'),
      { version, ...input, fields: { ...input.fields }, files: [...input.files] },
    ),
  deleteRecord: (id: string) => expectOk('DELETE', `/api/records/${encodeURIComponent(id)}`, none),
  itemProfile: (itemId: string) =>
    expectOk('GET', `/api/items/${encodeURIComponent(itemId)}/profile`, parseItemProfile),
  /** 항목 정보 고치기. 연 때의 판(`baseVersion`)이 지났으면 아무것도 바꾸지 않고 서버 최신본을 돌려준다(D-104). */
  async setItemProfile(
    itemId: string,
    baseVersion: number,
    profile: {
      leader_id: string | null;
      goal: string;
      scope: string;
      starts_on: string | null;
      ends_on: string | null;
      status: ItemStatus;
      details: ItemDetailsView;
    },
  ): Promise<{ kind: 'saved' } | { kind: 'conflict'; current: ItemProfileView }> {
    const { status, json } = await request(
      'PUT',
      `/api/items/${encodeURIComponent(itemId)}/profile`,
      { base_version: baseVersion, ...profile, details: { ...profile.details } },
    );
    const current = versionConflict(status, json);
    if (current !== undefined) return { kind: 'conflict', current: parseItemProfile(current) };
    if (status < 200 || status >= 300) fail(status, json);
    return { kind: 'saved' };
  },
  setItemMembers: (itemId: string, userIds: string[], groupIds: string[]) =>
    expectOk('PUT', `/api/items/${encodeURIComponent(itemId)}/members`, none, {
      user_ids: userIds,
      group_ids: groupIds,
    }),
  /** 항목 복제. reset이면 업무 상태·날짜·체크와 항목 상태·기간을 비운다(템플릿 사용). */
  copyItem: (itemId: string, requestId: string, moduleId: string, title: string, reset: boolean) =>
    expectOk(
      'POST',
      `/api/items/${encodeURIComponent(itemId)}/copy`,
      (v) => {
        const o = rec(v, 'copied item');
        return { item_id: str(o, 'item_id'), skipped_documents: int(o, 'skipped_documents') };
      },
      { request_id: requestId, module_id: moduleId, title, reset },
    ),
  setItemTemplate: (itemId: string, template: boolean) =>
    expectOk('PUT', `/api/items/${encodeURIComponent(itemId)}/template`, none, { template }),
  setModuleIcon: (moduleId: string, icon: string) =>
    expectOk('PUT', `/api/modules/${encodeURIComponent(moduleId)}/icon`, none, { icon }),
  /** 모듈 설명(관리자, D-78). 빈 문자열이면 지운다. */
  setModuleDescription: (moduleId: string, description: string) =>
    expectOk('PUT', `/api/modules/${encodeURIComponent(moduleId)}/description`, parseModule, {
      description,
    }),
  taskBacklinks: (id: string) =>
    expectOk('GET', `/api/tasks/${encodeURIComponent(id)}/backlinks`, (v) =>
      parseList(v, parseBacklink),
    ),
  trashDocument: systemApi.trashDocument,
  trashItem: systemApi.trashItem,
  trashModule: systemApi.trashModule,
  trash: readTrash,
  restoreTrash: systemApi.restoreTrash,
  purgeTrash: systemApi.purgeTrash,
  purgeExpiredTrash: systemApi.purgeExpiredTrash,
  setVersionRetention: systemApi.setVersionRetention,
  setWorkspaceRealtime: (workspaceId: string, enabled: boolean) =>
    expectOk('PUT', `/api/workspaces/${encodeURIComponent(workspaceId)}/realtime`, none, {
      enabled,
    }),
  setTrashRetention: systemApi.setTrashRetention,
  /** 홈은 현재 조직의 요약이다(D-19·D-120). */
  home: (organizationId: string, tz: string) =>
    expectOk(
      'GET',
      `/api/me/home?organization_id=${encodeURIComponent(organizationId)}&tz=${encodeURIComponent(tz)}`,
      parseHome,
    ),
  /** 내 업무: 담당=나, 날짜 없음 포함, 워크스페이스별 묶음(D-120). `today`는 사용자 시간대의 오늘(YYYY-MM-DD). */
  myTasks: (organizationId: string, today: string) =>
    expectOk(
      'GET',
      `/api/me/tasks?organization_id=${encodeURIComponent(organizationId)}&today=${encodeURIComponent(today)}`,
      parseMyTasks,
    ),
  /** 달력(D-80). `scope`로 모듈·항목 범위로 좁힌다(같은 데이터, 사본 없음). */
  calendar: (
    workspaceId: string,
    from: string,
    to: string,
    tz: string,
    scope: { moduleId?: string | undefined; itemId?: string | undefined } = {},
  ) => {
    const params = new URLSearchParams({ from, to, tz });
    if (scope.moduleId !== undefined) params.set('module_id', scope.moduleId);
    if (scope.itemId !== undefined) params.set('item_id', scope.itemId);
    return expectOk(
      'GET',
      `/api/workspaces/${encodeURIComponent(workspaceId)}/calendar?${params.toString()}`,
      parseCalendar,
    );
  },
  revisions: (id: string, before: number | null) =>
    expectOk(
      'GET',
      `/api/documents/${encodeURIComponent(id)}/revisions${before === null ? '' : `?before=${String(before)}`}`,
      (v) => parseList(v, parseRevisionSummary),
    ),
  revision: (id: string, revision: number) =>
    expectOk(
      'GET',
      `/api/documents/${encodeURIComponent(id)}/revisions/${String(revision)}`,
      parseRevision,
    ),
  /** 저장된 리비전 하나를 내보낸다(D-75). 서버가 내보내기 권한을 확인하고 기록을 남긴 뒤 그 리비전을 준다. */
  exportRevision: (id: string, revision: number, format: 'markdown' | 'zip' | 'pdf') =>
    expectOk(
      'POST',
      `/api/documents/${encodeURIComponent(id)}/revisions/${String(revision)}/export`,
      parseRevision,
      { format },
    ),
  /** 통합 검색. 볼 수 있는 것만, 최대 20건. */
  /** 검색은 현재 조직 범위다(D-111). `workspaceId`는 그 안의 좁힘이지 조직을 대신하지 않는다. */
  search: (
    organizationId: string,
    q: string,
    workspaceId: string | null,
    filters: SearchFilters = {},
  ) => {
    const params = new URLSearchParams({ organization_id: organizationId, q });
    if (workspaceId !== null) params.set('workspace_id', workspaceId);
    if (filters.kind !== undefined) params.set('kinds', filters.kind);
    if (filters.moduleId !== undefined) params.set('module_id', filters.moduleId);
    if (filters.statusCategory !== undefined) params.set('status_category', filters.statusCategory);
    if (filters.assignee !== undefined) params.set('assignee', filters.assignee);
    if (filters.from !== undefined) params.set('from', filters.from);
    if (filters.to !== undefined) params.set('to', filters.to);
    if (filters.tz !== undefined) params.set('tz', filters.tz);
    if (filters.author !== undefined) params.set('author', filters.author);
    if (filters.titleOnly === true) params.set('title_only', 'true');
    return expectOk('GET', `/api/search?${params.toString()}`, (v) =>
      parseList(v, parseSearchResult),
    );
  },
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
  /** 항목 정보(제목·설명·공개 범위) 고치기. 읽은 판(`baseVersion`)이 지났으면 아무것도 바꾸지 않고 서버 최신본을 돌려준다(D-104). */
  async updateItem(
    id: string,
    baseVersion: number,
    /** 공개 범위는 바꿀 때만 보낸다(없으면 서버의 지금 값 그대로). */
    input: { title: string; description: string; visibility?: 'workspace' | 'private' },
  ): Promise<{ kind: 'updated'; item: ItemView } | { kind: 'conflict'; current: ItemView }> {
    const { status, json } = await request('PUT', `/api/items/${encodeURIComponent(id)}`, {
      base_version: baseVersion,
      ...input,
    });
    const current = versionConflict(status, json);
    if (current !== undefined) return { kind: 'conflict', current: parseItem(current) };
    if (status !== 200) fail(status, json);
    return { kind: 'updated', item: parseItem(json) };
  },
  archiveItem: (id: string, archived: boolean) =>
    expectOk('PUT', `/api/items/${encodeURIComponent(id)}/archived`, parseItem, { archived }),
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
  /**
   * 저장. 충돌(409 기준 리비전이 지남, 412 `If-Match`가 맞지 않음)이면 `{ kind: 'conflict', current }`.
   * `etag`가 강한 값일 때만 `If-Match`로 보낸다(D-77). 없거나 약하면 `base_revision`만으로 판단한다.
   * `expectedUser`는 이 편집을 시작한 사용자 id다(편집기를 열 때 고정한 값, 보내기 직전에 다시 채우지 않는다).
   * 서버가 세션의 사용자와 다르면 아무것도 바꾸지 않고 409 `account_changed`(`ApiError`)를 돌려준다.
   */
  async saveDocument(
    id: string,
    input: {
      expectedUser: string;
      requestId: string;
      baseRevision: number;
      title: string;
      body: DocNode;
      etag?: string | undefined;
    },
  ): Promise<
    { kind: 'saved'; document: DocumentView } | { kind: 'conflict'; current: DocumentView }
  > {
    const { status, json, etag } = await request(
      'PUT',
      `/api/documents/${encodeURIComponent(id)}`,
      {
        request_id: input.requestId,
        base_revision: input.baseRevision,
        title: input.title,
        body: input.body,
      },
      isStrongEtag(input.etag)
        ? { [EXPECTED_USER]: input.expectedUser, 'If-Match': input.etag }
        : { [EXPECTED_USER]: input.expectedUser },
    );
    const current = conflictCurrent(status, json);
    if (current !== undefined) return { kind: 'conflict', current };
    if (status !== 200) fail(status, json);
    return { kind: 'saved', document: withEtag(parseDocument(json), etag) };
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
