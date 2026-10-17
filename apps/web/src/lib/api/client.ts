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
  /** 노출을 줄이는 관리(공유·공개 링크 철회, 비공개 전환)를 할 수 있는지. 보관 중에도 소유 권한자는 참. */
  can_manage: boolean;
  /** 휴지통으로 옮길 수 있는지. */
  can_delete: boolean;
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

// --- 리비전 -------------------------------------------------------------

export interface RevisionSummaryView {
  revision: number;
  title: string;
  author_name: string | null;
  created_at: string;
  restored_from: number | null;
}

export interface RevisionView extends RevisionSummaryView {
  body: DocNode;
  schema_version: number;
}

function optInt(o: Record<string, unknown>, key: string): number | null {
  const v = o[key];
  if (v === null || v === undefined) return null;
  if (typeof v !== 'number' || !Number.isSafeInteger(v))
    throw new TypeError(`expected integer at ${key}`);
  return v;
}

export function parseRevisionSummary(value: Json): RevisionSummaryView {
  const o = rec(value, 'revision');
  return {
    revision: int(o, 'revision'),
    title: str(o, 'title'),
    author_name: optStr(o, 'author_name'),
    created_at: str(o, 'created_at'),
    restored_from: optInt(o, 'restored_from'),
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

// --- 업무 -------------------------------------------------------------

export type TaskStatus = 'todo' | 'in_progress' | 'done';
export type TaskPriority = 'none' | 'low' | 'medium' | 'high' | 'urgent';
export const TASK_STATUSES: readonly TaskStatus[] = ['todo', 'in_progress', 'done'] as const;
export const TASK_PRIORITIES: readonly TaskPriority[] = [
  'none',
  'low',
  'medium',
  'high',
  'urgent',
] as const;

export interface AssigneeView {
  user_id: string;
  display_name: string;
}

export const FREQUENCIES = ['daily', 'weekly', 'monthly', 'yearly'] as const;
export type Frequency = (typeof FREQUENCIES)[number];

/** 반복 규칙. 기준일이 첫 회차, `count`는 첫 회차 포함, 없는 날(31일·2월 29일)은 건너뜀. */
export interface RecurrenceRule {
  freq: Frequency;
  interval: number;
  /** ISO 요일(월=1 … 일=7), 매주에만. */
  weekdays: number[];
  until: string | null;
  count: number | null;
}

function parseRule(value: Json): RecurrenceRule | null {
  if (value === null || value === undefined) return null;
  const o = rec(value, 'recurrence');
  const freq = FREQUENCIES.find((f) => f === str(o, 'freq'));
  if (freq === undefined) throw new TypeError(`unknown frequency ${str(o, 'freq')}`);
  return {
    freq,
    interval: int(o, 'interval'),
    weekdays: Array.isArray(o.weekdays) ? o.weekdays.map((d) => Number(d)) : [],
    until: optStr(o, 'until'),
    count: optInt(o, 'count'),
  };
}

export interface TaskSeriesView {
  id: string;
  root_task_id: string;
  rule: RecurrenceRule | null;
  timezone: string;
}

export interface TaskView {
  id: string;
  item_id: string;
  module_id: string;
  workspace_id: string;
  parent_id: string | null;
  reference: string | null;
  title: string;
  description: string;
  status: TaskStatus;
  priority: TaskPriority;
  start_date: string | null;
  due_date: string | null;
  estimate_minutes: number | null;
  tags: string[];
  milestone: boolean;
  /** 같은 항목 안의 선행 업무(끝나야 시작). */
  predecessor_ids: string[];
  /** 반복 업무의 회차면 그 반복. */
  series: TaskSeriesView | null;
  occurrence_date: string | null;
  assignees: AssigneeView[];
  checklist_done: number;
  checklist_total: number;
  version: number;
  completed_at: string | null;
  access: Access;
  updated_at: string;
}

export interface TaskChecklistItemView {
  id: string;
  text: string;
  checked: boolean;
}

export interface TaskDetailView extends TaskView {
  checklist: TaskChecklistItemView[];
}

export interface TaskInput {
  title: string;
  description: string;
  status: TaskStatus;
  priority: TaskPriority;
  start_date: string | null;
  due_date: string | null;
  estimate_minutes: number | null;
  tags: string[];
  assignee_ids: string[];
  milestone: boolean;
}

/** 선행 업무를 옮길 때 함께 옮길 후속 업무(현재 → 제안). 승인하면 그대로 `SuccessorShift`로 보낸다. */
export interface ShiftProposalView {
  task_id: string;
  reference: string | null;
  title: string;
  base_version: number;
  from_start_date: string | null;
  from_due_date: string | null;
  start_date: string | null;
  due_date: string | null;
}

export interface SuccessorShift {
  task_id: string;
  base_version: number;
  start_date: string | null;
  due_date: string | null;
}

function parseShiftProposal(value: Json): ShiftProposalView {
  const o = rec(value, 'shift proposal');
  return {
    task_id: str(o, 'task_id'),
    reference: optStr(o, 'reference'),
    title: str(o, 'title'),
    base_version: int(o, 'base_version'),
    from_start_date: optStr(o, 'from_start_date'),
    from_due_date: optStr(o, 'from_due_date'),
    start_date: optStr(o, 'start_date'),
    due_date: optStr(o, 'due_date'),
  };
}

function taskStatus(value: string): TaskStatus {
  const found = TASK_STATUSES.find((s) => s === value);
  if (found === undefined) throw new TypeError(`unknown task status ${value}`);
  return found;
}

function taskPriority(value: string): TaskPriority {
  const found = TASK_PRIORITIES.find((p) => p === value);
  if (found === undefined) throw new TypeError(`unknown priority ${value}`);
  return found;
}

function parseAssignee(value: Json): AssigneeView {
  const o = rec(value, 'assignee');
  return { user_id: str(o, 'user_id'), display_name: str(o, 'display_name') };
}

export function parseTask(value: Json): TaskView {
  const o = rec(value, 'task');
  return {
    id: str(o, 'id'),
    item_id: str(o, 'item_id'),
    module_id: str(o, 'module_id'),
    workspace_id: str(o, 'workspace_id'),
    parent_id: optStr(o, 'parent_id'),
    reference: optStr(o, 'reference'),
    title: str(o, 'title'),
    description: str(o, 'description'),
    status: taskStatus(str(o, 'status')),
    priority: taskPriority(str(o, 'priority')),
    start_date: optStr(o, 'start_date'),
    due_date: optStr(o, 'due_date'),
    estimate_minutes: optInt(o, 'estimate_minutes'),
    tags: strings(o.tags),
    milestone: bool(o, 'milestone'),
    predecessor_ids: strings(o.predecessor_ids),
    series:
      o.series === null || o.series === undefined
        ? null
        : ((): TaskSeriesView => {
            const s = rec(o.series, 'series');
            return {
              id: str(s, 'id'),
              root_task_id: str(s, 'root_task_id'),
              rule: parseRule(s.rule),
              timezone: str(s, 'timezone'),
            };
          })(),
    occurrence_date: optStr(o, 'occurrence_date'),
    assignees: parseList(o.assignees, parseAssignee),
    checklist_done: int(o, 'checklist_done'),
    checklist_total: int(o, 'checklist_total'),
    version: int(o, 'version'),
    completed_at: optStr(o, 'completed_at'),
    access: access(str(o, 'access')),
    updated_at: str(o, 'updated_at'),
  };
}

export function parseTaskDetail(value: Json): TaskDetailView {
  const o = rec(value, 'task detail');
  return {
    ...parseTask(o),
    checklist: parseList(o.checklist, (v): TaskChecklistItemView => {
      const c = rec(v, 'checklist item');
      return { id: str(c, 'id'), text: str(c, 'text'), checked: bool(c, 'checked') };
    }),
  };
}

export const EVENT_KINDS = ['meeting', 'presentation', 'event'] as const;
export type EventKind = (typeof EVENT_KINDS)[number];

export interface EventView {
  id: string;
  item_id: string;
  workspace_id: string;
  kind: EventKind;
  title: string;
  description: string;
  /** 종일이면 날짜(끝 포함), 아니면 UTC 시각. */
  all_day: boolean;
  start_date: string | null;
  end_date: string | null;
  starts_at: string | null;
  ends_at: string | null;
  /** 일정에 지정한 시간대(IANA). */
  timezone: string;
  recurrence: RecurrenceRule | null;
  /** 뺀 회차(일정 시간대 기준 날짜). */
  excluded_dates: string[];
  version: number;
  access: Access;
}

export interface EventInput {
  kind: EventKind;
  title: string;
  description: string;
  all_day: boolean;
  start_date: string | null;
  end_date: string | null;
  starts_at: string | null;
  ends_at: string | null;
  timezone: string;
}

export function parseEvent(value: Json): EventView {
  const o = rec(value, 'event');
  const kind = EVENT_KINDS.find((k) => k === str(o, 'kind'));
  if (kind === undefined) throw new TypeError(`unknown event kind ${str(o, 'kind')}`);
  return {
    id: str(o, 'id'),
    item_id: str(o, 'item_id'),
    workspace_id: str(o, 'workspace_id'),
    kind,
    title: str(o, 'title'),
    description: str(o, 'description'),
    all_day: bool(o, 'all_day'),
    start_date: optStr(o, 'start_date'),
    end_date: optStr(o, 'end_date'),
    starts_at: optStr(o, 'starts_at'),
    ends_at: optStr(o, 'ends_at'),
    timezone: str(o, 'timezone'),
    recurrence: parseRule(o.recurrence),
    excluded_dates: strings(o.excluded_dates),
    version: int(o, 'version'),
    access: access(str(o, 'access')),
  };
}

export interface CalendarTaskView {
  id: string;
  item_id: string;
  reference: string | null;
  title: string;
  status: TaskStatus;
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

export interface CalendarView {
  tasks: CalendarTaskView[];
  projected_tasks: ProjectedTaskView[];
  events: EventView[];
  occurrences: EventOccurrenceView[];
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
        status: taskStatus(str(t, 'status')),
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
    occurrences: parseList(o.occurrences, (v): EventOccurrenceView => {
      const e = rec(v, 'occurrence');
      return {
        event_id: str(e, 'event_id'),
        occurrence: str(e, 'occurrence'),
        start_date: optStr(e, 'start_date'),
        end_date: optStr(e, 'end_date'),
        starts_at: optStr(e, 'starts_at'),
        ends_at: optStr(e, 'ends_at'),
      };
    }),
    items: parseList(o.items, (v) => {
      const i = rec(v, 'calendar item');
      return { id: str(i, 'id'), reference: optStr(i, 'reference'), title: str(i, 'title') };
    }),
  };
}

export const TIME_KINDS = ['work', 'study', 'lecture', 'rest', 'other'] as const;
export type TimeKind = (typeof TIME_KINDS)[number];
export type TimeTargetKind = 'task' | 'document' | 'item';

export interface TimeTargetView {
  kind: TimeTargetKind;
  id: string;
  /** 지금 볼 수 없으면 없다. */
  label: string | null;
  workspace_id: string | null;
}

export interface TimeEntryView {
  id: string;
  started_at: string;
  /** 진행 중이면 없다. */
  ended_at: string | null;
  /** 일시정지 중이면 그 시각. */
  paused_at: string | null;
  paused_seconds: number;
  /** 응답 시각까지의 길이(초). */
  seconds: number;
  kind: TimeKind;
  memo: string;
  target: TimeTargetView | null;
  version: number;
}

export interface TimeEntryInput {
  started_at: string;
  ended_at: string | null;
  paused_seconds: number;
  kind: TimeKind;
  memo: string;
  target: { kind: TimeTargetKind; id: string } | null;
}

export interface TimeSummaryRowView {
  period: string;
  kind: TimeKind;
  seconds: number;
}

export interface TaskTimeView {
  estimate_minutes: number | null;
  mine_seconds: number;
  shared_seconds: number;
  shared_people: number;
}

function timeKind(value: string): TimeKind {
  const found = TIME_KINDS.find((k) => k === value);
  if (found === undefined) throw new TypeError(`unknown time kind ${value}`);
  return found;
}

export function parseTimeEntry(value: Json): TimeEntryView {
  const o = rec(value, 'time entry');
  let target: TimeTargetView | null = null;
  if (o.target !== null && o.target !== undefined) {
    const t = rec(o.target, 'time target');
    const kind = str(t, 'kind');
    if (kind !== 'task' && kind !== 'document' && kind !== 'item')
      throw new TypeError(`unknown target kind ${kind}`);
    target = {
      kind,
      id: str(t, 'id'),
      label: optStr(t, 'label'),
      workspace_id: optStr(t, 'workspace_id'),
    };
  }
  return {
    id: str(o, 'id'),
    started_at: str(o, 'started_at'),
    ended_at: optStr(o, 'ended_at'),
    paused_at: optStr(o, 'paused_at'),
    paused_seconds: int(o, 'paused_seconds'),
    seconds: int(o, 'seconds'),
    kind: timeKind(str(o, 'kind')),
    memo: str(o, 'memo'),
    target,
    version: int(o, 'version'),
  };
}

export type MarkTarget = 'item' | 'document' | 'task';
export type MarkKind = 'pin' | 'favorite';

export interface HomeTaskView {
  id: string;
  item_id: string;
  workspace_id: string;
  reference: string | null;
  title: string;
  status: TaskStatus;
  due_date: string | null;
  bucket: 'overdue' | 'today' | 'upcoming' | null;
}

export interface HomeView {
  today: string;
  up_next: HomeTaskView[];
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
    status: taskStatus(str(o, 'status')),
    due_date: optStr(o, 'due_date'),
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

function parseHome(value: Json): HomeView {
  const o = rec(value, 'home');
  return {
    today: str(o, 'today'),
    up_next: parseList(o.up_next, parseHomeTask),
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

function parseMarkState(value: Json): { pin: boolean; favorite: boolean } {
  const o = rec(value, 'mark state');
  return { pin: bool(o, 'pin'), favorite: bool(o, 'favorite') };
}

export interface TrashEntryView {
  id: string;
  kind: 'item' | 'document' | 'task' | 'event' | 'file';
  root_id: string;
  title: string;
  member_count: number;
  deleted_by: string;
  deleted_by_name: string | null;
  deleted_at: string;
  expires_at: string;
  expired: boolean;
  can_purge: boolean;
}

export interface TrashView {
  retention_days: number;
  entries: TrashEntryView[];
}

function parseTrash(value: Json): TrashView {
  const o = rec(value, 'trash');
  return {
    retention_days: int(o, 'retention_days'),
    entries: parseList(o.entries, (v): TrashEntryView => {
      const e = rec(v, 'trash entry');
      const kind = str(e, 'kind');
      if (
        kind !== 'item' &&
        kind !== 'document' &&
        kind !== 'task' &&
        kind !== 'event' &&
        kind !== 'file'
      )
        throw new TypeError(`unknown trash kind ${kind}`);
      return {
        id: str(e, 'id'),
        kind,
        root_id: str(e, 'root_id'),
        title: str(e, 'title'),
        member_count: int(e, 'member_count'),
        deleted_by: str(e, 'deleted_by'),
        deleted_by_name: optStr(e, 'deleted_by_name'),
        deleted_at: str(e, 'deleted_at'),
        expires_at: str(e, 'expires_at'),
        expired: bool(e, 'expired'),
        can_purge: bool(e, 'can_purge'),
      };
    }),
  };
}

export interface FileView {
  id: string;
  name: string;
  document_id: string | null;
  item_id: string | null;
  current_version: number;
  size: number;
  content_type: string;
  updated_at: string;
  can_write: boolean;
  can_download: boolean;
}

export interface FileVersionView {
  version: number;
  size: number;
  sha256: string;
  content_type: string;
  note: string;
  uploaded_by: string;
  uploaded_by_name: string | null;
  created_at: string;
}

export interface UploadView {
  id: string;
  state: string;
  name: string;
  size: number;
  chunk_bytes: number;
  parts_total: number;
  received: number[];
  expires_at: string;
  file_id: string | null;
  version: number | null;
  failure: string | null;
}

export interface FileTarget {
  kind: 'document' | 'item';
  id: string;
}

function parseFile(value: Json): FileView {
  const o = rec(value, 'file');
  return {
    id: str(o, 'id'),
    name: str(o, 'name'),
    document_id: optStr(o, 'document_id'),
    item_id: optStr(o, 'item_id'),
    current_version: int(o, 'current_version'),
    size: int(o, 'size'),
    content_type: str(o, 'content_type'),
    updated_at: str(o, 'updated_at'),
    can_write: bool(o, 'can_write'),
    can_download: bool(o, 'can_download'),
  };
}

function parseVersion(value: Json): FileVersionView {
  const o = rec(value, 'file version');
  return {
    version: int(o, 'version'),
    size: int(o, 'size'),
    sha256: str(o, 'sha256'),
    content_type: str(o, 'content_type'),
    note: str(o, 'note'),
    uploaded_by: str(o, 'uploaded_by'),
    uploaded_by_name: optStr(o, 'uploaded_by_name'),
    created_at: str(o, 'created_at'),
  };
}

function parseUpload(value: Json): UploadView {
  const o = rec(value, 'upload');
  const received = Array.isArray(o.received) ? o.received : [];
  return {
    id: str(o, 'id'),
    state: str(o, 'state'),
    name: str(o, 'name'),
    size: int(o, 'size'),
    chunk_bytes: int(o, 'chunk_bytes'),
    parts_total: int(o, 'parts_total'),
    received: received.filter((n): n is number => typeof n === 'number'),
    expires_at: str(o, 'expires_at'),
    file_id: optStr(o, 'file_id'),
    version: optInt(o, 'version'),
    failure: optStr(o, 'failure'),
  };
}

/** 파일 내용 주소(같은 원점, 쿠키로 권한 확인). */
export function fileContentUrl(id: string, version?: number, inline = false): string {
  const params = new URLSearchParams();
  if (version !== undefined) params.set('version', String(version));
  if (inline) params.set('inline', 'true');
  const query = params.toString();
  return `/api/files/${encodeURIComponent(id)}/content${query === '' ? '' : `?${query}`}`;
}

/** 업로드 조각 보내기(날것 본문 + SHA-256 머리글). */
async function putPart(upload: string, number: number, body: Blob, sha256: string): Promise<void> {
  let response: Response;
  try {
    response = await fetch(`/api/uploads/${encodeURIComponent(upload)}/parts/${String(number)}`, {
      method: 'PUT',
      credentials: 'same-origin',
      headers: { 'Content-Type': 'application/octet-stream', 'x-content-sha256': sha256 },
      body,
    });
  } catch (error: unknown) {
    throw new NetworkError(error instanceof Error ? error.message : 'network error');
  }
  if (response.status !== 204) {
    const text = await response.text();
    let json: Json;
    try {
      json = text === '' ? null : JSON.parse(text);
    } catch {
      json = null;
    }
    fail(response.status, json);
  }
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

export type SearchKind = 'document' | 'task' | 'item' | 'event' | 'file';

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
    kind !== 'file'
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

const parseBatch = (v: Json): string => str(rec(v, 'trashed'), 'batch_id');

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
  createWikiDocument: (
    workspaceId: string,
    requestId: string,
    title: string,
    parentId: string | null = null,
  ) =>
    expectOk(
      'POST',
      `/api/workspaces/${encodeURIComponent(workspaceId)}/wiki/documents`,
      parseDocument,
      { request_id: requestId, title, parent_id: parentId },
    ),
  /** 트리에서 부모·순서를 바꾼다. parentId가 null이면 최상위, beforeId가 있으면 그 형제 바로 앞. */
  setParent: (id: string, parentId: string | null, beforeId: string | null) =>
    expectOk('PUT', `/api/documents/${encodeURIComponent(id)}/parent`, parseDocument, {
      parent_id: parentId,
      before_id: beforeId,
    }),
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
  /** 워크스페이스 지우기(소유자, 이름 확인). 영구 삭제되는 때를 돌려준다. */
  deleteWorkspace: (ws: string, name: string) =>
    expectOk(
      'DELETE',
      `/api/workspaces/${encodeURIComponent(ws)}`,
      (v) => str(rec(v, 'workspace deleted'), 'expires_at'),
      { name },
    ),
  documentFiles: (id: string) =>
    expectOk('GET', `/api/documents/${encodeURIComponent(id)}/files`, (v) =>
      parseList(v, parseFile),
    ),
  itemFiles: (id: string) =>
    expectOk('GET', `/api/items/${encodeURIComponent(id)}/files`, (v) => parseList(v, parseFile)),
  file: (id: string) =>
    expectOk('GET', `/api/files/${encodeURIComponent(id)}`, (v) => {
      const o = rec(v, 'file detail');
      return { file: parseFile(o.file), versions: parseList(o.versions, parseVersion) };
    }),
  renameFile: (id: string, name: string) =>
    expectOk('PUT', `/api/files/${encodeURIComponent(id)}`, parseFile, { name }),
  deleteFileVersion: (id: string, version: number) =>
    expectOk('DELETE', `/api/files/${encodeURIComponent(id)}/versions/${String(version)}`, none),
  trashFile: (id: string) => expectOk('DELETE', `/api/files/${encodeURIComponent(id)}`, parseBatch),
  /** 업로드 시작. 같은 requestId면 같은 업로드(이어 올리기). */
  createUpload: (input: {
    requestId: string;
    target?: FileTarget;
    fileId?: string;
    name: string;
    note: string;
    contentType: string;
    size: number;
  }) =>
    expectOk('POST', '/api/uploads', parseUpload, {
      request_id: input.requestId,
      target: input.target ?? null,
      file_id: input.fileId ?? null,
      name: input.name,
      note: input.note,
      content_type: input.contentType,
      size: input.size,
    }),
  upload: (id: string) => expectOk('GET', `/api/uploads/${encodeURIComponent(id)}`, parseUpload),
  putUploadPart: putPart,
  completeUpload: (id: string) =>
    expectOk('POST', `/api/uploads/${encodeURIComponent(id)}/complete`, parseUpload),
  abortUpload: (id: string) => expectOk('DELETE', `/api/uploads/${encodeURIComponent(id)}`, none),
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
  createNote: (requestId: string, title: string, parentId: string | null = null) =>
    expectOk('POST', '/api/me/notes', parseDocument, {
      request_id: requestId,
      title,
      parent_id: parentId,
    }),
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
  tasks: (itemId: string) =>
    expectOk('GET', `/api/items/${encodeURIComponent(itemId)}/tasks`, (v) =>
      parseList(v, parseTask),
    ),
  assignable: (itemId: string) =>
    expectOk('GET', `/api/items/${encodeURIComponent(itemId)}/assignable`, (v) =>
      parseList(v, parseAssignee),
    ),
  task: (id: string) => expectOk('GET', `/api/tasks/${encodeURIComponent(id)}`, parseTaskDetail),
  createTask: (itemId: string, requestId: string, title: string, parentId: string | null = null) =>
    expectOk('POST', `/api/items/${encodeURIComponent(itemId)}/tasks`, parseTaskDetail, {
      request_id: requestId,
      title,
      parent_id: parentId,
    }),
  /**
   * 필드 고치기와 승인한 후속 재배치. 이 업무의 기준 버전이 지났으면 `conflict`, 승인한 후속 업무가 그사이
   * 바뀌었으면 `shift_conflict`(아무것도 바뀌지 않았으니 다시 미리 본다).
   */
  async updateTask(
    id: string,
    baseVersion: number,
    input: TaskInput,
    successorShifts: SuccessorShift[] = [],
  ): Promise<
    | { kind: 'updated'; task: TaskDetailView }
    | { kind: 'conflict'; current: TaskDetailView }
    | { kind: 'shift_conflict'; current: TaskDetailView }
  > {
    const { status, json } = await request('PUT', `/api/tasks/${encodeURIComponent(id)}`, {
      base_version: baseVersion,
      ...input,
      successor_shifts: successorShifts,
    });
    // 409 중 기준 버전 충돌만 서버 최신본이 온다(보관·다시 시도 등은 일반 오류).
    if (status === 409 && isRecord(json) && json.current !== undefined) {
      const body = rec(json, 'conflict');
      const current = parseTaskDetail(body.current);
      return str(body, 'error') === 'shift_conflict'
        ? { kind: 'shift_conflict', current }
        : { kind: 'conflict', current };
    }
    if (status !== 200) fail(status, json);
    return { kind: 'updated', task: parseTaskDetail(json) };
  },
  moveTask: (id: string, status: TaskStatus, beforeId: string | null) =>
    expectOk('PUT', `/api/tasks/${encodeURIComponent(id)}/position`, parseTaskDetail, {
      status,
      before_id: beforeId,
    }),
  previewSchedule: (id: string, startDate: string | null, dueDate: string | null) =>
    expectOk(
      'POST',
      `/api/tasks/${encodeURIComponent(id)}/schedule-preview`,
      (v) => parseList(v, parseShiftProposal),
      { start_date: startDate, due_date: dueDate },
    ),
  addPredecessor: (id: string, predecessorId: string) =>
    expectOk(
      'PUT',
      `/api/tasks/${encodeURIComponent(id)}/predecessors/${encodeURIComponent(predecessorId)}`,
      parseTaskDetail,
    ),
  removePredecessor: (id: string, predecessorId: string) =>
    expectOk(
      'DELETE',
      `/api/tasks/${encodeURIComponent(id)}/predecessors/${encodeURIComponent(predecessorId)}`,
      parseTaskDetail,
    ),
  setTaskParent: (id: string, parentId: string | null) =>
    expectOk('PUT', `/api/tasks/${encodeURIComponent(id)}/parent`, parseTaskDetail, {
      parent_id: parentId,
    }),
  addChecklist: (taskId: string, items: { request_id: string; text: string; checked: boolean }[]) =>
    expectOk('POST', `/api/tasks/${encodeURIComponent(taskId)}/checklist`, parseTaskDetail, {
      items,
    }),
  updateChecklistItem: (id: string, text: string, checked: boolean) =>
    expectOk('PUT', `/api/checklist-items/${encodeURIComponent(id)}`, none, { text, checked }),
  deleteChecklistItem: (id: string) =>
    expectOk('DELETE', `/api/checklist-items/${encodeURIComponent(id)}`, none),
  taskBacklinks: (id: string) =>
    expectOk('GET', `/api/tasks/${encodeURIComponent(id)}/backlinks`, (v) =>
      parseList(v, parseBacklink),
    ),
  itemEvents: (itemId: string) =>
    expectOk('GET', `/api/items/${encodeURIComponent(itemId)}/events`, (v) =>
      parseList(v, parseEvent),
    ),
  event: (id: string) => expectOk('GET', `/api/events/${encodeURIComponent(id)}`, parseEvent),
  createEvent: (itemId: string, requestId: string, input: EventInput) =>
    expectOk('POST', `/api/items/${encodeURIComponent(itemId)}/events`, parseEvent, {
      request_id: requestId,
      ...input,
    }),
  /** 일정 고치기. 기준 버전이 지났으면 `{ kind: 'conflict', current }`. */
  async updateEvent(
    id: string,
    baseVersion: number,
    input: EventInput,
  ): Promise<{ kind: 'updated'; event: EventView } | { kind: 'conflict'; current: EventView }> {
    const { status, json } = await request('PUT', `/api/events/${encodeURIComponent(id)}`, {
      base_version: baseVersion,
      ...input,
    });
    if (status === 409 && isRecord(json) && json.current !== undefined)
      return { kind: 'conflict', current: parseEvent(rec(json, 'conflict').current) };
    if (status !== 200) fail(status, json);
    return { kind: 'updated', event: parseEvent(json) };
  },
  deleteEvent: (id: string) => expectOk('DELETE', `/api/events/${encodeURIComponent(id)}`, none),
  setTaskRecurrence: (id: string, rule: RecurrenceRule | null, timezone: string) =>
    expectOk('PUT', `/api/tasks/${encodeURIComponent(id)}/recurrence`, parseTaskDetail, {
      rule: rule === null ? null : { ...rule },
      timezone,
    }),
  /** 반복 업무 회차 만들기(쓰기 권한으로 항목을 열 때). 만든 수. */
  syncRecurrences: (itemId: string) =>
    expectOk('POST', `/api/items/${encodeURIComponent(itemId)}/recurrences/sync`, (v) =>
      int(rec(v, 'sync'), 'created'),
    ),
  setEventRecurrence: (id: string, rule: RecurrenceRule | null) =>
    expectOk('PUT', `/api/events/${encodeURIComponent(id)}/recurrence`, parseEvent, {
      rule: rule === null ? null : { ...rule },
    }),
  excludeOccurrence: (id: string, date: string) =>
    expectOk('POST', `/api/events/${encodeURIComponent(id)}/exclusions`, none, { date }),
  trashDocument: (id: string) =>
    expectOk('DELETE', `/api/documents/${encodeURIComponent(id)}`, parseBatch),
  trashTask: (id: string) => expectOk('DELETE', `/api/tasks/${encodeURIComponent(id)}`, parseBatch),
  trashItem: (id: string) => expectOk('DELETE', `/api/items/${encodeURIComponent(id)}`, parseBatch),
  /** 워크스페이스 휴지통, 또는 null이면 내 개인 노트 휴지통. */
  trash: (workspaceId: string | null) =>
    expectOk(
      'GET',
      workspaceId === null
        ? '/api/me/trash'
        : `/api/workspaces/${encodeURIComponent(workspaceId)}/trash`,
      parseTrash,
    ),
  restoreTrash: (batch: string) =>
    expectOk('POST', `/api/trash/${encodeURIComponent(batch)}/restore`, none),
  purgeTrash: (batch: string) =>
    expectOk('DELETE', `/api/trash/${encodeURIComponent(batch)}`, none),
  purgeExpiredTrash: (workspaceId: string | null) =>
    expectOk(
      'POST',
      workspaceId === null
        ? '/api/me/trash/purge-expired'
        : `/api/workspaces/${encodeURIComponent(workspaceId)}/trash/purge-expired`,
      (v) => int(rec(v, 'purged'), 'purged'),
    ),
  setTrashRetention: (workspaceId: string, days: number) =>
    expectOk('PUT', `/api/workspaces/${encodeURIComponent(workspaceId)}/trash-retention`, none, {
      days,
    }),
  home: (tz: string) => expectOk('GET', `/api/me/home?tz=${encodeURIComponent(tz)}`, parseHome),
  mark: (kind: MarkTarget, id: string) =>
    expectOk('GET', `/api/me/marks/${kind}/${encodeURIComponent(id)}`, parseMarkState),
  setMark: (kind: MarkTarget, id: string, mark: MarkKind, on: boolean) =>
    expectOk('PUT', `/api/me/marks/${kind}/${encodeURIComponent(id)}`, parseMarkState, {
      mark,
      on,
    }),
  currentTimer: () =>
    expectOk('GET', '/api/me/timer', (v) => (v === null ? null : parseTimeEntry(v))),
  startTimer: (
    requestId: string,
    kind: TimeKind,
    memo: string,
    target: { kind: TimeTargetKind; id: string } | null,
  ) =>
    expectOk('POST', '/api/me/timer', parseTimeEntry, {
      request_id: requestId,
      kind,
      memo,
      target: target === null ? null : { ...target },
    }),
  timerAction: (id: string, action: 'pause' | 'resume' | 'stop') =>
    expectOk('POST', `/api/time-entries/${encodeURIComponent(id)}/timer`, parseTimeEntry, {
      action,
    }),
  timeEntries: (from: string, to: string, tz: string) =>
    expectOk(
      'GET',
      `/api/me/time-entries?from=${from}&to=${to}&tz=${encodeURIComponent(tz)}`,
      (v) => parseList(v, parseTimeEntry),
    ),
  addTimeEntry: (requestId: string, input: TimeEntryInput) =>
    expectOk('POST', '/api/me/time-entries', parseTimeEntry, {
      request_id: requestId,
      ...input,
      target: input.target === null ? null : { ...input.target },
    }),
  /** 기록 고치기. 기준 버전이 지났으면 `{ kind: 'conflict', current }`. */
  async updateTimeEntry(
    id: string,
    baseVersion: number,
    input: TimeEntryInput,
  ): Promise<
    { kind: 'updated'; entry: TimeEntryView } | { kind: 'conflict'; current: TimeEntryView }
  > {
    const { status, json } = await request('PUT', `/api/time-entries/${encodeURIComponent(id)}`, {
      base_version: baseVersion,
      ...input,
      target: input.target === null ? null : { ...input.target },
    });
    if (status === 409 && isRecord(json) && json.current !== undefined)
      return { kind: 'conflict', current: parseTimeEntry(rec(json, 'conflict').current) };
    if (status !== 200) fail(status, json);
    return { kind: 'updated', entry: parseTimeEntry(json) };
  },
  deleteTimeEntry: (id: string) =>
    expectOk('DELETE', `/api/time-entries/${encodeURIComponent(id)}`, none),
  timeEntryRevisions: (id: string) =>
    expectOk('GET', `/api/time-entries/${encodeURIComponent(id)}/revisions`, (v) =>
      parseList(v, (r) => {
        const o = rec(r, 'revision');
        return {
          started_at: str(o, 'started_at'),
          ended_at: optStr(o, 'ended_at'),
          paused_seconds: int(o, 'paused_seconds'),
          kind: timeKind(str(o, 'kind')),
          memo: str(o, 'memo'),
          changed_at: str(o, 'changed_at'),
        };
      }),
    ),
  timeSummary: (from: string, to: string, tz: string, group: 'day' | 'week' | 'month') =>
    expectOk(
      'GET',
      `/api/me/time-summary?from=${from}&to=${to}&tz=${encodeURIComponent(tz)}&group=${group}`,
      (v) =>
        parseList(v, (r): TimeSummaryRowView => {
          const o = rec(r, 'summary row');
          return {
            period: str(o, 'period'),
            kind: timeKind(str(o, 'kind')),
            seconds: int(o, 'seconds'),
          };
        }),
    ),
  timeSharing: (workspaceId: string) =>
    expectOk('GET', `/api/workspaces/${encodeURIComponent(workspaceId)}/time-sharing`, (v) =>
      bool(rec(v, 'sharing'), 'enabled'),
    ),
  setTimeSharing: (workspaceId: string, enabled: boolean) =>
    expectOk(
      'PUT',
      `/api/workspaces/${encodeURIComponent(workspaceId)}/time-sharing`,
      (v) => bool(rec(v, 'sharing'), 'enabled'),
      { enabled },
    ),
  taskTime: (taskId: string) =>
    expectOk('GET', `/api/tasks/${encodeURIComponent(taskId)}/time`, (v): TaskTimeView => {
      const o = rec(v, 'task time');
      return {
        estimate_minutes: optInt(o, 'estimate_minutes'),
        mine_seconds: int(o, 'mine_seconds'),
        shared_seconds: int(o, 'shared_seconds'),
        shared_people: int(o, 'shared_people'),
      };
    }),
  calendar: (workspaceId: string, from: string, to: string, tz: string) =>
    expectOk(
      'GET',
      `/api/workspaces/${encodeURIComponent(workspaceId)}/calendar?from=${from}&to=${to}&tz=${encodeURIComponent(tz)}`,
      parseCalendar,
    ),
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
  /** 옛 리비전 내용으로 새 리비전을 만든다. 보고 있던 리비전이 지났으면 `{ kind: 'conflict', current }`. */
  async restoreRevision(
    id: string,
    revision: number,
    input: { requestId: string; baseRevision: number },
  ): Promise<
    { kind: 'restored'; document: DocumentView } | { kind: 'conflict'; current: DocumentView }
  > {
    const { status, json } = await request(
      'POST',
      `/api/documents/${encodeURIComponent(id)}/revisions/${String(revision)}/restore`,
      { request_id: input.requestId, base_revision: input.baseRevision },
    );
    if (status === 409 && isRecord(json) && json.current !== undefined) {
      return { kind: 'conflict', current: parseDocument(rec(json, 'conflict').current) };
    }
    if (status !== 200) fail(status, json);
    return { kind: 'restored', document: parseDocument(json) };
  },
  /** 통합 검색. 볼 수 있는 것만, 최대 20건. */
  search: (q: string, workspaceId: string | null) => {
    const params = new URLSearchParams({ q });
    if (workspaceId !== null) params.set('workspace_id', workspaceId);
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
  createItemDocument: (
    itemId: string,
    requestId: string,
    title: string,
    parentId: string | null = null,
  ) =>
    expectOk('POST', `/api/items/${encodeURIComponent(itemId)}/documents`, parseDocument, {
      request_id: requestId,
      title,
      parent_id: parentId,
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
    if (status === 409 && isRecord(json) && json.current !== undefined) {
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
